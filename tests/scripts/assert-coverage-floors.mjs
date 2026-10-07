#!/usr/bin/env node
/**
 * 逐文件覆盖率地板 + 0% 文件递减棘轮。
 *
 * `vitest.config.ts` 的阈值是聚合值：单个模块掉到 0%，聚合值只动零点几个点，看不见。本脚本补两条：
 * 1. 核心模块的逐文件地板：语句数大、已有可观覆盖的那批各自不许掉；地板设在当前值下方 ~5 点（吸收 v8 版本差与用例增删的抖动）。
 * 2. 0% 文件递减棘轮：逐个登记，条数只住 `ZERO_COUNT_CEILING`；新文件掉进 0% ⇒ 红，0% 文件数只许降。
 *
 * 跑法：`npm run coverage && node tests/scripts/assert-coverage-floors.mjs`（CI 里紧跟在 `coverage floor` 那步之后，是真阻断门禁）。
 */
import { readFileSync } from "node:fs";
import { resolve, dirname, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const SUMMARY = resolve(REPO, "coverage/coverage-summary.json");

/**
 * 核心模块的逐文件覆盖地板：**语句 + 分支**。
 *
 * `[文件, 语句地板%, 写下时实测%, 分支地板%, 写下时实测%]`：实测值一起写下，下一个人才看得出地板过期没有。
 * 分支单列：语句不掉、分支掉光时语句地板没反应。分支地板同样设在当前值下方 ~5 点。
 */
const PER_FILE_FLOORS = [
  ["src/frontend/ui/tabs.ts", 64, 69.9, 54, 59.7],

  ["src/frontend/ui/tab-stream-view.ts", 85, 90.8, 78, 83.9],
  ["src/frontend/ui/tab-session-actions.ts", 62, 67.5, 59, 64.7],
  ["src/frontend/ui/tab-menu.ts", 66, 71.4, 70, 75.4],
  ["src/frontend/ui/tab-bar-view.ts", 82, 87.5, 72, 77.4],
  ["src/frontend/ui/views/history.ts", 63, 68.8, 45, 50.0],
  ["src/frontend/ui/settings/panel.ts", 75, 80.6, 50, 55.6],
  ["src/frontend/ui/settings/accounts-section.ts", 75, 80.7, 49, 54.2],
  ["src/frontend/ui/settings/remote-section.ts", 60, 65.6, 40, 45.1],
  ["src/frontend/ui/settings/ext-section.ts", 69, 74.6, 61, 66.4],
  ["src/frontend/ui/settings/cc-bus-section.ts", 90, 95.7, 70, 75.0],
  ["src/frontend/ui/views/grid-monitor.ts", 89, 94.7, 84, 89.3],
  ["src/frontend/ui/accounts.ts", 83, 88.9, 83, 88.1],
  ["src/frontend/ui/account-reads.ts", 92, 97.3, 89, 94.3],
  ["src/frontend/ui/launch-account.ts", 91, 96.5, 81, 86.0],
  ["src/frontend/ui/events.ts", 81, 86.7, 62, 67.0],
];

/**
 * 今天仍是 0% 的文件。这是欠账清单，不是豁免清单；条数只住 `ZERO_COUNT_CEILING`，清单跟着重测走。
 */
const ZERO_TODAY = [
  "src/frontend/ui/main.ts",
  "src/frontend/ui/settings/cc_integration.ts",
  "src/frontend/ui/views/session-viewer.ts",
  "src/frontend/ui/keybindings/editor.ts",
  "src/frontend/ui/settings/data-section.ts",
  "src/frontend/ui/tasks-panel.ts",
];

/** 0% 文件总数的棘轮地板（含上面没逐个列出的小文件）。**只许降。** */
// 棘轮记录 17→16→…→14→13：余量一出现就收掉，否则下一个掉进 0% 的文件会被余量悄悄吃掉。
// 改这个数要同时在这条记录末尾续一格（判据读它）。
const ZERO_COUNT_CEILING = 13;

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

/**
 * 把 summary 的键归一成仓根相对 + 正斜杠（`src/frontend/ui/tabs.ts`），下面每一处比对都吃归一化后的键。
 * 键是该平台的原生绝对路径（Windows 上是 `D:\a\cc-monitor\cc-monitor\src\…`，同名目录套两层），
 * 所以用 `relative(REPO, k)`：`REPO` 从本文件位置算出，与目录叫什么、嵌几层无关；`sep` 不赌平台。
 * `json-summary` 只有根节点 `total` 与逐文件两种键 ⇒ 滤掉 `total` 就够，且要滤在归一化之前。
 */
const normKey = (k) => relative(REPO, k).split(sep).join("/");

const files = Object.entries(summary)
  .filter(([k]) => k !== "total")
  .map(([k, v]) => [normKey(k), v]);
// 抽取器自检①：解析不出文件时下面每条都会零命中地绿。
if (files.length < 150) {
  console.error(`只解析出 ${files.length} 个文件 —— 抽取器坏了`);
  process.exit(2);
}
// 抽取器自检②：归一化之后必须真的落回 `src/…` 这个形状。
// `vitest.config.ts` 的 `coverage.include` 逐字是 `src/**/*.ts` ⇒ 一条都不落在 `src/` 下时，
// 唯一的解释是归一化的基准错了（键不在 `REPO` 之下）。**这时候要说「量具坏了」，不许往下判。**
if (!files.some(([k]) => k.startsWith("src/"))) {
  console.error(
    `归一化之后没有一个键落在 src/ 下 —— 抽取器坏了（不是那些文件没了）。\n` +
      `  仓根 REPO = ${REPO}\n` +
      `  头 3 个原始键 = ${Object.keys(summary)
        .filter((k) => k !== "total")
        .slice(0, 3)
        .join("、")}\n` +
      "⇒ summary 的键不在仓根之下（换了 CI 布局？跑在别的目录里？）。往下判会得到一串假红。",
  );
  process.exit(2);
}

const entryOf = (rel) => files.find(([k]) => k === rel)?.[1] ?? null;

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
  // 分支单独判：语句不掉、分支掉光时，上面那条没反应。
  if (v.branches.pct < bFloor) {
    problems.push(
      `  ${rel}：分支 ${v.branches.pct}% < 地板 ${bFloor}%（写下这条时实测 ${bMeasured}%）\n` +
        "    ★ **语句地板看不见这种回归**：删掉一条 `if` 的一侧、或让某个错误分支再没人走到，\n" +
        "      语句覆盖几乎不动，而那条分支从此无人验证。",
    );
  }
}

// 键已经是仓根相对正斜杠（见上面 `normKey`）⇒ 这里不再切、不再猜。
const zeroNow = files.filter(([, v]) => v.statements.pct === 0).map(([k]) => k);

// 用等号不用 `endsWith`：两边同一种形状，`endsWith` 会把 `src/a/src/tabs.ts` 也认成 `src/frontend/ui/tabs.ts`。
const newZero = zeroNow.filter((f) => !ZERO_TODAY.includes(f));
// 只有语句数够大的新 0% 才算回归；小工具文件天然可能没测。
const newZeroBig = newZero.filter((f) => {
  const hit = files.find(([k]) => k === f);
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
