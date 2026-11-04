// Plots size against speed for every build: frontier.svg.
//
// Usage: node plot.mjs (after build.sh and speed.mjs for each app, library
// and profile)
//
// A panel per app: a dot per library and profile at its compressed wasm
// size (x) and the total time of the app's operations in speed.mjs (y), on
// linear axes from zero, so bottom-left is best. Color is the library and
// shape the profile; a library's two builds are joined.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const here = new URL(".", import.meta.url).pathname;
const APPS = [
  ["todomvc", "TodoMVC"],
  ["todomvc_struct", "TodoMVC (struct)"],
  ["jsfb", "js-framework-benchmark"],
  ["jsfb_struct", "js-framework-benchmark (struct)"],
  ["life", "Game of Life"],
];
const LIBS = ["leafy", "yew"];
const PROFILES = [
  ["speed", "optimized for speed (opt-level 3, wasm-opt -O4)"],
  ["size", "optimized for size (opt-level \"z\", LTO, wasm-opt -Oz)"],
];

const read = (app, lib, profile) => {
  const speed = JSON.parse(readFileSync(join(here, "out", `speed-${app}-${profile}.json`), "utf8"));
  const sizes = readFileSync(join(here, "out", `${app}-${lib}-${profile}`, "sizes.txt"), "utf8");
  return { bytes: Number(/wasm\.br=(\d+)/.exec(sizes)[1]), ms: speed.total[lib] };
};
const groups = [];
for (const [app, name] of APPS) {
  groups.push({ name, builds: LIBS.flatMap((lib) => PROFILES.map(([profile]) => ({ lib, profile, ...read(app, lib, profile) }))) });
}

const fmtFull = (n) => Math.round(n).toLocaleString("en-US");
const fmtMs = (ms) => (ms >= 1000 ? `${+(ms / 1000).toFixed(2)} s` : ms >= 1 ? `${+ms.toFixed(1)} ms` : `${+(ms * 1000).toFixed(1)} µs`);
const fmtBytes = (b) => (b >= 1000 ? `${+(b / 1000).toFixed(0)} KB` : `${Math.round(b)} B`);
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;");
// A round upper bound for an axis, from its largest value.
const niceMax = (v) => {
  const p = 10 ** Math.floor(Math.log10(v));
  return [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10, 15, 20].map((m) => m * p).find((m) => m >= v * 1.08);
};

const COLS = 3, PW = 210, PH = 150; // plot area of a panel
const GAP_X = 74, GAP_Y = 80, LEFT = 64, TOP = 134;
const W = LEFT + COLS * PW + (COLS - 1) * GAP_X + 56;
const ROWS = Math.ceil(groups.length / COLS);
const H = TOP + ROWS * (PH + GAP_Y) - 8;

