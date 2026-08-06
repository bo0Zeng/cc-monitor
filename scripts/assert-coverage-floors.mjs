#!/usr/bin/env node
/**
 * **逐文件覆盖率地板 + 0% 文件递减棘轮**〔audit-0805 F17 下半，报告 §5.4/§5.5〕。
 *
 * # 为什么聚合阈值不够
 *
 * `vitest.config.ts` 的阈值是**聚合值**（statements/branches/functions/lines 各一个数）。
 * 后果：**单个模块掉到 0% 看不见** —— 187 个文件里少数几个归零，聚合值只动零点几个点，
 * 而地板留着 2-3 点余量，门禁一声不响。
 *
 * ⚠ 这不是推测：`vitest.config.ts` 那段注释自己就写着
 * 「收紧留后续按核心 DOM 模块 **per-file**」 —— **本脚本就是那个「后续」**。
 *
 * # 两条判据
 *
 * 1. **核心模块的逐文件地板**：语句数大、且今天已有可观覆盖的那批，各自不许掉下去。
 *    地板设在**当前值下方 ~5 点**（吸收 v8 版本差与用例增删的抖动），只挡明显回归。
 * 2. **0% 文件递减棘轮**：逐个登记（08-06 实测 16 个；数字以 `ZERO_COUNT_CEILING` 为准）。
 *    - **新文件掉进 0% ⇒ 红**（那正是聚合阈值看不见的那格）；
 *    - **0% 文件数只许降**。
 *
 * # 怎么跑
 *
 *   npm run coverage && node scripts/assert-coverage-floors.mjs
 *
 * CI 里紧跟在 `coverage floor` 那步之后（那步**无 `|| true`**，是真阻断门禁）。
 */
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SUMMARY = resolve(REPO, "coverage/coverage-summary.json");

/**
 * 核心模块的逐文件覆盖地板：**语句 + 分支**。
 *
 * `[文件, 语句地板%, 写下时实测%, 分支地板%, 写下时实测%]`（08-06）——
 * **实测值一起写下**，照 `vitest.config.ts` 那条棘紧纪律：
 * 只改数字不写实测，下一个人看不出它过期没过期。
 *
 * # 为什么加分支那一列〔audit-0805 §5 2b〕
 *
 * 2b 逐字写着「一个模块 statements 不掉、branches 掉光，本判据看不见」。
 * 量下去证实这不是假设：`views/panorama.ts` 今天 statements **56.5%** 而 branches
 * 只有 **24.8%** —— 一半以上的分支从没被走过，而语句地板一点反应都没有。
 *
 * ⚠ 分支地板同样设在**当前值下方 ~5 点**（与语句同一套纪律），只挡明显回归；
 * 分支覆盖比语句更容易被 v8 版本差与用例增删扰动，余量不能收得太紧。
 */
const PER_FILE_FLOORS = [
  ["src/tabs.ts", 64, 69.9, 54, 59.7],
  ["src/views/history.ts", 63, 68.8, 45, 50.0],
  ["src/views/panorama.ts", 51, 56.5, 19, 24.8],
  ["src/settings/panel.ts", 75, 80.6, 50, 55.6],
  ["src/settings/accounts-section.ts", 75, 80.7, 49, 54.2],
  ["src/settings/remote-section.ts", 60, 65.6, 40, 45.1],
  ["src/settings/mcp-section.ts", 58, 63.5, 49, 54.5],
  ["src/settings/cc-bus-section.ts", 90, 95.7, 70, 75.0],
  ["src/views/grid-monitor.ts", 89, 94.7, 84, 89.3],
  ["src/views/usage-view.ts", 81, 86.2, 63, 68.4],
  ["src/accounts.ts", 83, 88.9, 83, 88.1],
  // F17 下半：批量调度状态机三条分支落地（53.33 → 84.44）。它决定整个重放期是 batch 还是 live。
  ["src/events.ts", 79, 84.4, 62, 67.0],
];

/**
 * 今天仍是 0% 的文件。**这是欠账清单，不是豁免清单。**
 * ⚠ 条数**刻意不抄在这里** —— 它的家是 `ZERO_COUNT_CEILING`；
 *   同一个数在两处各存一份，改一处就是下一个假陈述（定框 E12，本仓已实测多次）。
 *
 * ⚠ 台账 §5.4 那张表已被 V4 订正过一次（`cards/index.ts` 今天是 5.8% 不是 0%），
 * 本轮重测又对上两处：`main.ts` 是 **594** 语句不是 611（F14 第三刀搬走了一圈轮询）。
 * ⇒ **这类清单必须跟着重测走**，抄一次就腐一次。
 */
