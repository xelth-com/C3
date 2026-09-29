"""Tokenizer fingerprint of a stealth model, from token counts the provider reported.

Known: two requests to `stealth/space-bunny-alpha` with the SAME system message and different
user messages (the packs), and one request to Nemotron with pack 1. For the model's real
tokenizer T:

    T(system) + T(pack1) + framing = 12564
    T(system) + T(pack4) + framing = 13126

so  S1 = 12564 - T(pack1)  and  S4 = 13126 - T(pack4)  must be EQUAL, and both must be a little
more than T(system) (the chat template adds a handful of tokens). Everything is computed locally
from public tokenizer files; nothing about the packs is sent anywhere.
"""
import io
import json
import re
import sys
import urllib.request

from tokenizers import Tokenizer

REPO = sys.argv[1]
HAND = REPO + "/.collab/openrouter-live/handoffs"


def rust_literal(path, name):
    """Decode the Rust string literal assigned to `name` in `path`."""
    src = io.open(path, encoding="utf-8").read()
    m = re.search(r"const\s+" + re.escape(name) + r"\s*:\s*&str\s*=\s*\"", src)
    i = m.end()
    out = []
    while True:
        c = src[i]
        if c == '"':
            break
        if c == "\\":
            n = src[i + 1]
            if n == "n":
                out.append("\n"); i += 2
            elif n == "t":
                out.append("\t"); i += 2
            elif n == "r":
                out.append("\r"); i += 2
            elif n in "\"'\\":
                out.append(n); i += 2
            elif n == "0":
                out.append("\0"); i += 2
            elif n == "u":
                j = src.index("}", i)
                out.append(chr(int(src[i + 3:j], 16))); i = j + 1
            elif n in "\r\n":
                i += 1
                while src[i] in " \t\r\n":
                    i += 1
            else:
                out.append(n); i += 2
        else:
            out.append(c); i += 1
    return "".join(out).replace("\r\n", "\n")


system = (rust_literal(REPO + "/crates/c3/src/consult/prompt.rs", "FINAL_OUTPUT_CONTRACT")
          + "\n\n" + rust_literal(REPO + "/crates/c3/src/pack/reviewer.rs", "REPLY_SCHEMA"))
p1 = io.open(HAND + "/01-http-reply.pack.md", encoding="utf-8", newline="").read()
p4 = io.open(HAND + "/04-http-reply.pack.md", encoding="utf-8", newline="").read()
print("system message: %d characters; packs: %d and %d characters" % (len(system), len(p1), len(p4)))


def latest(author, n, must=None):
    url = "https://huggingface.co/api/models?author=%s&sort=lastModified&direction=-1&limit=60" % author
    try:
        data = json.load(urllib.request.urlopen(url, timeout=30))
    except Exception as e:  # noqa: BLE001
        print("  (listing %s failed: %s)" % (author, e))
        return []
    ids = [m["id"] for m in data if not m.get("gated")]
    skip = re.compile(r"(?i)ocr|asr|tts|image|video|vae|embed|rerank|guard|safety|audio|speech|vision|-V-|V-FP8|4\.\dV|gguf|awq|fp8|bf16|int4|mlx|onnx|scail|kaleido|glyph|webvia|ui2code|autoglm|realvideo|cosmos|parakeet|canary|segformer|radio|eagle|bevformer")
    ids = [i for i in ids if not skip.search(i)]
    if must:
        ids = [i for i in ids if re.search(must, i, re.I)]
    return ids[:n]


cands = []
cands += latest("zai-org", 7)
cands += latest("nvidia", 4, must="nemotron")
cands += latest("Qwen", 3, must=r"qwen3|qwen4")
cands += latest("deepseek-ai", 3, must=r"deepseek-v|deepseek-r")
cands += latest("moonshotai", 3)
cands += latest("MiniMaxAI", 2)
cands += latest("XiaomiMiMo", 2)
cands += ["unsloth/gemma-3-27b-it", "unsloth/Llama-3.3-70B-Instruct", "unsloth/Mistral-Small-3.2-24B-Instruct-2506",
          "Xenova/claude-tokenizer", "Xenova/grok-1-tokenizer", "openai/gpt-oss-120b"]
seen = set()
cands = [c for c in cands if not (c in seen or seen.add(c))]

rows = []
for repo in cands:
    try:
        tok = Tokenizer.from_pretrained(repo)
    except Exception as e:  # noqa: BLE001
        print("  skip %-52s %s" % (repo, str(e).splitlines()[0][:70]))
        continue
    n = lambda s: len(tok.encode(s, add_special_tokens=False).ids)  # noqa: E731
    a, b, s = n(p1), n(p4), n(system)
    s1, s4 = 12564 - a, 13126 - b
    rows.append((abs(s1 - s4), repo, a, b, s, s1, s4, 14470 - a - s))

rows.sort()
print("\n%-50s %7s %7s %7s | %6s %6s %6s | %8s | %s" % (
    "tokenizer", "pack1", "pack4", "system", "S1", "S4", "S1-S4", "framing", "nemotron framing"))
for d, repo, a, b, s, s1, s4, nf in rows:
    frame = (s1 - s) if s1 == s4 else None
    print("%-50s %7d %7d %7d | %6d %6d %+6d | %8s | %+d" % (
        repo[:50], a, b, s, s1, s4, s1 - s4, ("%+d" % frame) if frame is not None else "-", nf))
print("\nread it like this: the real tokenizer has S1-S4 = 0 and a small positive framing;")
print("the last column is the same test for Nemotron's one run (its real tokenizer: small positive).")
