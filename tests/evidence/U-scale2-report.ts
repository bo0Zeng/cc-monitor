#!/usr/bin/env node
/**
 * 秤 2 的**出表器 + 金标准刷新器**。
 *
 * 读 `/tmp/scale2-height-truth/result-{chromium,webkitgtk}.json`（探针的原始读数），
 * ① 打印按 card class 分桶的相对误差分布（p50/p90/max，两个口径）；
 * ② 把 Chromium 那一份**真高**固化成 `tests/evidence/U-scale2-truth-golden.json`，
 *    供 `tests/scale2-height-truth.vitest.ts` 当门禁语料。
 *
 * # 为什么金标准只存"真高"、不存"估值当判据"
 *
 * 真高只取决于 DOM + CSS + 引擎，**与 `height-estimate.ts` 无关** ⇒ 存成静态金标准是合法的。
 * 估值必须**门禁跑的时候现算**，否则改坏一个常数它照样绿（那就是「没红 ≠ 守住了」）。
 * 金标准里同时存一份探针当时算的估值（`estBrowser`），只用于两件事：
 * 出报告时的 pretext 路读数，以及常数型卡的"金标准过期"哨兵。
 *
 * 复算：`npx tsx tests/evidence/U-scale2-report.ts`（前置：`bash tests/evidence/U-scale2-run.sh` 的 ①-④）
 *
 * ⚠ 写成 `.ts` 而不是 `.mjs` 的理由与采样器同一条，原文记在那边的头注。
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// 仓根从自己的住址推，不读 `process.cwd()`（理由同采样器）
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

interface Row {
  id: string;
  cls: string;
  source: string;
  bytes: number;
  bucket: string;
  htmlLen: number;
  htmlHash: string;
  estRaw: number | null;
  estApplied: number;
  trueBorderBox: number;
  trueContentBox: number;
  padBorder: number;
}
interface Intrinsic {
  label: string;
  cls: string;
  declared: number | null;
  padBorder: number;
  measured: number;
}
interface Probe {
  error?: string;
  env: { ua: string; streamContentWidth: number; [k: string]: unknown };
  rows: Row[];
  intrinsic: Intrinsic[];
}

const WORK = "/tmp/scale2-height-truth";
const GOLDEN = resolve(REPO_ROOT, "tests/evidence/U-scale2-truth-golden.json");

function load(engine: string): Probe {
  const d = JSON.parse(
    readFileSync(`${WORK}/result-${engine}.json`, "utf8"),
  ) as Probe;
  if (d.error) throw new Error(`${engine} 探针自己报错了：${d.error}`);
  return d;
}

function pct(values: number[], p: number): number {
  const v = [...values].sort((a, b) => a - b);
  if (!v.length) return NaN;
  const k = (v.length - 1) * p;
  const f = Math.floor(k);
  const c = Math.min(f + 1, v.length - 1);
  return v[f] + (v[c] - v[f]) * (k - f);
}

const chromium = load("chromium");
let webkit: Probe | null = null;
try {
  webkit = load("webkitgtk");
} catch (e) {
  console.warn(
    "⚠ 没有 WebKitGTK 那一份（只出 Chromium 的表）：",
    String((e as Error).message),
  );
}

function table(d: Probe, label: string): void {
  const by = new Map<string, Row[]>();
  for (const r of d.rows) {
    if (!by.has(r.cls)) by.set(r.cls, []);
    by.get(r.cls)!.push(r);
  }
  console.log(
    `\n=== ${label} · rows=${d.rows.length} · 列宽=${d.env.streamContentWidth}px`,
  );
  console.log(`    UA: ${d.env.ua}`);
  console.log(
    `${"card class".padEnd(18)}${"n".padStart(4)}${"语料".padStart(6)} |` +
      ` content-box 相对误差 p50/p90/max |  border-box 相对误差 p50/p90/max | 方向`,
  );
  for (const cls of [...by.keys()].sort()) {
    const rs = by.get(cls)!;
    const src = [
      ...new Set(rs.map((r) => (r.source === "shaped" ? "真形" : "构造"))),
    ].join("+");
    // content-box 口径：估值直接对 `contain-intrinsic-size` 的语义（B 段已实测它是 content-box）
    const ec = rs.map(
      (r) => Math.abs(r.estApplied - r.trueContentBox) / r.trueContentBox,
    );
    // border-box 口径：估值实际占的位置 = estApplied + padding + border，对滚动条贡献的就是它
    const eb = rs.map(
      (r) =>
        Math.abs(r.estApplied + r.padBorder - r.trueBorderBox) /
        r.trueBorderBox,
    );
    const signed = rs.map(
      (r) => (r.estApplied - r.trueContentBox) / r.trueContentBox,
    );
    const dir = signed.every((s) => s >= 0)
      ? "全部虚高"
      : signed.every((s) => s <= 0)
        ? "全部虚低"
        : "双向";
    const f = (x: number): string => `${(x * 100).toFixed(1)}%`.padStart(7);
    console.log(
      `${cls.padEnd(18)}${String(rs.length).padStart(4)}${src.padStart(6)} |` +
        `${f(pct(ec, 0.5))}${f(pct(ec, 0.9))}${f(Math.max(...ec))}         |` +
        `${f(pct(eb, 0.5))}${f(pct(eb, 0.9))}${f(Math.max(...eb))}        | ${dir}`,
    );
  }
}

table(chromium, "Chromium 153（Blink · 生产 WebView2 同引擎家族）");
if (webkit) table(webkit, "WebKitGTK 2.52.6（Linux 侧 Tauri/wry 同一引擎）");

console.log(
  "\n=== B 段 · `contain-intrinsic-size` 的盒模型（两引擎读数相同则只打一行）",
);
for (const p of chromium.intrinsic) {
  const verdict =
    p.declared === null
      ? "（CSS 兜底）"
      : Math.abs(p.measured - p.declared) < 0.5
        ? "⇒ border-box"
        : Math.abs(p.measured - (p.declared + p.padBorder)) < 0.5
          ? "⇒ **content-box**"
          : "⇒ 都对不上";
  console.log(
    `  ${p.label.padEnd(46)} 声明=${String(p.declared).padStart(4)} padding+border=${String(p.padBorder).padStart(3)} 实测=${String(p.measured).padStart(5)} ${verdict}`,
  );
}
if (webkit) {
  const same =
    JSON.stringify(chromium.intrinsic.map((p) => p.measured)) ===
    JSON.stringify(webkit.intrinsic.map((p) => p.measured));
  console.log(
    `  两引擎 B 段读数${same ? "**完全一致**" : "🔴 不一致，见原始 JSON"}`,
  );
}

// ── 金标准 ─────────────────────────────────────────────────────────────────
const golden = {
  note:
    "秤 2 的真高金标准。真高只取决于 DOM+CSS+引擎，与 height-estimate.ts 无关 ⇒ 可以静态固化。" +
    "估值必须门禁跑的时候现算。刷新：bash tests/evidence/U-scale2-run.sh",
  generatedAt: new Date().toISOString().slice(0, 10),
  engine: {
    name: "Chromium 153.0.8010.12 (headless shell, Playwright)",
    ua: chromium.env.ua,
  },
  crossCheck: webkit
    ? {
        name: "WebKitGTK 2.52.6 (PyGObject + WebKit2-4.1, xvfb)",
        ua: webkit.env.ua,
      }
    : null,
  env: chromium.env,
  intrinsic: chromium.intrinsic,
  rows: chromium.rows.map((r) => ({
    id: r.id,
    cls: r.cls,
    source: r.source,
    bytes: r.bytes,
    bucket: r.bucket,
    htmlLen: r.htmlLen,
    htmlHash: r.htmlHash,
    estBrowser: r.estRaw === null ? null : Math.round(r.estRaw * 100) / 100,
    estAppliedBrowser: r.estApplied,
    trueBorderBox: Math.round(r.trueBorderBox * 100) / 100,
    trueContentBox: Math.round(r.trueContentBox * 100) / 100,
    padBorder: Math.round(r.padBorder * 100) / 100,
  })),
  crossRows: webkit
    ? webkit.rows.map((r) => ({
        id: r.id,
        trueContentBox: Math.round(r.trueContentBox * 100) / 100,
      }))
    : null,
};
writeFileSync(GOLDEN, JSON.stringify(golden, null, 1) + "\n", "utf8");
console.log(`\nWROTE ${GOLDEN}（${golden.rows.length} 行）`);
