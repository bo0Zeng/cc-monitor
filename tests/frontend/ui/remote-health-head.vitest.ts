/**
 * 远端健康提示：壳带了复制详情就出［复制详情］。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { copyText } from "../../../src/frontend/ui/copy-table";

describe("远端健康提示：壳带了复制详情就出［复制详情］", () => {
  it("文件窗口开了又退：那一句上屏、退出状态与错误输出在详情里；没带详情的那条不出按钮", async () => {
    const { listen } = await import("@tauri-apps/api/event");
    const { bindRemoteHealthToast } = await import("../../../src/frontend/ui/remote-health");
    bindRemoteHealthToast();
    const handler = vi.mocked(listen).mock.calls.at(-1)![1] as (e: { payload: unknown }) => void;
    handler({ payload: { origin: "devbox", kind: "filewin-exit", message: "窗口开了又退 · 夹具", detail: "码：exit status: 7" } });
    handler({ payload: { origin: "devbox", kind: "overflow", message: "丢了几行", detail: "" } });
    const toasts = [...document.querySelectorAll("[role=status], [role=alert]")];
    const withCopy = toasts.filter((t) => [...t.querySelectorAll("button")].some((b) => b.textContent === copyText("detail.act.copy")));
    expect(withCopy.length, "带详情的那条该有［复制详情］，没带的不该有").toBe(1);
    expect(withCopy[0].textContent).toContain("窗口开了又退");
  });
});
