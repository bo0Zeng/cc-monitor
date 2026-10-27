/**
 * 回答与运行表的成品由后端写好，出口照摆、不各自判（A16-1-2）：
 * - 答题「别处已答」是 `needs-answer` 回包的一种结局（`result: "gone"` ＋ 那一句）：桌面 · 手机的产品源里不再认 `already_answered` · `closed_elsewhere` 两个码；
 * - 选项标红照语气（`tone: fail`）：出口不认 `deny-stop` 这个选项 id；
 * - 运行表的状态字 · 名字 · 记录里最近一件事由后端写进那一帧（`status_text` · `label_text` · `last_text`）：`runs.ts` 不再取 `runs.state.*` · `runs.label.*` · `runs.last.tool`。
 * 正控：同一把尺在后端那一处量得到它们（没量到 ⇒ 本条在空转）。
 * 只扫生产源：测试与生成物不算；注释照扫（「零出现」就是零出现，免得说明里还留着旧码让人照抄）。
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = resolve(__dirname, "../../..");

function walk(dir: string, ext: string[], skip: (p: string) => boolean): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (skip(p)) continue;
    if (statSync(p).isDirectory()) out.push(...walk(p, ext, skip));
    else if (ext.some((e) => p.endsWith(e))) out.push(p);
  }
  return out;
}

const rel = (p: string) => relative(ROOT, p).replaceAll("\\", "/");
const skip = (p: string) => /[\\/](node_modules|generated|build|test|androidTest|testFixtures)([\\/]|$)/.test(p) || /[\\/]gen([\\/]|$)/.test(p);
/** 出口：桌面界面 · 手机端（生产源）。 */
const EXITS = [...walk(join(ROOT, "src/frontend/ui"), [".ts"], skip), ...walk(join(ROOT, "src/mobile"), [".kt"], skip)];

function hits(files: string[], needles: readonly string[]): string[] {
  return files.flatMap((f) => {
    const code = readFileSync(f, "utf8");
    return needles.filter((n) => code.includes(n)).map((n) => `${rel(f)} · ${n}`);
  });
}

/** 后端判这几件的那几处（正控）。 */
const backend = (p: string) => readFileSync(join(ROOT, p), "utf8");

describe("回答与运行表：出口照摆后端写好的成品", () => {
  it("★ 出口零出现答题的旧码与选项 id（别处已答照回包结局、标红照语气）", () => {
    expect(EXITS.length, "一份出口源都没找到 —— 本条在空转").toBeGreaterThan(200);
    const needles = ["already_answered", "closed_elsewhere", "deny-stop"] as const;
    expect(hits(EXITS, needles)).toEqual([]);
    expect(backend("src/backend/agents/asks.rs"), "正控：选项 id 住后端").toContain('"deny-stop"');
    expect(backend("src/backend/control/asks/mod.rs"), "正控：别处已答那一种结局在后端写").toContain("AnswerResult::Gone");
  });

  it("★ 运行表的出口不取状态字 · 通称 · 「调用 X」（后端写进那一帧）", () => {
    const needles = ['copyText("runs.state.', 'copyText("runs.label.', 'copyText("runs.last.tool"', "copyText(`runs.state."];
    expect(hits([join(ROOT, "src/frontend/ui/runs.ts")], needles)).toEqual([]);
    const be = backend("src/backend/observe/runs.rs");
    for (const k of ["runs.state.running", "runs.state.done", "runs.state.failed", "runs.state.stopped", "runs.state.unknown", "runs.label.unnamed", "runs.last.tool"]) {
      expect(be, `正控：${k} 由后端写`).toContain(`"${k}"`);
    }
  });
});
