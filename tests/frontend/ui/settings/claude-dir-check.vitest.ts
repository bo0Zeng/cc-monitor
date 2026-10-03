/**
 * 设置里填的 Claude 数据目录：存之前问本机后端那个目录在不在。
 * 不在的照收的话，重启后会被悄悄忽略、退回默认目录，输入框却还显示着它。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { claudeDirProblem } from "../../../../src/frontend/ui/settings/claude-dir-check";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { chanReply, refusedReply, type ChanCallArgs } from "../../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function backend(answer: (path: string) => unknown): void {
  invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
    if (cmd !== "chan_call") return undefined;
    if (a.op !== "files-stat" || a.origin !== "<local>") throw new Error(`没料到：${a.op} ${a.origin}`);
    const { path } = JSON.parse(new TextDecoder().decode(new Uint8Array(a.payload))) as { path: string };
    const r = answer(path);
    // `{err, body}` ⇒ 通道交回的失败（与真 invoke 拒绝同形）。
    if (r !== null && typeof r === "object" && "err" in r) throw r;
    return r;
  });
}

describe("Claude 数据目录：存之前问在不在", () => {
  beforeEach(() => invokeMock.mockReset());

  it("★ 是个在的目录 ⇒ 能用；不在 ⇒ 说不在；是文件 ⇒ 说不是目录；问不到 ⇒ 说没法确认", async () => {
    backend((p) => chanReply({ path: p, kind: "dir", size: 0, readonly: false, owner: null, link_target: null }));
    expect(await claudeDirProblem("/h/.claude-x")).toBeNull();

    invokeMock.mockReset();
    backend(() => refusedReply("unreadable", "读不到这个路径"));
    expect(await claudeDirProblem("/mnt/x")).toBe(copyText("settingsPanel.claudeDir.missing", { path: "/mnt/x" }));

    invokeMock.mockReset();
    backend((p) => chanReply({ path: p, kind: "file", size: 3, readonly: false, owner: null, link_target: null }));
    expect(await claudeDirProblem("/h/a.txt")).toBe(copyText("settingsPanel.claudeDir.notDir", { path: "/h/a.txt" }));

    invokeMock.mockReset();
    backend(() => ({ err: { Hop: { idx: 0, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] }));
    expect(await claudeDirProblem("/h/y")).toMatch(/^没法确认 \/h\/y 在不在/);
  });
});
