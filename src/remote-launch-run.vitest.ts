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
  runRemoteAttach, POSIX_NO_WINDOW_MARKER } from "./remote-launch-run";

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
interface PayloadRenderReq {
  env: ({ kind: string; value?: string })[];
  cwd: string | null;
  launcher: string;
  args: string[];
  nestedEnv: string[];
}

function mockInvoke(launchTerminal: () => Promise<unknown>): void {
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "probe_ccm_cli") {
      return Promise.resolve({ installed: false, version: null, capabilities: [] });
    }
    // U8a-2c-pre：兜底那支的 `container:"none"` 载荷现在由 **Rust** 渲染
    // （`launch_core::render_payload`）。本文件的题目是 toast/剪贴板分支，不是渲染 ——
    // 但下面几条断言要看命令内容，所以这里给一个**忠实的最小镜像**。
    // ⚠ 它不是第三份实现：渲染的正确性由 `src/backend/control/fixtures/payload-golden.json`
    // 的跨语言逐字节对拍钉住，这里只是让 IPC 桩吐出形状对的串。
    if (cmd === "render_launch_payload") {
      const r = (args as { req: PayloadRenderReq }).req;
      const env = r.env
        .map((op) =>
          op.kind === "export-config-dir"
            ? `export CLAUDE_CONFIG_DIR='${op.value}'; `
            : op.kind === "export-model"
              ? `export ANTHROPIC_MODEL='${op.value}'; `
              : op.kind === "unset-config-dir"
                ? "unset CLAUDE_CONFIG_DIR; "
                : `unset ${r.nestedEnv.join(" ")}; `,
        )
        .join("");
      const cd = r.cwd ? `cd '${r.cwd}' && ` : "";
      return Promise.resolve(`${env}${cd}${[r.launcher, ...r.args].join(" ")}`);
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

  // ★ U8a-2c-pre 的**接缝判据**。没有它，「把兜底 none 那格切回 TS」这个变异全绿 ——
  // 而那正是本轮唯一的实质改动（实测过：加这两条之前两个变异都存活）。
  it("★ 兜底的 container:none 走的是后端渲染，不是 TS 的 renderFallback", async () => {
    mockInvoke(() => Promise.resolve(undefined));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResume("aya", "sid-9", "/w", "");
    const cmds = invokeMock.mock.calls.map((c) => c[0] as string);
    expect(cmds).toContain("render_launch_payload");
    // 送过去的必须是**结构化请求**，不是渲染好的串。
    const req = (invokeMock.mock.calls.find((c) => c[0] === "render_launch_payload")?.[1] as
      { req: PayloadRenderReq }).req;
    expect(Array.isArray(req.env)).toBe(true);
    expect(req.args).toContain("sid-9");
    expect(req.nestedEnv.length).toBeGreaterThan(0);
  });

  // ★ fail-closed：后端拒了就**不许**静默用 TS 版糊过去 —— 那等于把一次 fail-closed
  // 变成 fail-open（后端拒的正是非法 configDir / 会裂的 arg 那一类）。
  it("★ 后端拒绝渲染载荷 → 报错，绝不静默回退到 TS 渲染器", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      if (cmd === "render_launch_payload") return Promise.reject("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR");
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResume("aya", "sid-10", "/w", "");
    expect(ok).toBe(false);
    // 走的是「无法构造 resume 命令」那条，不是「拉起失败」—— 且**没有**发起拉起。
    expect(toastMock.mock.calls[0][0]).toBe("无法构造 resume 命令");
    expect(toastMock.mock.calls[0][1]).toContain("后端拒绝渲染载荷");
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("launch_remote_terminal");
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

  /** ★ U8b：**POSIX 上「不开终端窗口」是既定设计，标题不许叫「拉起失败」。**
   *
   *  在 Linux 上每次点 ↗ 都会走这条路。把一个正常状态报成「失败」，
   *  是把用户训练成「这东西坏了」。 */
  it("后端说「这是既定设计」→ 标题改成「本机不开终端窗口」，不再叫失败", async () => {
    mockInvoke(() =>
      Promise.reject(
        `本机不是 Windows：cc-monitor **${POSIX_NO_WINDOW_MARKER}**（会话容器是 tmux）——命令已复制。这是既定设计，不是没做完。`,
      ),
    );
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    await runRemoteResume("aya", "sid-9", "/p", "");
    expect(toastMock.mock.calls[0][0]).toBe("本机不开终端窗口，命令已复制");
    expect(toastMock.mock.calls[0][0]).not.toContain("失败");
    // 后端那句话（含「为什么」）要原样带给用户，别只留一个标题。
    expect(toastMock.mock.calls[0][1]).toContain(POSIX_NO_WINDOW_MARKER);
  });

  /** 反面同样要钉：**真失败不许被软化**。 */
  it("真失败（配置缺失）仍然叫「拉起失败」", async () => {
    mockInvoke(() => Promise.reject("未找到远端配置: \"aya\""));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResume("aya", "sid-10", "/p", "");
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，已复制 resume 命令");
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
