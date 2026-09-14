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
  runLocalResumeIntoExistingTmux,
  runNewSessionRemote,
  runRemoteResumeIntoExistingTmux,
  runRemoteLauncher,
  runRemoteAttach, POSIX_NO_WINDOW_MARKER,
  // 🔴 `K-R109` `KR109D3`：wire 上那两态同形，是「兜底那条路走得到」的第一环。
  buildCliRenderRequest } from "./remote-launch-run";
import { planAttach } from "./launch-requests";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const toastMock = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;

function stubClipboard(writeText: (t: string) => Promise<void>): void {
  Object.defineProperty(globalThis.navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

/**
 * 🔴 `K-R109`：本机后端交出来的那一串 attach 的**替身**。
 *
 * ⚠ **它刻意不是 `ccm attach <名>` 的字面** —— 本文件的题目是「前端有没有把后端交的那一串
 * 原样交出去」，不是「后端渲得对不对」。渲染的正确性由 Rust 那一侧驱动着钉
 *（`history.rs::tests::the_local_backend_renders_an_attach_that_lands_on_the_session_it_just_created`）。
 * 用一个**中性、认得出**的串，是为了让断言判的是「同一串」，
 * 而不是靠「这串长得像 ccm 命令」蒙混过去（`brief` 第 12 条：别让夹具名字混进断言）。
 */
const LOCAL_ATTACH_FROM_BACKEND = "<backend-rendered-attach-line>";

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
    // （`backend::control::payload::render_payload`）。本文件的题目是 toast/剪贴板分支，不是渲染 ——
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

  // ★★ P1：`sendIntoViaDaemon` 的 catch 此前把**两件事**混成一件 ——
  // IPC/序列化异常 与 载荷渲染被拒。它的注释推理「都在 daemon 那一跳之前 ⇒ 能证明什么都没
  // 发出去 ⇒ 可回落」**对一半错一半**：没发出去只说明重做不会重复执行，**不说明重做走的那条
  // 路也会拒**。而回落那条正是 TS 兜底渲染器，它对同样输入未必拒 ⇒ 一次 Rust 侧的 fail-closed
  // 被那个 catch 变成 fail-open。分法 = Rust 侧 `payload::refuse()` 打的 `REFUSE:` 标。
  it("★ P1：载荷渲染被拒（带 REFUSE 标）→ refused，不回落到兜底渲染器", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      // Rust 侧 `refuse()` 的产物形态：`REFUSE: <人读原因>`
      if (cmd === "render_launch_payload")
        return Promise.reject("REFUSE: 拒绝拼入命令：非法 CLAUDE_CONFIG_DIR \"/x;rm\"");
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeIntoExistingTmux("aya", "sid-p1", "cc-p1", "");
    expect(ok).toBe(false);
    // 用户必须看见（这次就地 resume 没做成），且 toast 带得上原因
    expect(toastMock.mock.calls[0][0]).toBe("就地 resume 未执行");
    expect(String(toastMock.mock.calls[0][1])).toContain("REFUSE:");
    // ★ 最要紧的一格：**没有**发起拉起 —— 也就是没有回落到兜底渲染器那条整串
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("launch_remote_terminal");
  });

  // ★★ P3 刀 3：**本机**就地 resume。它与上面那条远端的分水岭只有一处 ——
  // 远端在 `fallback` 时会去渲染整串重做一遍；**本机没有那条路，也不许造**
  //（`C1` 逐字排除「给本地单写一套控制逻辑」）。
  it("P3 刀3 本机：daemon 回报可回落 → 仍然诚实失败，绝不另找一条路重做", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      // `mayFallBack: true` = 证明没发出去。远端据此回落；**本机不许**。
      if (cmd === "daemon_send_into")
        return Promise.resolve({ typed: false, reason: "本机 daemon 通道不在", mayFallBack: true });
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runLocalResumeIntoExistingTmux("sid-l1", "l1-cc", "");
    expect(ok).toBe(false);
    expect(toastMock.mock.calls[0][0]).toBe("就地 resume 未执行");
    expect(String(toastMock.mock.calls[0][1])).toContain("本机 daemon 通道不在");
    // ★ 最要紧的一格：**一次拉起都没发起**。发起了就说明它去走了第二条路，
    //   而那条路会把可能已经键入过的载荷再提交给正在跑的 claude 一次（F14）。
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("launch_remote_terminal");
  });

  // 〔用户裁定 08-12：attach 用纯 linux bash / windows 的 PowerShell + Windows Terminal〕
  // ⇒ 本机 attach **与远端共用同一条路**（`launch_remote_terminal` + 那条复制回退），
  //   两侧分档由后端那句 `POSIX_NO_TERMINAL_WINDOW` 决定，前端不自己再写一份。
  it("P3 刀3 本机 typed + Linux（后端不开窗口）→ 仍算成功，命令交给用户在自己 bash 里跑", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "daemon_send_into") return Promise.resolve({ typed: true, reason: null, mayFallBack: false });
      // 🔴 `K-R109`：attach 那一句**归本机后端产**（`R61` 裁定三）⇒ 这里是它的替身。
      if (cmd === "render_local_attach") return Promise.resolve(LOCAL_ATTACH_FROM_BACKEND);
      // 后端在非 Windows 上的既定回答（含跨语言标记）。
      if (cmd === "launch_remote_terminal")
        return Promise.reject(`本机不是 Windows：cc-monitor **${POSIX_NO_WINDOW_MARKER}**（会话容器是 tmux）`);
      return Promise.resolve(undefined);
    });
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    const ok = await runLocalResumeIntoExistingTmux("sid-l2", "l2-cc", "");
    // ★ 就地 resume 成了；**attach 开不开得了窗口不改变这个结论**（两件事别混成一件）。
    expect(ok).toBe(true);
    // 🔴 `K-R109`：交给用户的那一串**逐字节等于后端交出来的那一串** ——
    //   前端一个字都不拼（`K-R109` 之前这里断言的是 `tmux attach -t '=l2-cc:'`，
    //   那时语法的主人是前端座 `session-backend.ts`）。
    expect(writeText.mock.calls[0][0]).toBe(LOCAL_ATTACH_FROM_BACKEND);
    expect(String(writeText.mock.calls[0][0])).not.toContain("ssh");
    // ★ 反向：座那条语法**不许**再从这条路上冒出来（回落到前端拼串 = §31 第①条那条禁令）。
    expect(String(writeText.mock.calls[0][0])).not.toContain("tmux attach");
    // 标题按后端的声明分档成「既定设计」，不是「拉起失败」。
    expect(String(toastMock.mock.calls[0][0])).toContain("本机不开终端窗口");
    // 正文不许照抄远端那句「到远端 [...] 的 ssh 终端粘贴执行」。
    expect(String(toastMock.mock.calls[0][1])).toContain("在你自己的 bash 里执行");
    expect(String(toastMock.mock.calls[0][1])).not.toContain("ssh 终端");
  });

  it("P3 刀3 本机 typed + Windows（wt + PowerShell 起来了）→ 不复制、不弹既定设计文案", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "daemon_send_into") return Promise.resolve({ typed: true, reason: null, mayFallBack: false });
      if (cmd === "render_local_attach") return Promise.resolve(LOCAL_ATTACH_FROM_BACKEND);
      if (cmd === "launch_remote_terminal") return Promise.resolve(undefined); // 窗口开成了
      return Promise.resolve(undefined);
    });
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    const ok = await runLocalResumeIntoExistingTmux("sid-l3", "l3-cc", "");
    expect(ok).toBe(true);
    expect(writeText).not.toHaveBeenCalled();
    expect(String(toastMock.mock.calls[0][0])).toBe("已就地 resume");
  });

  // ★★ 08-12：「在该目录起新会话」的默认名必须过铸名口。
  //
  // 全仓 `deriveTmuxName` 只有两个生产调用点，`machine-card.ts` 那个 F13 修过、
  // 这个漏了 ⇒ 同一个 cwd 点两次会派生同名 ⇒ 撞 create-or-attach 的幂等闸
  // ⇒ 静默接进第一个会话（issue #76 那一族）。
  //
  // 两条：撞了要**让**；列不出来要**诚实降级**（不避让，但也别挡住起会话）。
  it("同 cwd 已有会话 → 起新会话的名字让到 -2，不撞进幂等闸", async () => {
    const cmds: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      cmds.push(cmd);
      if (cmd === "list_remote_tmux")
        return Promise.resolve([
          { name: "proj-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: null },
        ]);
      if (cmd === "launch_remote_terminal") {
        remoteCmds.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    const remoteCmds: string[] = [];
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runNewSessionRemote("aya", "/home/u/proj", "");
    const sent = remoteCmds.join("\n");
    expect(sent).toContain("proj-cc-2");
    // ★ 不许还是那个裸基名 —— 撞上去就是「以为开了新的，其实回到了旧的」。
    expect(sent).not.toMatch(/[^-]proj-cc[^-0-9]/);
  });

  it("列不出会话（远端不可达）→ 诚实降级用基名，不因为查询失败挡住起会话", async () => {
    const remoteCmds: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "list_remote_tmux") return Promise.reject("ssh 抖动");
      if (cmd === "launch_remote_terminal") {
        remoteCmds.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runNewSessionRemote("aya", "/home/u/proj", "");
    expect(remoteCmds.join("\n")).toContain("proj-cc");
  });

  it("★ P1 对照：IPC 异常（无 REFUSE 标）→ 仍然回落，行为逐字不变", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      // 通道问题，与载荷本身无关 ⇒ 重做是安全的、且兜底那条路能成
      if (cmd === "daemon_send_into") return Promise.reject("ipc closed");
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeIntoExistingTmux("aya", "sid-p1b", "cc-p1b", "");
    expect(ok).toBe(true);
    // 回落成功 ⇒ 走到了真正的拉起，而且**没有**弹「就地 resume 未执行」
    expect(invokeMock.mock.calls.map((c) => c[0])).toContain("launch_remote_terminal");
    expect(toastMock.mock.calls.map((c) => c[0])).not.toContain("就地 resume 未执行");
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


// ═════════════════════════════════════════════════════════════════════════════
// `K-R109` `KR109D2` / `KR109D3`：**attach 那一句归后端** ＋ **兜底那条路今天还走得到**
// ═════════════════════════════════════════════════════════════════════════════

describe("K-R109 本机 attach 那一句问后端要", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("KR109D2 ★ 就地 resume 之后，attach 那一句是**问后端要**的，参数是那个会话名", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "daemon_send_into")
        return Promise.resolve({ typed: true, reason: null, mayFallBack: false });
      if (cmd === "render_local_attach") return Promise.resolve(LOCAL_ATTACH_FROM_BACKEND);
      if (cmd === "launch_remote_terminal") return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runLocalResumeIntoExistingTmux("sid-l9", "l9-cc", "");
    expect(ok).toBe(true);
    // ① 真的问了后端，而且问的是**那个会话名**（问错名字 = 接进别人的会话，issue #76 那一族）。
    const asked = invokeMock.mock.calls.filter((c) => c[0] === "render_local_attach");
    expect(asked, "本机就地 resume 没有问后端要 attach 那一句").toHaveLength(1);
    expect(asked[0][1]).toEqual({ tmuxName: "l9-cc" });
    // ② 交给拉起那一跳的，**逐字节是后端交出来的那一串**。
    const launched = invokeMock.mock.calls.find((c) => c[0] === "launch_remote_terminal");
    expect((launched?.[1] as { remoteCmd: string }).remoteCmd).toBe(LOCAL_ATTACH_FROM_BACKEND);
  });

  it("KR109D2 ★ 后端渲不出来 ⇒ **诚实失败**，不许回落到前端自己拼一条 tmux attach", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "daemon_send_into")
        return Promise.resolve({ typed: true, reason: null, mayFallBack: false });
      // 后端拒（本机没装 ccm / 名字过不了闸 …）—— 这条 reject 是**该被看见的读数**：
      // 走到这里说明 daemon 刚刚把载荷键进去了，而「有后端、没有 ccm」是 `R64` 判过的幽灵态。
      if (cmd === "render_local_attach") return Promise.reject("本机没装 ccm");
      return Promise.resolve(undefined);
    });
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    const ok = await runLocalResumeIntoExistingTmux("sid-l10", "l10-cc", "");
    // ★ 就地 resume 本身成了 —— attach 这一跳的成败不改变它（与上面那两条同一个道理）。
    expect(ok).toBe(true);
    // ★★ 最要紧的一格：**一次拉起都没发起**，而且**什么都没往剪贴板里塞**。
    //    发起了就说明它去拼了一条串，而那正是 §31 最终形态第①条禁的事。
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("launch_remote_terminal");
    expect(writeText).not.toHaveBeenCalled();
    expect(String(toastMock.mock.calls[0][0])).toContain("渲不出来");
    expect(String(toastMock.mock.calls[0][1])).toContain("本机没装 ccm");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// `KR109D3`：**「座今天还删不删得掉」判 A** —— 兜底渲染器今天真走得到，而且不是幽灵态
