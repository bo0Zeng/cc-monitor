// F41 runRemoteResume 的分支测试(vitest + jsdom):成功 toast /
// invoke 失败→剪贴板回退 / 剪贴板也失败→诚实文案。错误处理是本功能的心脏,
// tabs.vitest 只测了 resumeTab 的委派分流,这里补 runner 本体。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("./behavior", () => ({ getBehavior: vi.fn().mockResolvedValue({ forceLegacyLaunchRenderer: false }) }));

import { invoke } from "@tauri-apps/api/core";
import { showActionFailureToast } from "./error-toast";
import {
  runRemoteResume,
  runRemoteResumeTmux,
  runRemoteResumeIntoExistingTmux,
  runRemoteLauncher,
  runRemoteAttach,
} from "./remote-launch-run";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const toastMock = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;

function stubClipboard(writeText: (t: string) => Promise<void>): void {
  Object.defineProperty(globalThis.navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

/**
 * F03：`renderLaunchCommand` 先给 `probeCcm` 打一发 `invoke("probe_ccm_cli", …)`，早于本测试组
 * 原本唯一关心的 `launch_remote_terminal` 调用——不能再用 `mockResolvedValueOnce`/
 * `mockRejectedValueOnce` 排队（先到的是 probe 调用，会把队列里配给 launch_remote_terminal 的
 * once 值吃掉）。改按 cmd 路由：probe 恒答"未装"（强制走兜底渲染器，保持本文件断言的裸 shell
 * 命令串不变），`launch_remote_terminal` 才落到调用方传入的具体行为。
 */
function mockInvoke(launchTerminal: () => Promise<unknown>): void {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "probe_ccm_cli") {
      return Promise.resolve({ installed: false, version: null, capabilities: [] });
    }
    if (cmd === "launch_remote_terminal") return launchTerminal();
    return Promise.resolve(undefined);
  });
}

describe("F41 runRemoteResume", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("invoke 成功 → info toast「已拉起」,不碰剪贴板", async () => {
    mockInvoke(() => Promise.resolve(undefined));
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
    mockInvoke(() => Promise.reject("未找到远端配置"));
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
    mockInvoke(() => Promise.reject("boom"));
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

// F03 Phase D 审计发现：其余 4 个 executor 的 toast 文案此前只被 e2e（端到端）/ tabs.vitest（间接、
// mock 掉整个模块）覆盖，唯独 runRemoteResume 有直接的单测断言——若只改了其中一个函数的文案，
// 单测网只会为 runRemoteResume 立刻抓到。补齐其余 4 个的成功/失败文案 smoke test，复用同一套
// mockInvoke 路由。
describe("F52/F03/F53/F51 其余 4 个 executor：toast 文案 smoke test", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("runRemoteResumeTmux 成功 → toast「已拉起 tmux resume」+ 返回 true", async () => {
    mockInvoke(() => Promise.resolve(undefined));
    const ok = await runRemoteResumeTmux("aya", "sid-1", "/p", "", "cc-sid1");
    expect(ok).toBe(true);
    expect(toastMock.mock.calls[0][0]).toBe("已拉起 tmux resume");
  });
  it("runRemoteResumeTmux 失败 → toast「拉起失败，已复制 tmux resume 命令」+ 返回 false", async () => {
    mockInvoke(() => Promise.reject("boom"));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeTmux("aya", "sid-1", "/p", "", "cc-sid1");
    expect(ok).toBe(false);
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，已复制 tmux resume 命令");
  });

  it("runRemoteResumeIntoExistingTmux 成功 → toast「已在原 tmux 就地 resume」+ 返回 true", async () => {
    mockInvoke(() => Promise.resolve(undefined));
    const ok = await runRemoteResumeIntoExistingTmux("aya", "sid-1", "cc-sid1", "");
    expect(ok).toBe(true);
    expect(toastMock.mock.calls[0][0]).toBe("已在原 tmux 就地 resume");
    expect(toastMock.mock.calls[0][1]).toContain("cc-sid1");
  });
  it("runRemoteResumeIntoExistingTmux 失败 → toast「拉起失败，已复制就地 resume 命令」+ 返回 false", async () => {
    mockInvoke(() => Promise.reject("boom"));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeIntoExistingTmux("aya", "sid-1", "cc-sid1", "");
    expect(ok).toBe(false);
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，已复制就地 resume 命令");
  });

  it("runRemoteLauncher 成功 → toast「已拉起「开新 Claude」」", async () => {
    mockInvoke(() => Promise.resolve(undefined));
    await runRemoteLauncher("aya", "/p", "cc-proj", "");
    expect(toastMock.mock.calls[0][0]).toBe("已拉起「开新 Claude」");
    expect(toastMock.mock.calls[0][1]).toContain("cc-proj");
  });
  it("runRemoteLauncher 失败 → toast「拉起失败，已复制命令」", async () => {
    mockInvoke(() => Promise.reject("boom"));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteLauncher("aya", "/p", "cc-proj", "");
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，已复制命令");
  });

  it("runRemoteAttach 成功 → toast「已拉起 tmux attach」", async () => {
    mockInvoke(() => Promise.resolve(undefined));
    await runRemoteAttach("aya", "cc-proj");
    expect(toastMock.mock.calls[0][0]).toBe("已拉起 tmux attach");
    expect(toastMock.mock.calls[0][1]).toContain("cc-proj");
  });
  it("runRemoteAttach 失败 → toast「拉起失败，已复制 attach 命令」", async () => {
    mockInvoke(() => Promise.reject("boom"));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteAttach("aya", "cc-proj");
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，已复制 attach 命令");
  });
});
