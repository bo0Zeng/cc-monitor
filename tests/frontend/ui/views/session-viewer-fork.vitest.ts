/**
 * 查看器里「从这一轮分叉」交出去的 sid 照打开时交的那一行的 `sessionId`，不从记录文件名反推
 * （Codex 的记录文件名是 `rollout-<时刻>-<sid>.jsonl`，不是 sid）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  return rig.tauriCoreMock();
});
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../../src/frontend/ui/events", () => ({
  followSession: vi.fn(async () => ({ stop: () => {} })),
}));
vi.mock("../../../../src/frontend/ui/new-session", () => ({ openNewSession: vi.fn() }));

import { installViewerRig, userLine, viewerRig } from "../../../test-support/session-viewer-rig";
import { SessionViewer } from "../../../../src/frontend/ui/views/session-viewer";
import { openNewSession } from "../../../../src/frontend/ui/new-session";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";

const SID = "0199a3b2-7c1d-7e00-8a11-1234567890ab";

beforeEach(() => {
  installViewerRig();
  vi.mocked(openNewSession).mockClear();
});
afterEach(() => vi.unstubAllGlobals());

describe("查看器分叉交的 sid", () => {
  it("★ Codex 形的记录文件名 ⇒ 分叉交的是打开时给的 sessionId", async () => {
    viewerRig.chunk = [userLine(0, "u0", "你好")];
    const v = new SessionViewer();
    document.body.appendChild(v.element);
    await v.load({ jsonlPath: `/h/.codex/sessions/2026/11/10/rollout-2026-11-10T09-00-00-${SID}.jsonl`, sessionId: SID, displayTitle: "T", origin: LOCAL });
    const btn = v.element.querySelector<HTMLButtonElement>('[data-id="u0"] .viewer-branch-btn');
    expect(btn, "那张卡上有分叉那一颗").not.toBeNull();
    btn!.click();
    expect(vi.mocked(openNewSession)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(openNewSession).mock.calls[0][0]).toMatchObject({ origin: LOCAL, fork: { sid: SID, uuid: "u0" } });
  });
});
