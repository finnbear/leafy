#!/usr/bin/env python3
"""calls.py FILE.wat REGEX: instruction and call counts over functions whose
name matches REGEX."""
import re, sys, collections
lines = open(sys.argv[1]).read().split('\n')
pat = re.compile(sys.argv[2])
calls = collections.Counter(); n = 0; instrs = 0; on = False
for l in lines:
    if l.startswith('  (func '):
        on = bool(pat.search(l)); n += on
        continue
    if not on:
        continue
    t = l.strip(); instrs += 1
    if t.startswith("call $"):
        nm = re.sub(r'\[[0-9a-f]+\]', '', re.search(r'call \$"?([^"]*)', t).group(1))
        for _ in range(5):
            nm = re.sub(r'<[^<>]*>', '', nm)
        calls[nm[-70:]] += 1
print(n, 'functions', instrs, 'instructions')
for k, v in calls.most_common(int(sys.argv[3]) if len(sys.argv) > 3 else 40):
    print(v, k)