const ZERO_TODAY = [
  "src/main.ts",
  "src/settings/cc_integration.ts",
  "src/views/session-viewer.ts",
  "src/keybindings/editor.ts",
  "src/settings/data-section.ts",
  "src/tasks-panel.ts",
];

/** 0% 文件总数的棘轮地板（含上面没逐个列出的小文件）。**只许降。** */
// ⚠ 08-06 F15 把 `src/branch-fold.ts` 从 0% 里领走了（它此前**一条专属单测都没有**，
// 而那条 O(N²) 主线重算就住在里面）⇒ 上限 17→16。**只许降**。
const ZERO_COUNT_CEILING = 16;

let summary;
try {
  summary = JSON.parse(readFileSync(SUMMARY, "utf8"));
} catch (e) {
  console.error(
    `读不到 ${SUMMARY}：${e.message}\n` +
      "⇒ 先跑 `npm run coverage`（它带 json-summary reporter）。\n" +
      "⚠ 本脚本**不许**在读不到时静默通过 —— 那就成了一条恒绿判据。",
  );
  process.exit(2);
}

const files = Object.entries(summary).filter(([k]) => k !== "total");
// 抽取器自检：解析不出文件时下面每条都会零命中地绿。
if (files.length < 150) {
  console.error(`只解析出 ${files.length} 个文件（08-06 实测 187）—— 抽取器坏了`);
  process.exit(2);
}

const entryOf = (rel) => files.find(([k]) => k.endsWith(`/${rel}`) || k === rel)?.[1] ?? null;

const problems = [];

for (const [rel, floor, measured, bFloor, bMeasured] of PER_FILE_FLOORS) {
  const v = entryOf(rel);
  if (v === null) {
    problems.push(
      `  ${rel}：在覆盖率报告里找不到 —— 文件搬了/改名了就把这条一起改（别让它悄悄消失）`,
    );
    continue;
  }
  if (v.statements.pct < floor) {
    problems.push(
      `  ${rel}：语句 ${v.statements.pct}% < 地板 ${floor}%（写下这条时实测 ${measured}%）\n` +
        "    ★ 聚合阈值看不见这种单模块回归 —— 187 个文件里掉一个，总数只动零点几个点。",
    );
  }
  // ★〔audit-0805 §5 2b〕**分支单独判**：语句不掉、分支掉光时，上面那条一点反应都没有。
  // 实测样本：`views/panorama.ts` 语句 56.5% 而分支只有 24.8%。
  if (v.branches.pct < bFloor) {
    problems.push(
      `  ${rel}：分支 ${v.branches.pct}% < 地板 ${bFloor}%（写下这条时实测 ${bMeasured}%）\n` +
        "    ★ **语句地板看不见这种回归**：删掉一条 `if` 的一侧、或让某个错误分支再没人走到，\n" +
        "      语句覆盖几乎不动，而那条分支从此无人验证。2b 说的正是这个。",
    );
  }
}

const zeroNow = files
  .filter(([, v]) => v.statements.pct === 0)
  .map(([k]) => k.split("/cc-monitor/").pop());

const newZero = zeroNow.filter(
  (f) => !ZERO_TODAY.includes(f) && !ZERO_TODAY.some((z) => f.endsWith(z)),
);
// 只有语句数够大的新 0% 才算回归；小工具文件天然可能没测。
const newZeroBig = newZero.filter((f) => {
  const hit = files.find(([k]) => k.endsWith(f));
  return hit && hit[1].statements.total >= 100;
});
if (newZeroBig.length > 0) {
  problems.push(
    `  新掉进 0% 的大文件（≥100 语句）：${newZeroBig.join("、")}\n` +
      "    ★ 这正是聚合阈值的盲区：新增一大块无测代码，总覆盖率只掉一两个点，\n" +
      "      而地板留着 2-3 点余量 ⇒ **门禁一声不响**。",
  );
}

if (zeroNow.length > ZERO_COUNT_CEILING) {
  problems.push(
    `  0% 文件数 ${zeroNow.length} > 棘轮上限 ${ZERO_COUNT_CEILING}\n` +
      "    ★ 这是**递减棘轮**：只许降。补了测试就把上限一起调下来，\n" +
      "      **不许把上限调上去让今天好过**。",
  );
}

if (problems.length > 0) {
  console.error("覆盖率逐文件地板未通过：\n" + problems.join("\n"));
  process.exit(1);
}

console.log(
  `[coverage-floors] ${PER_FILE_FLOORS.length} 个核心模块的**语句与分支**都在地板之上；` +
    `0% 文件 ${zeroNow.length}/${ZERO_COUNT_CEILING}（棘轮，只许降）`,
);
