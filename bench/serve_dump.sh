#!/bin/bash
# Usage: serve_dump.sh DIR [BUDGET_MS] — serves DIR and prints the DOM after
# headless Chromium has run the page for BUDGET_MS of virtual time.
DIR=$1; BUDGET=${2:-3000}
PORT=$((20000 + RANDOM % 20000))
python3 -m http.server $PORT --directory "$DIR" >/dev/null 2>&1 &
PID=$!
sleep 0.5
timeout 60 "${CHROME:-chromium}" --enable-logging=stderr --v=0 --headless=new --no-sandbox --disable-gpu --virtual-time-budget=$BUDGET --dump-dom "http://127.0.0.1:$PORT/" 2>/tmp/chrome_err.log
kill $PID
