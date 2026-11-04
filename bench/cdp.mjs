// Minimal Chrome DevTools Protocol driver (Node 24, no dependencies).
//
// Usage: node cdp.mjs URL OUT_PREFIX [STEPS_JSON]
//
// Opens URL in headless Chromium and runs STEPS (default: wait 6s, then
// screenshot). Steps: {"wait": ms}, {"shot": "suffix"}, {"eval": "js"},
// {"click": [x, y]}. Console messages and eval results go to stdout.
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const [url, out, stepsJson] = process.argv.slice(2);
const steps = stepsJson ? JSON.parse(stepsJson) : [{ wait: 6000 }, { shot: "" }];
const port = 9300 + Math.floor(Math.random() * 500);
const chrome = spawn("chromium", [
  "--headless=new", "--no-sandbox", "--enable-unsafe-swiftshader",
  "--window-size=1280,720", `--remote-debugging-port=${port}`, "about:blank",
], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let targets;
for (let i = 0; i < 50; i++) {
  try {
    targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    if (targets.some((t) => t.type === "page")) break;
  } catch {}
  await sleep(100);
}
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  } else if (msg.method === "Runtime.consoleAPICalled") {
    console.log("console:", msg.params.args.map((a) => a.value ?? a.description).join(" "));
  } else if (msg.method === "Runtime.exceptionThrown") {
    console.log("exception:", msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
  }
});
const send = (method, params = {}) =>
  new Promise((r) => {
    const i = ++id;
    pending.set(i, r);
    ws.send(JSON.stringify({ id: i, method, params }));
  });

await send("Runtime.enable");
await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
await send("Page.navigate", { url });
for (const step of steps) {
  if (step.wait) await sleep(step.wait);
  if (step.shot !== undefined) {
    const r = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(`${out}${step.shot}.png`, Buffer.from(r.result.data, "base64"));
  }
  if (step.eval) {
    const r = await send("Runtime.evaluate", { expression: step.eval, returnByValue: true });
    console.log("eval:", JSON.stringify(r.result?.result?.value ?? r.result));
  }
  if (step.metrics) {
    await send("Performance.enable");
    const r = await send("Performance.getMetrics");
    const m = Object.fromEntries(r.result.metrics.map((x) => [x.name, x.value]));
    console.log(`metrics ${step.metrics}:`, JSON.stringify({ script: m.ScriptDuration, task: m.TaskDuration, layout: m.LayoutCount, style: m.RecalcStyleCount, nodes: m.Nodes, heap: m.JSHeapUsedSize }));
  }
  if (step.click) {
    const [x, y] = step.click;
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
    }
  }
}
ws.close();
chrome.kill();
process.exit(0);
