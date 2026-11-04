// Plots build time against size for the synthetic app: buildtime.svg.
//
// Usage: node buildtime.mjs (after buildtime.sh)
//
// A line per library and profile through its builds with 1, 10, 20, ...,
// 100 components (left to right): compressed wasm size (x) against the
// median time to build it (y), on log scales, so bottom-left is best. Color is the library and shape the profile.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const here = new URL(".", import.meta.url).pathname;
const LIBS = ["leafy", "yew"];
const PROFILES = [
  ["speed", "optimized for speed (opt-level 3, wasm-opt -O4, brotli -q 9)"],
  ["size", "optimized for size (opt-level \"z\", LTO, wasm-opt -Oz, brotli -q 11)"],
];
const COUNTS = [1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

const times = JSON.parse(readFileSync(join(here, "out", "buildtime-synthetic.json"), "utf8"));
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const bytesOf = (dir) => Number(/wasm\.br=(\d+)/.exec(readFileSync(join(here, "out", dir, "sizes.txt"), "utf8"))[1]);
const series = LIBS.flatMap((lib) =>
  PROFILES.map(([profile]) => ({
    lib,
    profile,
    points: COUNTS.map((n) => ({ n, bytes: bytesOf(`synthetic-${n}-${lib}-${profile}`), s: median(times[lib][profile][n]), all: times[lib][profile][n] })),
  })),
);

const fmtFull = (n) => Math.round(n).toLocaleString("en-US");
const fmtS = (s) => `${+s.toFixed(2)} s`;
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;");
const niceMax = (v) => {
  const p = 10 ** Math.floor(Math.log10(v));
  return [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10, 15, 20].map((m) => m * p).find((m) => m >= v * 1.06);
};

const W = 900, H = 560;
const M = { left: 76, right: 40, top: 140, bottom: 64 };
const pw = W - M.left - M.right, ph = H - M.top - M.bottom;
// Log scales: sizes and times each span about a decade.
const xDomain = [15_000, 200_000], yDomain = [0.45, 36];
const log = Math.log10;
const x = (b) => M.left + ((log(b) - log(xDomain[0])) / (log(xDomain[1]) - log(xDomain[0]))) * pw;
const y = (s) => M.top + ph - ((log(s) - log(yDomain[0])) / (log(yDomain[1]) - log(yDomain[0]))) * ph;
const xTicks = [20_000, 30_000, 50_000, 100_000, 150_000];
const yTicks = [0.5, 1, 2, 5, 10, 20];

const out = [];
out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d" font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">`);
out.push(`<title id="t">Build time and size with 1 to 100 components: leafy and yew</title>`);
out.push(`<desc id="d">${esc(series.flatMap((s) => s.points.map((p) => `${s.lib}, optimized for ${s.profile}, ${p.n} components: ${fmtFull(p.bytes)} bytes, built in ${fmtS(p.s)}`)).join("; "))}</desc>`);
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
  .line { stroke-width: 2; fill: none; stroke-linecap: round; stroke-linejoin: round; }
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
out.push(`<text class="ink" x="${M.left - 50}" y="32" font-size="17" font-weight="600">Build time and size, with 1 to 100 components</text>`);
out.push(`<text class="ink2" x="${M.left - 50}" y="54" font-size="13">The synthetic app built with 1, 10, 20, …, 100 components (each line, left to right): its compressed wasm size</text>`);
out.push(`<text class="ink2" x="${M.left - 50}" y="72" font-size="13">against the time to build it (compile, optimize, compress; 4 cores; median of 3). Bottom-left is best.</text>`);

const marker = (cls, profile, cx, cy, r = 5) =>
  profile === "speed"
    ? `<circle class="${cls} ring mark" cx="${cx}" cy="${cy}" r="${r}"/>`
    : `<rect class="${cls} ring mark" x="${cx - r + 1}" y="${cy - r + 1}" width="${2 * r - 2}" height="${2 * r - 2}" transform="rotate(45 ${cx} ${cy})"/>`;

// Legend: libraries, then profiles on a second row.
let lx = M.left - 50;
const ly = 96;
for (const lib of LIBS) {
  out.push(`<line class="line line-${lib}" x1="${lx}" y1="${ly}" x2="${lx + 22}" y2="${ly}"/>`);
  out.push(`<text class="ink2" x="${lx + 30}" y="${ly + 4}" font-size="12">${lib}</text>`);
  lx += 30 + lib.length * 7 + 22;
}
lx = M.left - 50;
for (const [profile, label] of PROFILES) {
  out.push(marker("muted", profile, lx + 6, ly + 20, 5));
  out.push(`<text class="ink2" x="${lx + 18}" y="${ly + 24}" font-size="12">${esc(label)}</text>`);
  lx += 18 + label.length * 6.3 + 22;
}

// Grid and axes.
for (const t of xTicks) {
  out.push(`<line class="grid" x1="${x(t)}" y1="${M.top}" x2="${x(t)}" y2="${M.top + ph}"/>`);
  out.push(`<text class="muted" x="${x(t)}" y="${M.top + ph + 18}" font-size="12" text-anchor="middle">${Math.round(t / 1000)} KB</text>`);
}
for (const t of yTicks) {
  out.push(`<line class="grid" x1="${M.left}" y1="${y(t)}" x2="${M.left + pw}" y2="${y(t)}"/>`);
  out.push(`<text class="muted" x="${M.left - 8}" y="${y(t) + 4}" font-size="12" text-anchor="end">${fmtS(t)}</text>`);
}
out.push(`<line class="axis" x1="${M.left}" y1="${M.top + ph}" x2="${M.left + pw}" y2="${M.top + ph}"/>`);
out.push(`<line class="axis" x1="${M.left}" y1="${M.top}" x2="${M.left}" y2="${M.top + ph}"/>`);
out.push(`<text class="ink2" x="${M.left + pw / 2}" y="${H - 20}" font-size="12" text-anchor="middle">Compressed wasm size (brotli; log scale)</text>`);
out.push(`<text class="ink2" transform="translate(20 ${M.top + ph / 2}) rotate(-90)" font-size="12" text-anchor="middle">Build time (log scale)</text>`);

for (const s of series) {
  out.push(`<polyline class="line line-${s.lib}" points="${s.points.map((p) => `${x(p.bytes).toFixed(1)},${y(p.s).toFixed(1)}`).join(" ")}"/>`);
}
for (const s of series) {
  for (const p of s.points) {
    const cx = +x(p.bytes).toFixed(1), cy = +y(p.s).toFixed(1);
    out.push(`<g class="dot"><title>${esc(`${s.lib}, optimized for ${s.profile}, ${p.n} component${p.n === 1 ? "" : "s"}: ${fmtFull(p.bytes)} B, built in ${fmtS(p.s)} (runs: ${p.all.map((t) => fmtS(t)).join(", ")})`)}</title>`);
    out.push(`<circle class="hit" cx="${cx}" cy="${cy}" r="10"/>${marker(s.lib, s.profile, cx, cy)}</g>`);
  }
  // The counts at the line's ends: 1 below-left, 100 above-right.
  const [first, last] = [s.points[0], s.points[s.points.length - 1]];
  out.push(`<text class="muted" x="${(x(first.bytes) - 9).toFixed(1)}" y="${(y(first.s) + 16).toFixed(1)}" font-size="11" text-anchor="end">${first.n}</text>`);
  out.push(`<text class="muted" x="${(x(last.bytes) + 9).toFixed(1)}" y="${(y(last.s) - 7).toFixed(1)}" font-size="11">${last.n}</text>`);
}
// The takeaways, with 1 and with 100 components: leafy against yew with the
// same profile (the geometric mean over the profiles).
{
  const ratio = (i, key) => {
    const rs = PROFILES.map(([profile]) => {
      const [l, w] = LIBS.map((lib) => series.find((s) => s.lib === lib && s.profile === profile).points.at(i));
      return w[key] / l[key];
    });
    return rs.reduce((a, b) => a * b, 1) ** (1 / rs.length);
  };
  const line = (i) => {
    const n = series[0].points.at(i).n;
    return `With ${n} component${n === 1 ? "" : "s"}: ${ratio(i, "bytes").toFixed(1)}× smaller, ${ratio(i, "s").toFixed(1)}× faster to build`;
  };
  // 1 component in the bottom-left, 100 in the top-right: by those builds.
  out.push(`<text class="ink" x="${M.left + 12}" y="${M.top + ph - 12}" font-size="15" font-weight="600">${line(0)}</text>`);
  out.push(`<text class="ink" x="${M.left + pw - 12}" y="${M.top + 24}" font-size="15" font-weight="600" text-anchor="end">${line(-1)}</text>`);
}
out.push(`</svg>`);
writeFileSync(join(here, "buildtime.svg"), out.join("\n") + "\n");
console.log("buildtime.svg:");
for (const s of series) console.log(`  ${s.lib.padEnd(6)} ${s.profile.padEnd(6)} ${s.points.map((p) => `${p.n}: ${fmtS(p.s)}`).join(", ")}`);
