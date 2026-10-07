/**
 * 设置里填的 Claude 目录：存之前问本机后端一次（`agent-home-check`），后端判在不在 · 是不是目录 · 有没有记录树、回码；界面按码取那一句。
 * 不在的照收的话，重启后会被悄悄忽略、退回默认目录，输入框却还显示着它；没有 `projects/` 的读不出一条会话。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { claudeDirProblem } from "../../../../src/frontend/ui/settings/claude-dir-check";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../../test-support/copy-pattern";
import { chanReply, type ChanCallArgs } from "../../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
let asked: string[] = [];

function backend(answer: (path: string) => unknown): void {
  invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
    if (cmd !== "chan_call") return undefined;
    asked.push(a.op);
    if (a.op !== "agent-home-check" || a.origin !== "<local>") throw new Error(`没料到：${a.op} ${a.origin}`);
    const { path } = JSON.parse(new TextDecoder().decode(new Uint8Array(a.payload))) as { path: string };
    const r = answer(path);
    if (r !== null && typeof r === "object" && "err" in r) throw r;
    return r;
  });
}

describe("Claude 目录：存之前问后端一次，按码说", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    asked = [];
  });

  it("★ 四个码各一句；只问一次 agent-home-check（界面不自己问 files-stat）", async () => {
    const cases: [string, string | null][] = [
      ["ok", null],
      ["missing", copyText("settingsPanel.claudeDir.missing", { path: "/mnt/x" })],
      ["not_dir", copyText("settingsPanel.claudeDir.notDir", { path: "/mnt/x" })],
      ["no_records", copyText("settingsPanel.claudeDir.noRecords", { path: "/mnt/x" })],
    ];
    for (const [state, want] of cases) {
      invokeMock.mockReset();
      asked = [];
      backend(() => chanReply({ state }));
      expect(await claudeDirProblem("/mnt/x")).toBe(want);
      expect(asked).toEqual(["agent-home-check"]);
    }
  });

  it("★ 问不到 ⇒ 说没法确认；码不认识 ⇒ 说认不出（不当能用）", async () => {
    backend(() => ({ err: { Hop: { idx: 0, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] }));
    expect(await claudeDirProblem("/h/y")).toMatch(copyPattern("settingsPanel.claudeDir.uncheckable", { path: "/h/y" }));
    invokeMock.mockReset();
    backend(() => chanReply({ state: "maybe" }));
    const said = await claudeDirProblem("/h/y");
    expect(said).not.toBeNull();
    expect(said).toContain(copyText("peerVersion.said.unreadable", { machine: copyText("control.machine.local") }));
  });
});
