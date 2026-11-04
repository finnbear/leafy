// Times interactions with a benchmark app built against yew and against leafy,
// in headless Chromium, through the Chrome DevTools Protocol (Node 24; no
// dependencies).
//
// Usage: node speed.mjs APP [RUNS=10] [PROFILE=speed]
//   APP: jsfb | jsfb_struct | todomvc | todomvc_struct | life | synthetic-N
//   (built by `build.sh APP yew PROFILE [N]` and
//   `build.sh APP leafy PROFILE [N]`)
//
// Also writes the medians to out/speed-APP-PROFILE.json (for plot.mjs).
//
// Each run loads the app afresh and performs the app's operations in order.
// An operation is timed from triggering it to the DOM being updated and laid
// out: after a task boundary (both libraries render in a microtask) and a
// forced layout; painting isn't included. The first run is a warm-up. Prints
// the median of each operation for each library, their ratio, and whether the
// DOM after each operation matched between the libraries. `$CHROME` picks
// the browser (default `chromium`).
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { join, extname } from "node:path";
import { tmpdir } from "node:os";

const [app, runsArg, profileArg] = process.argv.slice(2);
const PROFILE = profileArg ?? "speed";
const RUNS = Number(runsArg ?? 10);
const here = new URL(".", import.meta.url).pathname;

// In the page: helpers, then each app's operations. An operation returns
// what to compare between the libraries.
const HELPERS = `
window.__yield = () => new Promise((r) => {
  const c = new MessageChannel();
  c.port1.onmessage = () => r();
  c.port2.postMessage(0);
});
window.__time = async (f) => {
  const t0 = performance.now();
  f();
  await __yield();
  document.body.offsetHeight;
  return performance.now() - t0;
};
window.__click = (sel) => document.querySelector(sel).click();
`;

const APPS = {
  life: {
    ready: "!!document.querySelector('.game-button')",
    // (The board is random, so only its size is compared.)
    state: "document.querySelectorAll('.game-cellule').length + ' cells'",
    ops: [
      ["randomize", "__click('.game-buttons .game-button:nth-child(1)')"],
      ["step 20 times (each rendered)", null, `(async () => {
        let t = 0;
        for (let i = 0; i < 20; i++) t += await __time(() => __click('.game-buttons .game-button:nth-child(2)'));
        return t;
      })()`],
      ["reset", "__click('.game-buttons .game-button:nth-child(5)')"],
    ],
  },
  synthetic: {
    ready: "!!document.querySelector('#frame')",
    state: "document.body.querySelectorAll('*').length + ' elements'",
    ops: [
      ["20 frames (each rendered)", null, `(async () => {
        let t = 0;
        for (let i = 0; i < 20; i++) t += await __time(() => __click('#frame'));
        return t;
      })()`],
    ],
  },
  jsfb: {
    ready: "!!document.querySelector('#run')",
    state: "(() => { const rows = document.querySelectorAll('tbody tr'); return rows.length + ' rows, first ' + (rows[0]?.firstElementChild.textContent ?? '-') + ', selected ' + document.querySelectorAll('tbody tr.danger').length; })()",
    ops: [
      ["create 1,000 rows", "__click('#run')"],
      ["replace 1,000 rows", "__click('#run')"],
      ["update every 10th row", "__click('#update')"],
      ["select row", "document.querySelector('tbody tr:nth-child(2) td:nth-child(2)').click()"],
      ["swap rows", "__click('#swaprows')"],
      ["remove row", "document.querySelector('tbody tr:nth-child(2) a.remove').click()"],
      ["clear 1,000 rows", "__click('#clear')"],
      ["create 10,000 rows", "__click('#runlots')"],
      ["clear 10,000 rows", "__click('#clear')"],
      ["create 1,000 rows (again)", "__click('#run')"],
      ["append 1,000 rows", "__click('#add')"],
    ],
  },
  todomvc: {
    ready: "!!document.querySelector('.new-todo')",
    before: "localStorage.clear()",
    state: "(() => document.querySelectorAll('.todo-list li').length + ' items, ' + document.querySelectorAll('.todo-list li.completed').length + ' completed, count \"' + (document.querySelector('.todo-count')?.textContent ?? '') + '\"')()",
    ops: [
      // 100 separate additions (each rendered), timed together.
      ["add 100 todos", null, `(async () => {
        let t = 0;
        for (let i = 0; i < 100; i++) {
          const input = document.querySelector('.new-todo');
          t += await __time(() => {
            input.value = 'todo ' + i;
            input.dispatchEvent(new KeyboardEvent('keypress', { key: 'Enter', bubbles: true }));
          });
        }
        return t;
      })()`],
      ["toggle one", "__click('.todo-list li:nth-child(3) .toggle')"],
      ["toggle all", "__click('.toggle-all')"],
      ["toggle all back", "__click('.toggle-all')"],
      ["toggle 10", null, `(async () => {
        let t = 0;
        for (let i = 1; i <= 10; i++) t += await __time(() => __click('.todo-list li:nth-child(' + (i * 2) + ') .toggle'));
        return t;
      })()`],
      ["filter active", "__click('.filters li:nth-child(2) a')"],
      ["filter completed", "__click('.filters li:nth-child(3) a')"],
      ["filter all", "__click('.filters li:nth-child(1) a')"],
      ["remove one", "__click('.todo-list li:nth-child(1) .destroy')"],
      ["clear completed", "__click('.clear-completed')"],
    ],
  },
};

