// F41 runRemoteResume 的分支测试(vitest + jsdom):成功 toast /
// invoke 失败→剪贴板回退 / 剪贴板也失败→诚实文案。错误处理是本功能的心脏,
// tabs.vitest 只测了 resumeTab 的委派分流,这里补 runner 本体。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { showActionFailureToast } from "./error-toast";
import { runRemoteResume } from "./remote-launch-run";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const toastMock = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;

function stubClipboard(writeText: (t: string) => Promise<void>): void {
  Object.defineProperty(globalThis.navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

describe("F41 runRemoteResume", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("invoke 成功 → info toast「已拉起」,不碰剪贴板", async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    await runRemoteResume("aya", "sid-1", "/home/pi/p", "");
    expect(invokeMock).toHaveBeenCalledWith("launch_remote_terminal", {
      origin: "aya",
      remoteCmd: expect.stringContaining("claude --resume sid-1"),
    });
    expect(writeText).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledTimes(1);
    expect(toastMock.mock.calls[0][0]).toBe("已拉起远端 resume");
  });

  it("invoke 失败 → 剪贴板写入完整命令 + toast 含原因与命令", async () => {
    invokeMock.mockRejectedValueOnce("未找到远端配置");
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    await runRemoteResume("aya", "sid-2", "/home/pi/my p", "cct");
    expect(writeText).toHaveBeenCalledTimes(1);
    const copied = writeText.mock.calls[0][0] as string;
    expect(copied).toContain("cd '/home/pi/my p' && cct --resume sid-2");
    expect(copied).toContain("unset "); // 嵌套 env 前缀在回退文本里保留
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，已复制 resume 命令");
    expect(toastMock.mock.calls[0][1]).toContain("未找到远端配置");
    expect(toastMock.mock.calls[0][1]).toContain(copied);
  });

  it("剪贴板也失败 → 文案改「请手动复制」,命令仍在 toast 里", async () => {
    invokeMock.mockRejectedValueOnce("boom");
    stubClipboard(vi.fn().mockRejectedValue(new Error("no clipboard")));
    await runRemoteResume("aya", "sid-3", "", "");
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，请手动复制以下命令");
    expect(toastMock.mock.calls[0][1]).toContain("claude --resume sid-3");
  });

  it("非法 sid → 构造报错 toast,不 invoke", async () => {
    invokeMock.mockClear();
    await runRemoteResume("aya", "--evil", "/p", "");
    expect(invokeMock).not.toHaveBeenCalled();
    expect(toastMock.mock.calls[0][0]).toBe("无法构造 resume 命令");
  });
});
