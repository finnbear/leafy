#!/bin/bash
# Usage: shot.sh DIR OUT_PREFIX [BUDGET_MS] — serves DIR; saves a screenshot
# (OUT_PREFIX.png), the DOM (OUT_PREFIX.html) and console (OUT_PREFIX.log).
DIR=$1; OUT=$2; BUDGET=${3:-5000}
PORT=$((20000 + RANDOM % 20000))
python3 -m http.server $PORT --directory "$DIR" >/dev/null 2>&1 &
PID=$!
sleep 0.5
FLAGS="--headless=new --no-sandbox --enable-unsafe-swiftshader --window-size=1280,720 --virtual-time-budget=$BUDGET --enable-logging=stderr --v=0"
timeout 90 chromium $FLAGS --screenshot=$OUT.png "http://127.0.0.1:$PORT/" 2>/dev/null >/dev/null
timeout 90 chromium $FLAGS --dump-dom "http://127.0.0.1:$PORT/" 2>$OUT.log >$OUT.html
kill $PID
