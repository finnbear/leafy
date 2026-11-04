#!/usr/bin/env python3
"""ui_report.py WASM [CRATE_REGEX] [N]

Attributes a wasm's code (built with names, e.g. trunk's `--debuginfo`
wasm-opt pass, before stripping) to the source functions it came from: a
component's render function together with its closures, its properties'
comparison and drop glue, and anything instantiated for it. Prints the N
heaviest (default 40), to find where a UI's bytes go.

CRATE_REGEX selects the crates to report (default: all but the standard
library's, the UI library's and the bindings').
"""
import collections
import csv
import io
import re
import subprocess
import sys

wasm = sys.argv[1]
crates = re.compile(
    sys.argv[2]
    if len(sys.argv) > 2
    else r"^(?!(std|core|alloc|dlmalloc|hashbrown|leafy\w*|yew\w*|wasm_bindgen\w*|js_sys|web_sys)$)"
)
n = int(sys.argv[3]) if len(sys.argv) > 3 else 40

out = subprocess.check_output(["twiggy", "top", "-n", "1000000", "-f", "csv", wasm], text=True)
rows = [(r[0], int(r[1])) for r in csv.reader(io.StringIO(out)) if len(r) > 1 and r[1].isdigit()]


def strip_generics(s):
    s = re.sub(r"\[[0-9a-f]+\]", "", s)
    for _ in range(10):
        s = re.sub(r"<[^<>]*>", "", s)
    return s


def owner(name):
    """The user-level path a function belongs to, or None."""
    # Component machinery: PropsOf<path::Comp> ... -> path::Comp (component).
    m = re.search(r"PropsOf<([a-z_]+)\[[0-9a-f]+\]::([A-Za-z0-9_:]+)", name)
    if m:
        return m.group(1), m.group(2).split("<")[0] + " (component)"
    # Closures and functions: crate[hash]::path::fn::{closure#..} -> path::fn.
    m = re.search(r"(?:^|<)([a-z_]+)\[[0-9a-f]+\]::([A-Za-z0-9_:]+?)(?:::\{closure|::<|>| as |$)", name)
    if m:
        return m.group(1), m.group(2)
    return None


totals = collections.Counter()
counts = collections.Counter()
for name, size in rows:
    if name.startswith("data") or "subsection" in name:
        continue
    # Attribute instantiations (e.g. leafy::callback::call::<.., closure>) to
    # the innermost user crate mentioned.
    candidates = [owner(m.group(0)) for m in re.finditer(r"(?:PropsOf<)?[a-z_]+\[[0-9a-f]+\]::[A-Za-z0-9_:]+", name)]
    candidates = [c for c in candidates if c and crates.search(c[0])]
    if not candidates:
        continue
    crate, path = candidates[-1] if "PropsOf<" not in name else candidates[0]
    key = f"{crate}::{path}"
    totals[key] += size
    counts[key] += 1

grand = sum(totals.values())
print(f"{grand} bytes in {sum(counts.values())} functions attributed to {len(totals)} source items")
for key, size in totals.most_common(n):
    print(f"{size:>8} {counts[key]:>4} fns  {key}")
