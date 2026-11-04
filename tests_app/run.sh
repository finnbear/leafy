#!/bin/bash
# Builds the browser tests and runs them in headless Chromium (`$CHROME`,
# default `chromium`). Exits nonzero unless every test passes.
set -e
cd "$(dirname "$0")"
trunk build --skip-version-check --no-sri >/tmp/leafy_tests_build.log 2>&1 || { grep -E "^(error|warning)" -A15 /tmp/leafy_tests_build.log | head -80; exit 1; }
title=$(../bench/serve_dump.sh dist 20000 | grep -o "<title>[^<]*</title>" || true)
echo "$title"
grep -o '"\(PASS\|FAIL\|PANIC\)[^"]*"' /tmp/chrome_err.log || grep CONSOLE /tmp/chrome_err.log | head
[[ "$title" =~ ^\<title\>done:\ [0-9]+\ passed,\ 0\ failed\</title\>$ ]]