// The struct-component versions are the same apps.
APPS.jsfb_struct = APPS.jsfb;
APPS.todomvc_struct = APPS.todomvc;

// The synthetic app, as built with N components (`synthetic-N`).
const spec = APPS[app.replace(/^synthetic-\d+$/, "synthetic")];
if (!spec) {
  console.error("usage: node speed.mjs jsfb|jsfb_struct|todomvc|todomvc_struct|life|synthetic-N [RUNS] [PROFILE]");
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css" };

function serve(dir) {
  const server = createServer((req, res) => {
    let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (path.endsWith("/")) path += "index.html";
    const file = join(dir, path);
    if (!file.startsWith(dir) || !existsSync(file)) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
    res.end(readFileSync(file));
  });
  return new Promise((r) => server.listen(0, "127.0.0.1", () => r(server)));
}

async function browser() {
  const profile = mkdtempSync(join(tmpdir(), "bench-chrome-"));
  const port = 9300 + Math.floor(Math.random() * 500);
  const chrome = spawn(process.env.CHROME ?? "chromium", [
    "--headless=new", "--no-sandbox", `--user-data-dir=${profile}`, "--window-size=1280,720",
    `--remote-debugging-port=${port}`, "about:blank",
  ], { stdio: "ignore" });
  let page;
  for (let i = 0; i < 100 && !page; i++) {
    try {
      page = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page");
    } catch {}
    await sleep(100);
  }
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r));
  let id = 0;
  const pending = new Map();
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    } else if (msg.method === "Runtime.exceptionThrown") {
      console.error("exception:", msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
    }
  });
  const send = (method, params = {}) =>
    new Promise((r) => {
      const i = ++id;
      pending.set(i, r);
      ws.send(JSON.stringify({ id: i, method, params }));
    });
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description ?? JSON.stringify(r.result.exceptionDetails));
    return r.result?.result?.value;
  };
  await send("Runtime.enable");
  await send("Page.enable");
  return {
    evaluate,
    navigate: (url) => send("Page.navigate", { url }),
    close: () => {
      ws.close();
      chrome.kill();
      setTimeout(() => rmSync(profile, { recursive: true, force: true }), 500);
    },
  };
}

async function measure(lib) {
  const dist = join(here, "out", `${app}-${lib}-${PROFILE}`, "dist");
  if (!existsSync(dist)) throw new Error(`${dist} missing: run ./build.sh ${app} ${lib} ${PROFILE}`);
  const server = await serve(dist);
  const url = `http://127.0.0.1:${server.address().port}/`;
  const b = await browser();
  const times = spec.ops.map(() => []);
  let states;
  for (let run = 0; run <= RUNS; run++) {
    if (spec.before) {
      await b.navigate(url);
      await sleep(300);
      await b.evaluate(spec.before);
    }
    await b.navigate(url);
    for (let i = 0; !(await b.evaluate(spec.ready).catch(() => false)); i++) {
      if (i > 100) throw new Error(`${lib}: app didn't start`);
      await sleep(50);
    }
    await b.evaluate(HELPERS);
    const s = [];
    for (const [i, [, op, custom]] of spec.ops.entries()) {
      const t = await b.evaluate(custom ?? `__time(() => { ${op}; })`);
      if (run > 0) times[i].push(t);
      s.push(await b.evaluate(spec.state));
    }
    states = s;
  }
  b.close();
  server.close();
  return { times, states };
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

const results = { yew: await measure("yew"), leafy: await measure("leafy") };
console.log(`${app} (${PROFILE} profile): median of ${RUNS} runs (ms), headless Chromium`);
console.log(`${"operation".padEnd(28)} ${"yew".padStart(8)} ${"leafy".padStart(8)}  leafy/yew  same DOM`);
let totals = [0, 0];
for (const [i, [name]] of spec.ops.entries()) {
  const a = median(results.yew.times[i]);
  const b = median(results.leafy.times[i]);
  totals[0] += a;
  totals[1] += b;
  const same = results.yew.states[i] === results.leafy.states[i] ? "yes" : `NO: ${results.yew.states[i]} | ${results.leafy.states[i]}`;
  console.log(`${name.padEnd(28)} ${a.toFixed(2).padStart(8)} ${b.toFixed(2).padStart(8)}  ${(b / a).toFixed(2).padStart(8)}  ${same}`);
}
console.log(`${"total".padEnd(28)} ${totals[0].toFixed(2).padStart(8)} ${totals[1].toFixed(2).padStart(8)}  ${(totals[1] / totals[0]).toFixed(2).padStart(8)}`);
writeFileSync(
  join(here, "out", `speed-${app}-${PROFILE}.json`),
  JSON.stringify({
    app,
    profile: PROFILE,
    runs: RUNS,
    ops: spec.ops.map(([name], i) => ({ name, yew: median(results.yew.times[i]), leafy: median(results.leafy.times[i]) })),
    total: { yew: totals[0], leafy: totals[1] },
  }, null, 1),
);
process.exit(0);
