/**
 * 截图台架的假适配层（`tests/shots/fake/`）不写核心的成品：核心写的字（额度行 · 距今 · 需手动 · 状态字 …）与时刻字一律由真后端算，
 * 台架只造盘上原始格式的输入（`tests/shots/disk/`）交给真后端（`tests/shots/real/pool.mjs`）。
 *
 * 为什么有这一格：mg39 把额度行收进核心（`faces/quota_rows.rs`）的同一批，截图假后端又抄了一份行模型与距今（`quota-face.ts` ·
 * `backend.ts::withTexts`）——核心每加一格，台架就得手补一次，补错了截图照样「长得像」，谁也看不出（架构复审四 R6 / R7）。
 *
 * 认什么（每一种都是「替核心写成品」的写法）：
 * - `copyText("<键>")` 而这个键核心自己也在写（`src/backend` · `src/common` · `src/comms` 的 Rust 里 `copy_text("<键>"` 出现过）；
 * - 自己拼钟面：`getHours(` · `getMinutes(` · `toDateString(`；
 * - 自己拼时长：`fmtDur(`（界面那一侧的读口，替核心写 `…Text`）；
 * - 自己拼距今：模板串里 `+${`（`+1h50m` 那一形）。
 *
 * 人群：`tests/shots/fake/` 下全部 `.ts`（新加一份也自动进来）。
 * 允许名单：还没换成真后端的那几份，**按文件 × 写法记确切的处数**，每条带理由（它在替核心的哪一格、为什么还没换）；
 * 处数只许降 —— 换掉一处就得同拍改小（多了红、少了也红，死条目红），不许空放。
 * 买不到：读的是源码文本；键经变量传进 `copyText`、核心那一侧不经 `copy_text` 字面量取文、把钟面拆进别的帮手里，都认不出。
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";

const repo = path.resolve(__dirname, "../..");
const fakeDir = path.join(__dirname, "fake");

function walk(dir: string, ext: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = path.join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "target" || name === "node_modules" || name === "generated") continue;
      out.push(...walk(p, ext));
    } else if (name.endsWith(ext)) out.push(p);
  }
  return out;
}

/** 核心自己写的那些键。 */
function coreKeys(): Set<string> {
  const keys = new Set<string>();
  for (const root of ["src/backend", "src/common", "src/comms"]) {
    for (const f of walk(path.join(repo, root), ".rs")) {
      for (const m of readFileSync(f, "utf8").matchAll(/copy_text\(\s*"([A-Za-z0-9_.]+)"/g)) keys.add(m[1]);
    }
  }
  return keys;
}

type Form = "coreKey" | "clock" | "duration" | "relative";

const FORMS: Record<Exclude<Form, "coreKey">, RegExp> = {
  clock: /\b(getHours|getMinutes|toDateString)\(/g,
  duration: /\bfmtDur\(/g,
  relative: /`[^`]*\+\$\{/g,
};

function hitsIn(src: string, keys: Set<string>): Record<Form, number> {
  const n: Record<Form, number> = { coreKey: 0, clock: 0, duration: 0, relative: 0 };
  for (const m of src.matchAll(/copyText\(\s*"([A-Za-z0-9_.]+)"/g)) if (keys.has(m[1])) n.coreKey++;
  for (const [form, re] of Object.entries(FORMS) as [Exclude<Form, "coreKey">, RegExp][]) n[form] = [...src.matchAll(re)].length;
  return n;
}

/**
 * 还没换成真后端的那几份：文件 → 写法 → [处数, 理由]。
 * 换掉了就删条目或改小处数；新写的成品不许往这里加 —— 改成造盘上的原始文件、把那条帧命令放进 `backend.ts::REAL_OPS`。
 */
const ALLOW: Record<string, Partial<Record<Form, [number, string]>>> = {
  "backend.ts": {
    coreKey: [
      5,
      "会话流（`session-lines`）还是合成的：活动 · 已结束 · 可重连那几格的字（`wire::activity_cells` 的 beSession.* · sessionState.*）与合成拒答的复制详情（detail.*）要等会话流接真后端那一刀（会话记录落盘 ＋ 判活 ＋ 壳的会话账本）",
    ],
  },
  "clock.ts": {
    coreKey: [5, "历史清单 · 内容头 · 查找那几格的时刻字（`common::time::{row_time, section_text, span_text, hit_time}`）：history-* 那一族还是合成的，等会话记录落盘那一刀"],
    clock: [2, "同上：历史那几格的钟面（当天 HH:MM · 别的天 MM-DD …）"],
  },
  "machine.ts": {
    coreKey: [39, "机器页那一族帧命令（扩展 · MCP · 别名 · 足迹 · 数据报告 · 终端 · ssh 导入 …）还是合成的，回包里的后端那几句是照抄的；要先给每条造好盘上的输入（多数还要假的 ssh / 终端在边界上替身）"],
  },
  "ops.ts": {
    coreKey: [14, "记录那一族（history-turns · history-facts 的上下文 / 后台命令那几格）还是合成的：合成的是解析后的通用记录，不是盘上的原始 jsonl；等会话记录落盘那一刀"],
    duration: [2, "同上：history-facts 的需手动等了多久 · 后台命令跑了多久（`waitedText` · 后台那一格）"],
  },
  "plan.ts": {
    clock: [4, "计划那一族（plan-*）还是合成的：计划 dump 的读法与成品没接真后端（要造计划工作区的原始 dump）"],
  },
  "profiles.ts": {
    coreKey: [26, "配置文件那一族（profiles-*）还是合成的：几种坏样子（语法错 · 迁移过 · 改过）的回包句子照抄后端；要造对应的 profiles.toml 原文"],
  },
};

const keys = coreKeys();
const files = walk(fakeDir, ".ts").filter((p) => !p.endsWith(".vitest.ts"));

describe("截图台架的假适配层不写核心成品", () => {
  it("人群不空：fake/ 下有那几份，核心的键也读到了", () => {
    expect(files.map((p) => path.basename(p))).toContain("backend.ts");
    expect(keys.size).toBeGreaterThan(300);
    // 判据认得出这几种写法（自检：拿一段写着成品的样本过一遍）。
    const sample = 'copyText("history.section.today"); d.getHours(); fmtDur(3); `+${m}m`';
    expect(hitsIn(sample, keys)).toEqual({ coreKey: 1, clock: 1, duration: 1, relative: 1 });
  });

  it("每份只许有允许名单里记着的那几处（多了不许、少了要同拍改小）", () => {
    const got: Record<string, Partial<Record<Form, number>>> = {};
    for (const f of files) {
      const n = hitsIn(readFileSync(f, "utf8"), keys);
      const nz = Object.fromEntries(Object.entries(n).filter(([, v]) => v > 0)) as Partial<Record<Form, number>>;
      if (Object.keys(nz).length > 0) got[path.relative(fakeDir, f)] = nz;
    }
    const want = Object.fromEntries(Object.entries(ALLOW).map(([f, forms]) => [f, Object.fromEntries(Object.entries(forms).map(([k, v]) => [k, v![0]]))]));
    expect(got, "核心的成品由真后端算：改成造 disk/ 里的原始文件、把那条帧命令放进 backend.ts::REAL_OPS").toEqual(want);
  });

  it("允许名单每条都带理由", () => {
    for (const [f, forms] of Object.entries(ALLOW)) for (const [k, v] of Object.entries(forms)) expect(v![1].length, `${f} · ${k}`).toBeGreaterThan(20);
  });

  it("交给真后端答的帧命令，fake/ 里没有第二份答法", () => {
    const backend = readFileSync(path.join(fakeDir, "backend.ts"), "utf8");
    const real = [...backend.slice(backend.indexOf("export const REAL_OPS"), backend.indexOf("]);", backend.indexOf("export const REAL_OPS"))).matchAll(/"([a-z][a-z0-9-]+)"/g)].map((m) => m[1]);
    expect(real).toContain("quota-read");
    const windowsOnly = [...backend.slice(backend.indexOf("const WINDOWS_ONLY"), backend.indexOf("\n", backend.indexOf("const WINDOWS_ONLY"))).matchAll(/"([a-z][a-z0-9-]+)"/g)].map((m) => m[1]);
    const dup: string[] = [];
    for (const f of files) {
      const src = readFileSync(f, "utf8");
      for (const op of real) if (!windowsOnly.includes(op) && new RegExp(`^\\s+"${op}":\\s*\\(`, "m").test(src)) dup.push(`${path.basename(f)}: ${op}`);
    }
    expect(dup).toEqual([]);
  });
});
