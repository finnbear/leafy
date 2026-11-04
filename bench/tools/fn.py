#!/usr/bin/env python3
"""fn.py FILE.wat SUBSTRING: a condensed listing of the first function whose
name contains SUBSTRING, split at calls."""
import re, sys
wat, sub = sys.argv[1], sys.argv[2]
lines = open(wat).read().split('\n')
start = next(i for i, l in enumerate(lines) if l.startswith('  (func ') and sub in l)
out = []
for l in lines[start + 1:]:
    if l.startswith('  (func ') or l.startswith('  (data') or l.startswith('  (elem'):
        break
    l = l.strip()
    if l == 'end' or l.startswith('block') or l.startswith('loop'):
        continue
    l = re.sub(r'\$"[^"]*::([A-Za-z_0-9<>]+)"', r'$\1', l)
    out.append(l)
print(lines[start][:200])
s = re.sub(r'local\.(get|set|tee) [0-9]+', 'L', ' ; '.join(out))
for part in s.split('call '):
    print('call ' + part[:600])
