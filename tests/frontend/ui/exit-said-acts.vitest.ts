/**
 * 回答与运行表的成品由后端写好，出口照摆、不各自判（A16-1-2）：
 * - 答题「别处已答」是 `needs-answer` 回包的一种结局（`result: "gone"` ＋ 那一句）：桌面 · 手机的产品源里不再认 `already_answered` · `closed_elsewhere` 两个码；
 * - 选项标红照语气（`tone: fail`）：出口不认 `deny-stop` 这个选项 id；
 * - 运行表的状态字 · 名字 · 记录里最近一件事由后端写进那一帧（`status_text` · `label_text` · `last_text`）：`runs.ts` 不再取 `runs.state.*` · `runs.label.*` · `runs.last.tool`；
 * - 送字的结局带下一步（`terminal-input` 回包的 `acts`：再发一次 · 重抓 · 重读名单）：出口不认拒因码。
 * 正控：同一把尺在后端那一处量得到它们（没量到 ⇒ 本条在空转）。
 * 只扫生产源：测试与生成物不算；注释照扫（「零出现」就是零出现，免得说明里还留着旧码让人照抄）。
 */
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { sourceFiles, type ProductionSource } from "../../test-support/production-sources";

const ROOT = resolve(__dirname, "../../..");

/** 手机端那几类不算生产源的目录（构建产物 · 生成的 · 各种测试源）。 */
const NOT_MOBILE_SOURCE = ["build", "gen", "test", "androidTest", "testFixtures"];
/** 出口：桌面界面 · 手机端（生产源）。 */
const EXITS = [...sourceFiles("src/frontend/ui", { ext: ".ts" }), ...sourceFiles("src/mobile", { ext: ".kt", skipDirs: NOT_MOBILE_SOURCE })];

function hits(files: readonly ProductionSource[], needles: readonly string[]): string[] {
  return files.flatMap((f) => needles.filter((n) => f.text.includes(n)).map((n) => `${f.file} · ${n}`));
}

/**
 * 还在照拒因码自己判、改动已另交给占着它的那一路的出口：每一项都要真还命中（不命中 ⇒ 红，删掉那一项），不是豁免名单。
 * 手机聊天屏那一份照 `acts` 判的改动随它那一路落。
 */
const PENDING: readonly string[] = [];

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

  it("★ 送字的结局：出口不认拒因码，给不给［再发一次］· 重抓 · 重读照后端的 `acts`", () => {
    // 拒因码里不与别的词撞的那几个（`ambiguous` · `ended` · `waiting` · `elsewhere` 别处另有义项，不当针），连手机那张「能重发」表的名字。
    const needles = ["screen_changed", "not_known", "not_yours", "not_managed", "other_windows", "not_resumed", "RETRYABLE"] as const;
    const got = hits(EXITS, needles);
    expect(got.filter((h) => !PENDING.some((p) => h.startsWith(`${p} · `)))).toEqual([]);
    for (const p of PENDING) expect(got.some((h) => h.startsWith(`${p} · `)), `${p} 已经不认拒因码了 —— 把它从 PENDING 里删掉`).toBe(true);
    const be = backend("src/backend/control/terminals.rs");
    expect(be, "正控：下一步在后端那一处判").toContain("pub(crate) fn acts_of(");
    expect(be, "正控：拒因码住后端").toContain("Refused::ScreenChanged");
  });

  it("★ 运行表的出口不取状态字 · 通称 · 「调用 X」（后端写进那一帧）", () => {
    const needles = ['copyText("runs.state.', 'copyText("runs.label.', 'copyText("runs.last.tool"', "copyText(`runs.state."];
    const runs = "src/frontend/ui/runs.ts";
    expect(hits([{ file: runs, text: backend(runs) }], needles)).toEqual([]);
    const be = backend("src/backend/observe/runs.rs");
    for (const k of ["history.row.live", "runs.state.done", "runs.state.failed", "runs.state.stopped", "sessionState.unseen.name", "newSession.label.agent", "runs.last.tool"]) {
      expect(be, `正控：${k} 由后端写`).toContain(`"${k}"`);
    }
  });
});