// ─────────────────────────────────────────────────────────────────────────────
//
// # 这一组钉的是什么
//
// `K-R106` 交回时写着「删不掉」，理由是 `launch-render-fallback.ts` 被走到的前提是
// 「后端渲染器拒了」，而那被归成六格表第 ⑤ 格「这台机没装 ccm」= 部署面。
// 🔴 **`R64`〔用 09-13 逐字「不存在什么没装 ccm 装了后端的情况」〕之后，那一格是幽灵态**
// —— 如果拒的理由只有这一条，那条兜底路守的就是一个不可能态。
//
// # 而它不只有这一条，`R64` 自己就把另一条划出去了
//
// `R64` 逐字：「⚠ **别一刀切**：`unknown`（探不到）与 `not-installed`（探到了、没装）
// **不是同一件事**，而本条只否掉后者与「有后端」并存。」
// 而 `ccm-probe.ts` 的三态里 `unknown` **今天就在**（一次 ssh 抖动就是它），
// 它在 wire 上被压成「拿不到能力集」（`caps: null`，`K-R95` 登记过这个缺口）
// ⇒ 后端拒 ⇒ **落到座**。⇒ **那条路今天走得到，而且走到它的不是幽灵态。**
//
// ⇒ `KR109D3` 判 **A**：座留着，**这一条路写成判据钉住**。
//
// # ⚠ 它买不到什么（如实写）
//
// - 它**不**证明「今天真的有用户走过这条路」—— 那要真机，不在本条射程。
//   它证明的是「这条路在代码上通着，而且触发它的条件今天造得出来」。
// - 它**不**替 `not-installed` 那一半辩护：那一半确实是幽灵态，
//   处置归 `K-R107`（`R64` 的「归属」那一节逐字点的名）。
describe("KR109D3 兜底渲染器（座）今天真走得到 —— 判 A 的机检形态", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("★ 第一环：探测**没探出来**（不是「没装」）⇒ wire 上只剩「拿不到能力集」", () => {
    // `ctx`/`plan` 由**生产构造口**产（`planAttach`），不手捏 —— 手捏的那份下一次改字段就馊。
    const { ctx, plan } = planAttach("u1-cc");
    const flaky = buildCliRenderRequest(ctx, plan, { state: "unknown", error: "ssh 抖了一下" });
    // 🔴 这就是 `K-R95` 登记的那个缺口：值那一侧分得开（三态），**线上只有两态**。
    //    它今天仍然在 ⇒ 「后端拒」这件事**不是只有「真没装」一种来历**。
    expect(flaky.caps).toBeNull();
    // 反向锚点：探到了就**不是** null —— 否则上一条是空真（恒 null 照样过）。
    const installed = buildCliRenderRequest(ctx, plan, {
      state: "installed",
      version: "9.9.9",
      capabilities: new Set(["tmux"]),
    });
    expect(installed.caps).not.toBeNull();
    // ★ 三态里那个 `unknown` **今天还在**。它哪天没了（真的收成两态），
    //   本条会红 —— 那时第 ⑤ 格才真的只剩幽灵态，`KR109D3` 要回来重判 A/B。
    //   （`testing.md` 硬规则 11：钉「今天恰好如此」的判据要写清去哪里重新裁定。）
    const notInstalled = buildCliRenderRequest(ctx, plan, { state: "not-installed" });
    expect(notInstalled.caps).toBeNull();
    expect(
      { unknown: flaky.caps, notInstalled: notInstalled.caps },
      "wire 上这两态今天同形 —— 它们要是分开了，`K-R95` 那个缺口就补上了，回来重判",
    ).toEqual({ unknown: null, notInstalled: null });
  });

  it("★★ 第二环：那一态走到生产入口上 ⇒ 真的落到座产的那一串（`tmux …`）", async () => {
    const rendered: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      // 探测**出错** ⇒ `ccm-probe.ts` 回 `{state:"unknown"}`（它不进缓存，下次会重探）。
      if (cmd === "probe_ccm_cli") return Promise.reject("ssh 抖了一下");
      // 后端照 wire 上那两态办事：拿不到能力集 ⇒ 诚实降级（**不是错误**）。
      if (cmd === "render_ccm_launch")
        return Promise.resolve({ ok: false, cmd: null, reason: "远端未装 ccm" });
      if (cmd === "launch_remote_terminal") {
        rendered.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResumeTmux("aya", "sid-u1", "/p", "claude", "u1-cc");
    expect(rendered, "生产入口一次拉起都没发起 —— 本条此刻什么都没量到").toHaveLength(1);
    // ★ 座产的外层 tmux 命令：这一串只可能从 `launch-render-fallback.ts` → `session-backend.ts` 来
    //   （Rust 那条渲染器刚刚拒了，而 `container:"none"` 那一格走的是 `render_launch_payload`）。
    expect(rendered[0]).toContain("tmux new-session");
    expect(rendered[0]).toContain("u1-cc");
  });
});
