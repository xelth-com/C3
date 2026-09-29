"""Round 2 of the tokenizer fingerprint: more public tokenizers, and Kimi's tiktoken file."""
import importlib.util
import io
import os
import re
import sys
import urllib.request

import tiktoken
from tiktoken.load import load_tiktoken_bpe
from tokenizers import Tokenizer

here = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("round1", os.path.join(here, "tokfinger.py"))

REPO = sys.argv[1]
HAND = REPO + "/.collab/openrouter-live/handoffs"
src = io.open(os.path.join(here, "tokfinger.py"), encoding="utf-8").read()
ns = {}
exec(src[src.index("def rust_literal"):src.index("system = (")], {"io": io, "re": re}, ns)  # the literal decoder only
rust_literal = ns["rust_literal"]

system = (rust_literal(REPO + "/crates/c3/src/consult/prompt.rs", "FINAL_OUTPUT_CONTRACT")
          + "\n\n" + rust_literal(REPO + "/crates/c3/src/pack/reviewer.rs", "REPLY_SCHEMA"))
p1 = io.open(HAND + "/01-http-reply.pack.md", encoding="utf-8", newline="").read()
p4 = io.open(HAND + "/04-http-reply.pack.md", encoding="utf-8", newline="").read()

rows = []


def add(name, count):
    a, b, s = count(p1), count(p4), count(system)
    rows.append((abs((12564 - a) - (13126 - b)), name, a, b, s, 12564 - a, 13126 - b))


for repo in ["xai-org/grok-2", "stepfun-ai/Step-3.7-Flash", "stepfun-ai/Step-3.5-Flash",
             "unsloth/Llama-4-Scout-17B-16E-Instruct", "unsloth/gemma-4-12B-it", "unsloth/gemma-3n-E4B-it",
             "CohereLabs/command-a-plus-05-2026-w4a4", "CohereLabs/North-Mini-Code-1.0-eagle",
             "baidu/ERNIE-4.5-21B-A3B-PT", "microsoft/Phi-4-mini-instruct", "ibm-granite/granite-4.0-h-small",
             "MiniMaxAI/MiniMax-M2", "MiniMaxAI/MiniMax-M2.5", "inclusionAI/Ling-1T", "tencent/Hunyuan-A13B-Instruct",
             "LiquidAI/LFM2-8B-A1B", "upstage/solar-pro-preview-instruct", "rednote-hilab/dots.llm1.inst"]:
    try:
        tok = Tokenizer.from_pretrained(repo)
        add(repo, lambda s, tok=tok: len(tok.encode(s, add_special_tokens=False).ids))
    except Exception as e:  # noqa: BLE001
        print("  skip %-46s %s" % (repo, str(e).splitlines()[0][:60]))

# Kimi ships a tiktoken rank file; its split pattern is approximated by the cl100k one.
for repo in ["moonshotai/Kimi-K3", "moonshotai/Kimi-K2.6", "moonshotai/Kimi-K2-Instruct"]:
    try:
        url = "https://huggingface.co/%s/resolve/main/tiktoken.model" % repo
        path = os.path.join(here, repo.replace("/", "_") + ".tiktoken")
        if not os.path.exists(path):
            urllib.request.urlretrieve(url, path)
        ranks = load_tiktoken_bpe(path)
        pat = tiktoken.get_encoding("cl100k_base")._pat_str
        enc = tiktoken.Encoding(name=repo, pat_str=pat, mergeable_ranks=ranks, special_tokens={})
        add(repo + " (tiktoken file)", lambda s, enc=enc: len(enc.encode(s, disallowed_special=())))
    except Exception as e:  # noqa: BLE001
        print("  skip %-46s %s" % (repo, str(e).splitlines()[0][:60]))

rows.sort()
print("\n%-50s %7s %7s %7s | %6s %6s %6s" % ("tokenizer", "pack1", "pack4", "system", "S1", "S4", "S1-S4"))
for d, name, a, b, s, s1, s4 in rows:
    print("%-50s %7d %7d %7d | %6d %6d %+6d" % (name[:50], a, b, s, s1, s4, s1 - s4))
print("\nthe real tokenizer: S1 = S4, both a little above its own 'system' count")
