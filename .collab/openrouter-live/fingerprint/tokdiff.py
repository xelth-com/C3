"""Differential tokenizer fingerprint.

Two requests went to the same model with the same system message and different packs. Whatever
the system message and the chat framing cost, they cancel in the difference, so for the model's
real tokenizer  tokens(pack4) - tokens(pack1)  must equal  input4 - input1  (within a token or two
at the seams). Nothing is sent anywhere: the packs are tokenised locally.
"""
import io
import sys

import tiktoken

base = sys.argv[1]
p1 = io.open(base + "/01-http-reply.pack.md", encoding="utf-8").read()
p4 = io.open(base + "/04-http-reply.pack.md", encoding="utf-8").read()

observed = {"space-bunny": (12564, 13126), "nemotron (pack 1 only)": (14470, None)}
want = observed["space-bunny"][1] - observed["space-bunny"][0]
print("observed by the provider: run 1 = 12564, run 4 = 13126, difference = %d" % want)
print("pack sizes: %d and %d characters\n" % (len(p1), len(p4)))

print("%-16s %9s %9s %9s %9s   %s" % ("tokenizer", "pack1", "pack4", "diff", "off by", "system+framing implied"))
for name in ("o200k_base", "cl100k_base", "p50k_base", "gpt2"):
    try:
        enc = tiktoken.get_encoding(name)
    except Exception as e:  # noqa: BLE001
        print("%-16s unavailable: %s" % (name, e))
        continue
    a = len(enc.encode(p1, disallowed_special=()))
    b = len(enc.encode(p4, disallowed_special=()))
    print("%-16s %9d %9d %9d %+9d   %d and %d" % (name, a, b, b - a, (b - a) - want, 12564 - a, 13126 - b))

print("\nfor scale: characters per token as the provider counted, if the pack were the whole prompt:")
print("  space-bunny run 1: %.2f   nemotron run 3: %.2f" % (len(p1) / 12564.0, len(p1) / 14470.0))