const out = [];
out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d" font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">`);
out.push(`<title id="t">Size and speed of each build: leafy and yew</title>`);
out.push(`<desc id="d">${esc(groups.flatMap((g) => g.builds.map((b) => `${g.name}, ${b.lib}, ${b.profile} profile: ${fmtFull(b.bytes)} bytes, ${fmtMs(b.ms)}`)).join("; "))}</desc>`);
out.push(`<style>
  .surface { fill: #fcfcfb; }
  .ink { fill: #0b0b0b; }
  .ink2 { fill: #52514e; }
  .muted { fill: #898781; }
  .grid { stroke: #e1e0d9; stroke-width: 1; }
  .axis { stroke: #c3c2b7; stroke-width: 1; }
  .leafy { fill: #2a78d6; }
  .yew { fill: #eb6834; }
  .line-leafy { stroke: #2a78d6; }
  .line-yew { stroke: #eb6834; }
  .line { stroke-width: 2; fill: none; stroke-linecap: round; }
  .ring { stroke: #fcfcfb; stroke-width: 2; }
  .hit { fill: transparent; }
  .dot:hover .mark { stroke: #0b0b0b; stroke-width: 2; }
  @media (prefers-color-scheme: dark) {
    .surface { fill: #1a1a19; }
    .ink { fill: #f0efec; }
    .ink2 { fill: #c3c2b7; }
    .grid { stroke: #2c2c2a; }
    .axis { stroke: #383835; }
    .leafy { fill: #3987e5; }
    .yew { fill: #d95926; }
    .line-leafy { stroke: #3987e5; }
    .line-yew { stroke: #d95926; }
    .ring { stroke: #1a1a19; }
    .dot:hover .mark { stroke: #f0efec; }
  }
</style>`);
out.push(`<rect class="surface" width="${W}" height="${H}" rx="8"/>`);
out.push(`<text class="ink" x="${LEFT - 40}" y="32" font-size="17" font-weight="600">Size and speed of each build: leafy and yew</text>`);
out.push(`<text class="ink2" x="${LEFT - 40}" y="54" font-size="13">Each app: wasm size (brotli -q 11) against the total time of its benchmark operations; bottom-left is best.</text>`);
out.push(`<text class="ink2" x="${LEFT - 40}" y="72" font-size="13">Each panel's text compares leafy with yew with the same profile (the geometric mean of both profiles).</text>`);

const marker = (cls, profile, cx, cy, r = 6) =>
  profile === "speed"
    ? `<circle class="${cls} ring mark" cx="${cx}" cy="${cy}" r="${r}"/>`
    : `<rect class="${cls} ring mark" x="${cx - r + 1}" y="${cy - r + 1}" width="${2 * r - 2}" height="${2 * r - 2}" transform="rotate(45 ${cx} ${cy})"/>`;

// Legend.
let lx = LEFT - 40;
const ly = 98;
for (const lib of LIBS) {
  out.push(`<line class="line line-${lib}" x1="${lx}" y1="${ly}" x2="${lx + 22}" y2="${ly}"/>`);
  out.push(`<text class="ink2" x="${lx + 30}" y="${ly + 4}" font-size="12">${lib}</text>`);
  lx += 30 + lib.length * 7 + 26;
}
for (const [profile, label] of PROFILES) {
  out.push(marker("muted", profile, lx + 6, ly, 5));
  out.push(`<text class="ink2" x="${lx + 18}" y="${ly + 4}" font-size="12">${esc(label)}</text>`);
  lx += 18 + label.length * 6.3 + 24;
}

groups.forEach((g, i) => {
  const left = LEFT + (i % COLS) * (PW + GAP_X);
  const top = TOP + Math.floor(i / COLS) * (PH + GAP_Y);
  const xMax = niceMax(Math.max(...g.builds.map((p) => p.bytes)));
  const yMax = niceMax(Math.max(...g.builds.map((p) => p.ms)));
  const x = (b) => left + (b / xMax) * PW;
  const y = (ms) => top + PH - (ms / yMax) * PH;
  out.push(`<text class="ink" x="${left}" y="${top - 10}" font-size="12.5" font-weight="600">${esc(g.name)}</text>`);
  for (const t of [0, xMax / 2, xMax]) {
    out.push(`<line class="grid" x1="${x(t)}" y1="${top}" x2="${x(t)}" y2="${top + PH}"/>`);
    out.push(`<text class="muted" x="${x(t)}" y="${top + PH + 16}" font-size="11" text-anchor="${t === 0 ? "start" : t === xMax ? "end" : "middle"}">${t === 0 ? "0" : fmtBytes(t)}</text>`);
  }
  for (const t of [0, yMax / 2, yMax]) {
    out.push(`<line class="grid" x1="${left}" y1="${y(t)}" x2="${left + PW}" y2="${y(t)}"/>`);
    out.push(`<text class="muted" x="${left - 6}" y="${y(t) + 4}" font-size="11" text-anchor="end">${t === 0 ? "0" : fmtMs(t)}</text>`);
  }
  out.push(`<line class="axis" x1="${left}" y1="${top + PH}" x2="${left + PW}" y2="${top + PH}"/>`);
  out.push(`<line class="axis" x1="${left}" y1="${top}" x2="${left}" y2="${top + PH}"/>`);
  for (const lib of LIBS) {
    const [a, b] = g.builds.filter((p) => p.lib === lib);
    out.push(`<line class="line line-${lib}" x1="${x(a.bytes).toFixed(1)}" y1="${y(a.ms).toFixed(1)}" x2="${x(b.bytes).toFixed(1)}" y2="${y(b.ms).toFixed(1)}"/>`);
  }
  // The takeaway: leafy against yew with the same profile (the geometric
  // mean over the profiles), in the corner no build is in.
  const ratio = (key) => {
    const rs = PROFILES.map(([profile]) => {
      const [l, w] = LIBS.map((lib) => g.builds.find((p) => p.lib === lib && p.profile === profile));
      return w[key] / l[key];
    });
    return rs.reduce((a, b) => a * b, 1) ** (1 / rs.length);
  };
  out.push(`<text class="ink" x="${left + PW - 6}" y="${top + PH - 8}" font-size="12.5" font-weight="600" text-anchor="end">${ratio("bytes").toFixed(1)}× smaller, ${ratio("ms").toFixed(1)}× faster</text>`);
  for (const p of g.builds) {
    const cx = +x(p.bytes).toFixed(1), cy = +y(p.ms).toFixed(1);
    out.push(`<g class="dot"><title>${esc(`${g.name}, ${p.lib}, ${p.profile === "speed" ? "optimized for speed" : "optimized for size"}: ${fmtFull(p.bytes)} B, ${fmtMs(p.ms)}`)}</title>`);
    out.push(`<circle class="hit" cx="${cx}" cy="${cy}" r="12"/>${marker(p.lib, p.profile, cx, cy)}</g>`);
  }
});
out.push(`</svg>`);
writeFileSync(join(here, "frontier.svg"), out.join("\n") + "\n");
console.log(`frontier.svg: ${groups.length} apps`);
