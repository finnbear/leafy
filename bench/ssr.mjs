// Plots the server-rendering benchmark: ssr.svg.
//
// Usage: node ssr.mjs (after ssr.sh)
//
// A pair of bars (leafy, yew) per scene: the median time to render it to a
// string, natively; shorter is better.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const here = new URL(".", import.meta.url).pathname;
const SCENES = [
  ["table", "1,000-row table", "js-framework-benchmark's rows"],
  ["components", "100 components", "the synthetic app's, with a context"],
  ["article", "An article", "50 sections of text, links and lists"],
];
const LIBS = ["leafy", "yew"];
const data = JSON.parse(readFileSync(join(here, "out", "ssr.json"), "utf8"));

const fmtUs = (us) => (us >= 1000 ? `${+(us / 1000).toFixed(2)} ms` : `${Math.round(us)} µs`);
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;");
const niceMax = (v) => {
  const p = 10 ** Math.floor(Math.log10(v));
  return [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10].map((m) => m * p).find((m) => m >= v * 1.12);
};

const W = 760, H = 460;
const M = { left: 76, right: 24, top: 104, bottom: 70 };
const pw = W - M.left - M.right, ph = H - M.top - M.bottom;
const yMax = niceMax(Math.max(...SCENES.flatMap(([k]) => LIBS.map((l) => data[l][k].us))));
const y = (us) => M.top + ph - (us / yMax) * ph;

const out = [];
out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d" font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">`);
out.push(`<title id="t">Server-rendering time: leafy and yew</title>`);
out.push(`<desc id="d">${esc(SCENES.map(([k, name]) => `${name}: ${LIBS.map((l) => `${l} ${fmtUs(data[l][k].us)}`).join(", ")}`).join("; "))}</desc>`);
out.push(`<style>
  .surface { fill: #fcfcfb; }
  .ink { fill: #0b0b0b; }
  .ink2 { fill: #52514e; }
  .muted { fill: #898781; }
  .grid { stroke: #e1e0d9; stroke-width: 1; }
  .axis { stroke: #c3c2b7; stroke-width: 1; }
  .leafy { fill: #2a78d6; }
  .yew { fill: #eb6834; }
  .bar:hover path { opacity: 0.85; }
  @media (prefers-color-scheme: dark) {
    .surface { fill: #1a1a19; }
    .ink { fill: #f0efec; }
    .ink2 { fill: #c3c2b7; }
    .grid { stroke: #2c2c2a; }
    .axis { stroke: #383835; }
    .leafy { fill: #3987e5; }
    .yew { fill: #d95926; }
  }
</style>`);
out.push(`<rect class="surface" width="${W}" height="${H}" rx="8"/>`);
out.push(`<text class="ink" x="${M.left - 52}" y="32" font-size="17" font-weight="600">Server-rendering time: leafy and yew</text>`);
out.push(`<text class="ink2" x="${M.left - 52}" y="54" font-size="13">Median time to render each scene to a string (natively, one core, release build). Shorter is better.</text>`);
let lx = M.left - 52;
for (const lib of LIBS) {
  out.push(`<rect class="${lib}" x="${lx}" y="72" width="12" height="12" rx="2"/>`);
  out.push(`<text class="ink2" x="${lx + 18}" y="82" font-size="12">${lib}</text>`);
  lx += 18 + lib.length * 7 + 22;
}

// Round ticks: a step of 1, 2, 2.5 or 5 times a power of ten, 3 to 6 of
// them.
const step = (() => {
  const p = 10 ** Math.floor(Math.log10(yMax / 3));
  return [1, 2, 2.5, 5, 10].map((m) => m * p).find((s) => yMax / s <= 5);
})();
for (let t = 0; t <= yMax + 1e-9; t += step) {
  out.push(`<line class="grid" x1="${M.left}" y1="${y(t)}" x2="${M.left + pw}" y2="${y(t)}"/>`);
  out.push(`<text class="muted" x="${M.left - 8}" y="${y(t) + 4}" font-size="12" text-anchor="end">${t === 0 ? "0" : fmtUs(t)}</text>`);
}
out.push(`<line class="axis" x1="${M.left}" y1="${M.top + ph}" x2="${M.left + pw}" y2="${M.top + ph}"/>`);

// A pair of bars (2px apart) per scene, rounded (4px) at the top.
const groupW = pw / SCENES.length, barW = 56, gap = 2;
const bar = (x, top, w, bottom) => {
  const r = Math.min(4, bottom - top);
  return `M${x},${bottom} V${top + r} Q${x},${top} ${x + r},${top} H${x + w - r} Q${x + w},${top} ${x + w},${top + r} V${bottom} Z`;
};
SCENES.forEach(([key, name, note], i) => {
  const cx = M.left + groupW * (i + 0.5);
  let bx = cx - barW - gap / 2;
  for (const lib of LIBS) {
    const { us, bytes } = data[lib][key];
    const top = y(us);
    out.push(`<g class="bar"><title>${esc(`${name}, ${lib}: ${fmtUs(us)} per render (${bytes.toLocaleString("en-US")} bytes of HTML)`)}</title>`);
    out.push(`<path class="${lib}" d="${bar(bx, top, barW, M.top + ph)}"/></g>`);
    out.push(`<text class="ink2" x="${bx + barW / 2}" y="${(top - 6).toFixed(1)}" font-size="12" text-anchor="middle">${fmtUs(us)}</text>`);
    bx += barW + gap;
  }
  out.push(`<text class="ink" x="${cx}" y="${M.top + ph + 20}" font-size="12.5" font-weight="600" text-anchor="middle">${esc(name)}</text>`);
  out.push(`<text class="muted" x="${cx}" y="${M.top + ph + 37}" font-size="11" text-anchor="middle">${esc(note)}</text>`);
});
out.push(`</svg>`);
writeFileSync(join(here, "ssr.svg"), out.join("\n") + "\n");
console.log("ssr.svg:");
for (const [k, name] of SCENES) console.log(`  ${name.padEnd(16)} ${LIBS.map((l) => `${l} ${fmtUs(data[l][k].us)}`).join(", ")}  (${(data.yew[k].us / data.leafy[k].us).toFixed(1)}× faster)`);
