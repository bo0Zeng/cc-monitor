/**
 * 要求（streamfix 第 4 条）：起页时 / 那台晚连上时补过来的旧记录不许把当前 tab 切走 —— 旧记录里人说过的话不是「用户刚敲的」。
 * 补历史与刚写进来的由 monitor 那一侧分开说（载荷的 `catch_up`，会话宣告那一刻的行数为界，`snapshot.rs::catch_up_at`）；
 * 界面只认它，不按时刻猜。刚写进来的人话照旧自动跟过去。
 * 起因：起页时的探针 —— 前端就绪之后各会话的记录才到，每个会话里最后那句人话都走了一次 `onRealUserInput`，tab 连跳七下。
 */
import { describe, it, expect, vi } from "vitest";
import { owned } from "../../test-support/owned-tab-manager.ts";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../../test-support/chan-fake");
  return {
    invoke: vi.fn(
      withSessionReads(async (cmd: string, args: Record<string, unknown>) =>
        cmd === "list_user_inputs" ? rig.answerListUserInputs(args as { fromOffset: number }) : undefined,
      ),
    ),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { TabManager } from "../../../src/frontend/ui/tabs";
import { installViewerRig, userLine, type RigPayload } from "../../test-support/session-viewer-rig";

function rig(): TabManager {
  installViewerRig();
  const barEl = document.createElement("div");
  const streamRootEl = document.createElement("div");
  document.body.replaceChildren(barEl, streamRootEl);
  return owned(new TabManager(barEl, streamRootEl));
}

const said = (sid: string, seq: number, catchUp: boolean): RigPayload =>
  ({ ...userLine(seq, `${sid}-u${seq}`, "接着改"), session_id: sid, path: `/p/${sid}.jsonl`, ...(catchUp ? { catch_up: true } : {}) }) as RigPayload;

describe("补历史的人话不切 tab，刚写进来的照旧跟过去", () => {
  it("★ 前端就绪之后才补到的旧记录（`catch_up`）：当前 tab 不动；之后那个会话里刚敲的一句 ⇒ 跟过去", () => {
    const tm = rig();
    tm.onLine(said("a", 0, false) as never); // 第一个 tab 自己成当前
    tm.switchTo("a", "auto");
    tm.onLine(said("b", 0, true) as never);
    tm.onLine(said("c", 0, true) as never);
    expect(tm.activeSessionId()).toBe("a");
    tm.onLine(said("b", 1, false) as never);
    expect(tm.activeSessionId()).toBe("b");
  });
});
