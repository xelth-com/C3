"""Two tiny probes: known user texts, no system message; only `usage.prompt_tokens` is read.

The key is taken from the environment variable OPENROUTER_API_KEY of this process and is sent only
in the Authorization header to openrouter.ai. It is never printed and never written.
"""
import io
import json
import os
import sys
import urllib.request

from tokenizers import Tokenizer

REPO = sys.argv[1]
MODEL = sys.argv[2]
HAND = REPO + "/.collab/openrouter-live/handoffs"
key = os.environ.get("OPENROUTER_API_KEY", "")
if not key:
    sys.exit("no key in the environment")

p1 = io.open(HAND + "/01-http-reply.pack.md", encoding="utf-8", newline="").read()
p4 = io.open(HAND + "/04-http-reply.pack.md", encoding="utf-8", newline="").read()
probes = [("A", p1[20000:22000]), ("B", p4[30000:34500]), ("C", "ping")]

toks = {}
for name in ("MiniMaxAI/MiniMax-M2.5", "zai-org/GLM-5.3-Flash", "deepseek-ai/DeepSeek-V4.1-Flash", "openai/gpt-oss-120b",
             "CohereLabs/command-a-plus-05-2026-w4a4", "unsloth/Llama-4-Scout-17B-16E-Instruct"):
    try:
        toks[name] = Tokenizer.from_pretrained(name)
    except Exception as e:  # noqa: BLE001
        print("skip", name, str(e).splitlines()[0][:60])

seen = {}
for label, text in probes:
    body = json.dumps({
        "model": MODEL,
        "messages": [{"role": "user", "content": text + "\n\nReply with the single word: ok"}],
        "max_tokens": 64,
        "reasoning": {"effort": "low"},
    }).encode("utf-8")
    req = urllib.request.Request(
        "https://openrouter.ai/api/v1/chat/completions", data=body, method="POST",
        headers={"Authorization": "Bearer " + key, "Content-Type": "application/json", "X-Title": "c3"})
    try:
        with urllib.request.urlopen(req, timeout=180) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except Exception as e:  # noqa: BLE001
        print("probe %s failed: %s" % (label, str(e).replace(key, "[KEY]")[:120]))
        continue
    usage = data.get("usage") or {}
    seen[label] = (usage.get("prompt_tokens"), text + "\n\nReply with the single word: ok")
    print("probe %s: %d characters -> prompt_tokens %s (provider: %s)" % (
        label, len(text), usage.get("prompt_tokens"), data.get("provider")))

if len(seen) >= 2:
    print("\n%-44s %s" % ("tokenizer", "  ".join("%s: own count / hidden" % k for k in seen)))
    for name, tok in toks.items():
        cells, hidden = [], []
        for k, (pt, text) in seen.items():
            n = len(tok.encode(text, add_special_tokens=False).ids)
            cells.append("%5d / %+5d" % (n, pt - n))
            hidden.append(pt - n)
        same = "  <-- the same hidden part in every probe" if len(set(hidden)) == 1 else ""
        print("%-44s %s%s" % (name[:44], "      ".join(cells), same))
    print("\nthe real tokenizer leaves the SAME hidden part (chat framing + the provider's own system text) in every probe")
