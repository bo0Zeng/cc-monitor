// F41 runRemoteResume 的分支测试(vitest + jsdom):成功 toast /
// invoke 失败→剪贴板回退 / 剪贴板也失败→诚实文案。错误处理是本功能的心脏,
// tabs.vitest 只测了 resumeTab 的委派分流,这里补 runner 本体。
import { describe, it, expect, vi, beforeEach } from "vitest";

// 〔C4e · 第四波 4C〕就地 resume 那一次键入从 Tauri 命令 `backend_send_into`〔散文墓碑〕改成界面经通道直接说后端的
//   `launch{mode:"send-into"}`（`src/tmux-control.ts::sendInto`）。本文件判的是起会话那几条路的**编排**与 F14 的三态处置 ⇒
//   生产 `invoke` 换成一层翻译（`chan-fake.ts::tmuxControlShim`）：那一发 `chan_call` 照旧按旧名字 `backend_send_into`
//   交给 `invokeMock`，旧回包（`{typed, mayFallBack, reason}`）译成通道那一跳的结局。
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", async () => {
  const { tmuxControlShim } = await import("./test-support/chan-fake");
  return { invoke: tmuxControlShim(invokeMock, "backend_send_into") };
});
vi.mock("../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
// 〔LR2〕原来这里 mock 了 `../src/behavior`（只为那个已删的逃生口 `forceLaunchPayloadRenderer`）；`remote-launch-run.ts` 不再读行为配置。

import { showActionFailureToast } from "../src/error-toast";
import {
  runRemoteResume,
  runRemoteResumeTmux,
  runLocalResumeIntoExistingTmux,
  runNewSessionRemote,
  runRemoteResumeIntoExistingTmux,
  runRemoteLauncher,
  runRemoteAttach, POSIX_NO_WINDOW_MARKER,
  // 〔RL1〕拉起之前问中转地址的那一口（attach 那一道闸直接量它）。
  withRelayEndpoint,
  // 🔴 `设计/80 §8.7` 步 3：全仓唯一的启动期令牌铸币口。
  mintRbindToken,
  // 🔴 `K-R109` `KR109D3`：wire 上那两态同形，是「兜底那条路走得到」的第一环。
  buildCliRenderRequest } from "../src/remote-launch-run";
import { planAttach } from "../src/launch-requests";
import { renderLaunchPayloadStub, STUB_REFUSE_TAG } from "./test-support/launch-render-ipc-stub.ts";
import type { PayloadRenderRequest } from "../src/launch-cli-wire.ts";

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
 * 🔴 〔步 22b·B 2026-09-20〕**外层 tmux 那三格**由后端交出来的那一串的替身。
 * 同上一条的理由：本文件判的是「前端有没有把后端交的那一串原样交出去」，
 * 不是「后端渲得对不对」（那是 `fixtures/tmux-outer-golden.json` 那条逐字节对拍的活）。
 * 刻意**不**长得像 `tmux new-session …` —— 长得像的话，前端偷偷自己拼一条也照样过。
 */
const OUTER_FROM_BACKEND = "<backend-rendered-outer-line>";

/**
 * F03：`renderLaunchCommand` 先给 `probeCcm` 打一发 `invoke("probe_ccm_cli", …)`，早于本测试组
 * 原本唯一关心的 `launch_remote_terminal` 调用——不能再用 `mockResolvedValueOnce`/
 * `mockRejectedValueOnce` 排队（先到的是 probe 调用，会把队列里配给 launch_remote_terminal 的
 * once 值吃掉）。改按 cmd 路由：probe 恒答"未装"（强制走兜底渲染器，保持本文件断言的裸 shell
 * 命令串不变），`launch_remote_terminal` 才落到调用方传入的具体行为。
 */
function mockInvoke(launchTerminal: () => Promise<unknown>): void {
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
    if (cmd === "probe_ccm_cli") {
      return Promise.resolve({ installed: false, version: null, capabilities: [] });
    }
    // U8a-2c-pre：兜底那支的 `container:"none"` 载荷由 **Rust** 渲染
    //（`backend::control::payload::render_payload`）；🔴 **步 22b·B 起外层 tmux 那三格
    // 也走同一条命令**（`设计/90 §4 E` 收官）。本文件的题目是 toast/剪贴板分支，不是渲染 ——
    // 但下面几条断言要看命令内容，所以桩要吐出形状对的串。
    // ⚠ 它**不是**第三份渲染实现：字节的正确性由 `payload-golden.json` 与
    //   `tmux-outer-golden.json` 两份入库夹具的跨语言逐字节对拍钉着。
    // 🔴 **这份镜像只有一个家**（`tests/test-support/launch-render-ipc-stub.ts`）——
    //   原来本文件与 `send-into-backend.vitest.ts` 各写一份，而两份手写镜像必漂。
    if (cmd === "render_launch_payload") {
      return Promise.resolve(
        renderLaunchPayloadStub((args as { req: PayloadRenderRequest }).req),
      );
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
      // 〔第二波 T4〕本地半的令牌握手：resume 起 agent 进程 ⇒ 带着这次铸的令牌。
      rbindToken: expect.stringMatching(/^[0-9a-f]{32}$/),
    });
    expect(writeText).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledTimes(1);
    expect(toastMock.mock.calls[0][0]).toBe("已在远端 resume");
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
      { req: PayloadRenderRequest }).req;
    expect(Array.isArray(req.env)).toBe(true);
    expect(req.args).toContain("sid-9");
    expect(req.nestedEnv.length).toBeGreaterThan(0);
  });

  // ★ fail-closed：后端拒了就**不许**静默用 TS 版糊过去 —— 那等于把一次 fail-closed
  // 变成 fail-open（后端拒的正是非法 configDir / 会裂的 arg 那一类）。
  it("★ 后端拒绝渲染载荷 → 报错，绝不静默回退到 TS 渲染器", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      if (cmd === "render_launch_payload") return Promise.reject("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR");
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResume("aya", "sid-10", "/w", "");
    expect(ok).toBe(false);
    // 走的是「生成不了 resume 命令」那条，不是「拉起失败」—— 且**没有**发起拉起。
    expect(toastMock.mock.calls[0][0]).toBe("生成不了 resume 命令");
    expect(toastMock.mock.calls[0][1]).toContain("后端生成不了启动命令");
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("launch_remote_terminal");
  });

  // ★★ P1：`sendIntoViaBackend` 的 catch 此前把**两件事**混成一件 ——
  // IPC/序列化异常 与 载荷渲染被拒。它的注释推理「都在后端那一跳之前 ⇒ 能证明什么都没
  // 发出去 ⇒ 可回落」**对一半错一半**：没发出去只说明重做不会重复执行，**不说明重做走的那条
  // 路也会拒**。而回落那条正是 TS 兜底渲染器，它对同样输入未必拒 ⇒ 一次 Rust 侧的 fail-closed
  // 被那个 catch 变成 fail-open。分法 = Rust 侧 `payload::refuse()` 打的 `REFUSE:` 标。
  it("★ P1：载荷渲染被拒（带 REFUSE 标）→ refused，不回落到兜底渲染器", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
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
  it("P3 刀3 本机：backend 回报可回落 → 仍然诚实失败，绝不另找一条路重做", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      // `mayFallBack: true` = 证明没发出去。远端据此回落；**本机不许**。
      if (cmd === "backend_send_into")
        return Promise.resolve({ typed: false, reason: "本机后端通道不在", mayFallBack: true });
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runLocalResumeIntoExistingTmux("sid-l1", "l1-cc", "");
    expect(ok).toBe(false);
    expect(toastMock.mock.calls[0][0]).toBe("就地 resume 未执行");
    // 〔C4e〕那句话今天出自文案表（`tmuxControl.channel.localDown`），不再是 monitor 那句「本机后端通道不在」。
    expect(String(toastMock.mock.calls[0][1])).toContain("本机后端没有运行");
    // ★ 最要紧的一格：**一次拉起都没发起**。发起了就说明它去走了第二条路，
    //   而那条路会把可能已经键入过的载荷再提交给正在跑的 claude 一次（F14）。
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("launch_remote_terminal");
  });

  // 〔用户裁定 08-12：attach 用纯 linux bash / windows 的 PowerShell + Windows Terminal〕
  // ⇒ 本机 attach **与远端共用同一条路**（`launch_remote_terminal` + 那条复制回退），
  //   两侧分档由后端那句 `POSIX_NO_TERMINAL_WINDOW` 决定，前端不自己再写一份。
  it("P3 刀3 本机 typed + Linux（后端不开窗口）→ 仍算成功，命令交给用户在自己 bash 里跑", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "backend_send_into") return Promise.resolve({ typed: true, reason: null, mayFallBack: false });
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
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "backend_send_into") return Promise.resolve({ typed: true, reason: null, mayFallBack: false });
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
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      cmds.push(cmd);
      if (cmd === "list_remote_tmux")
        return Promise.resolve([
          { name: "proj-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: null },
        ]);
      // 🔴 〔步 22b·B〕铸出来的名字今天落在**后端渲的外层 tmux 命令**里
      //（`create` 那一格 ⇒ `new-session -d -s <名>`），所以这条桩非配不可。
      if (cmd === "render_launch_payload")
        return Promise.resolve(renderLaunchPayloadStub((args as { req: PayloadRenderRequest }).req));
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

  // 〔FE1〕这一条先前钉的是**缺陷**：「列不出会话 ⇒ 诚实降级用基名」—— 空集铸名 = 不避让 = #76 的形状，
  //   而本机那一侧早写着「绝不退化成空集」。住址 `设计/01 §5` D4「一条都不许静默忽略」。
  //   ⇒ 没问到 ⇒ 不起、出声；正控：远端**没装 tmux**（`null`，确定答案）⇒ 照起、用基名。
  const newSessionRig = (listing: () => Promise<unknown>): string[] => {
    const remoteCmds: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "list_remote_tmux") return listing();
      if (cmd === "render_launch_payload")
        return Promise.resolve(renderLaunchPayloadStub((args as { req: PayloadRenderRequest }).req));
      if (cmd === "launch_remote_terminal") {
        remoteCmds.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    return remoteCmds;
  };

  it("★ 〔FE1〕列不出会话（远端不可达）→ 不起、出声，不拿空集铸名", async () => {
    const remoteCmds = newSessionRig(() => Promise.reject("ssh 抖动"));
    await runNewSessionRemote("aya", "/home/u/proj", "");
    expect(remoteCmds, "没问到名单还起了 —— 名字没避让，可能接进已有会话（#76）").toEqual([]);
    expect(invokeMock.mock.calls.some((c) => c[0] === "launch_remote_terminal")).toBe(false);
    expect(toastMock).toHaveBeenCalledTimes(1);
    expect(toastMock.mock.calls[0][0]).toBe("没有起会话");
    expect(toastMock.mock.calls[0][1]).toContain("aya");
    expect(toastMock.mock.calls[0][1]).toContain("ssh 抖动");
  });

  it("正控：远端没装 tmux（确定答案 `null`）→ 照起、用基名", async () => {
    const remoteCmds = newSessionRig(() => Promise.resolve(null));
    await runNewSessionRemote("aya", "/home/u/proj", "");
    expect(remoteCmds.join("\n")).toContain("proj-cc");
  });

  // 〔C4e · 第四波 4C〕对照那一格换了造法：原来让 `backend_send_into`〔散文墓碑〕的 IPC 抛一个不带标的错（那时它等于
  //   「monitor 那条命令根本没跑」）。键入改走通道之后，**能证明没发出去**的那一档是「那台没有控制通道」
  //   （`hop/NotSent`）；IPC 自己坏了那一种今天拿不准、不回落（`send-into-backend.vitest.ts` ② 那一条）。
  it("★ P1 对照：通道问题（能证明没发出去）→ 仍然回落，行为逐字不变", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      // 通道问题，与载荷本身无关 ⇒ 重做是安全的、且兜底那条路能成
      if (cmd === "backend_send_into") return Promise.resolve({ typed: false, mayFallBack: true, reason: "无通道" });
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

  // 〔DUP1 · `设计/90 §3` 判据 2〕这条原来断「非法 sid ⇒ 前端构造那一步就拒、一次 invoke 都不发」（TS 那份
  // `isValidSessionId`〔散文墓碑〕判的）。今天前端不判 sid：请求照发给渲染侧、sid 在 `resumeSid` 单报一次，
  // 渲染侧（`launch_wire.rs`）过 `shell_quote_core::session_id_ok` 拒并打 `REFUSE:` 标 ⇒ 构造报错 toast、终端不起。
  // 桩不模拟校验闸（见 `launch-render-ipc-stub.ts` 顶注）⇒ 这里直接 mock 一次带标的拒。
  it("非法 sid 前端不判：请求照发（resumeSid 单报），渲染侧拒 ⇒ 构造报错 toast、不起终端", async () => {
    const seen: PayloadRenderRequest[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null);
      if (cmd === "probe_ccm_cli") return Promise.resolve({ installed: false, version: null, capabilities: [] });
      if (cmd === "render_launch_payload") {
        seen.push((args as { req: PayloadRenderRequest }).req);
        return Promise.reject(`${STUB_REFUSE_TAG} 会话 ID "--evil" 不合形状`);
      }
      return Promise.resolve(undefined);
    });
    await runRemoteResume("aya", "--evil", "/p", "");
    expect(seen.map((r) => r.resumeSid)).toEqual(["--evil"]);
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("launch_remote_terminal");
    expect(toastMock.mock.calls[0][0]).toBe("生成不了 resume 命令");
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

  it("runRemoteResumeTmux 成功 → toast「已在 tmux 里 resume」+ 返回 true", async () => {
    mockInvoke(() => Promise.resolve(undefined));
    const ok = await runRemoteResumeTmux("aya", "sid-1", "/p", "", "cc-sid1");
    expect(ok).toBe(true);
    expect(toastMock.mock.calls[0][0]).toBe("已在 tmux 里 resume");
  });
  it("runRemoteResumeTmux 失败 → toast「拉起失败，已复制 tmux resume 命令」+ 返回 false", async () => {
    mockInvoke(() => Promise.reject("boom"));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeTmux("aya", "sid-1", "/p", "", "cc-sid1");
    expect(ok).toBe(false);
    expect(toastMock.mock.calls[0][0]).toBe("拉起失败，已复制 tmux resume 命令");
  });

  it("runRemoteResumeIntoExistingTmux 成功 → toast「已在原来的 tmux 里就地 resume」+ 返回 true", async () => {
    // 〔C4e〕键入那一跳答「没有控制通道」（能证明没发出去）⇒ 回落到整串、终端拉起成功。原来这一格靠 `backend_send_into`
    //   〔散文墓碑〕回 `undefined` 时读 `.typed` 抛出来的那个 TypeError 碰巧走到回落 —— 那是一次意外，不是它要测的东西。
    mockInvoke(() => Promise.resolve(undefined));
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: unknown) =>
      cmd === "backend_send_into" ? Promise.resolve({ typed: false, mayFallBack: true, reason: "无通道" }) : base(cmd, args),
    );
    const ok = await runRemoteResumeIntoExistingTmux("aya", "sid-1", "cc-sid1", "");
    expect(ok).toBe(true);
    expect(toastMock.mock.calls[0][0]).toBe("已在原来的 tmux 里就地 resume");
    expect(toastMock.mock.calls[0][1]).toContain("cc-sid1");
  });
  it("runRemoteResumeIntoExistingTmux 失败 → toast「拉起失败，已复制就地 resume 命令」+ 返回 false", async () => {
    mockInvoke(() => Promise.reject("boom"));
    // 〔C4e〕同上一条：键入那一跳答「没有控制通道」⇒ 回落到整串，终端那一下才失败（本条要测的是那一下）。
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: unknown) =>
      cmd === "backend_send_into" ? Promise.resolve({ typed: false, mayFallBack: true, reason: "无通道" }) : base(cmd, args),
    );
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
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "backend_send_into")
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
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "render_launch_payload") return Promise.resolve("payload");
      if (cmd === "backend_send_into")
        return Promise.resolve({ typed: true, reason: null, mayFallBack: false });
      // 后端拒（本机没装 ccm / 名字过不了闸 …）—— 这条 reject 是**该被看见的读数**：
      // 走到这里说明后端刚刚把载荷键进去了，而「有后端、没有 ccm」是 `R64` 判过的幽灵态。
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
    expect(String(toastMock.mock.calls[0][0])).toContain("生成不了接终端的命令");
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
// # 🔴🔴 〔步 22b·B 2026-09-20〕**这一组重裁了一半：那条路还在，落点换人了。**
//
// `设计/90 §4 E` 收官 ⇒ `renderLaunchCommand` 最后那一格（`container:tmux` 的
// `create` / `send-into` ＋ `action:attach`）改问 `commands.render_launch_payload` 要，
// 请求里带 `outer`，由 `backend::control::payload::render_tmux_outer` 渲。
// ⇒ **上面那句「后端拒 ⇒ 落到座」今天只对前半句**：
// 后端的 **CLI 渲染器**（`render_ccm_launch`）拒了没错，但接手的是**后端的载荷渲染器**，
// 不再是 TS 的座。⇒ 第二环原来断的「真的落到座产的那一串」**是一句今天为假的话**，
// 本刀把它翻成「真的落到**后端渲**的那一串，而且请求里带着对的那一格 `outer`」。
//
// 🔴 **判 A 的结论没变（座留着），而理由换人了 ——** 座今天靠的不是「这条路走得到」，
// 是「它是逐字节金标准（`tmux-outer-golden.json` / `payload-golden.json`）的**左边**」，
// 也就是那份「另一种语言的独立说法」。〔LR2 2026-09-25〕那条理由也到期了：座连同兜底渲染器按
// `设计/00 §2.5 ④` 删了，两份夹具的左边换成手写期望；当年量它的两张消费者表随之删。
// 本组钉的「那一态走到生产入口、落到后端渲的那一串」一个字没动。
//
// 〔墓碑〕第二环原名逐字：「★★ 第二环：那一态走到生产入口上 ⇒ 真的落到座产的那一串（tmux …）」
// —— `tests/evidence/K-R109-deathvalue.md` 里那两行按旧名字记着，**那份留档不动**。
//
// # ⚠ 它买不到什么（如实写）
//
// - 它**不**证明「今天真的有用户走过这条路」—— 那要真机，不在本条射程。
//   它证明的是「这条路在代码上通着，而且触发它的条件今天造得出来」。
// - 它**不**替 `not-installed` 那一半辩护：那一半确实是幽灵态，
//   处置归 `K-R107`（`R64` 的「归属」那一节逐字点的名）。
// - 它**不**验那一串的字节（那是两份入库夹具的活）。它验的是**路由与请求形状**。
describe("KR109D3 探不到那一态今天真走得到 —— 判 A 的机检形态（步 22b·B 起落点是后端）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("★ 第一环：探测**没探出来**（不是「没装」）⇒ wire 上是它自己那一态（〔LR2 · R95b〕不再与「没装」同形）", () => {
    // `ctx`/`plan` 由**生产构造口**产（`planAttach`），不手捏 —— 手捏的那份下一次改字段就馊。
    const { ctx, plan } = planAttach("u1-cc");
    const flaky = buildCliRenderRequest(ctx, plan, { state: "unknown", error: "ssh 抖了一下" });
    const notInstalled = buildCliRenderRequest(ctx, plan, { state: "not-installed" });
    const installed = buildCliRenderRequest(ctx, plan, {
      state: "installed",
      version: "9.9.9",
      capabilities: new Set(["tmux"]),
    });
    // 〔LR2 · R95b〕这里原来钉的是 `K-R95` 那个缺口「今天还在」（`unknown` 与 `not-installed` 在线上同为 `caps: null`），
    //   并写着「它们要是分开了，缺口就补上了，回来重判」。补上了：三态一对一过线，`unknown` 带着原话。
    //   ⇒ 判 A 的前提（「后端拒」不只有「真没装」一种来历）今天由线上第三态直接说出来，不再靠两态同形推。
    expect(flaky.ccm).toEqual({ state: "unknown", error: "ssh 抖了一下" });
    expect(notInstalled.ccm).toEqual({ state: "not-installed" });
    expect(installed.ccm).toEqual({ state: "installed", caps: ["tmux"] });
    expect(flaky.ccm, "「没探出来」与「没装」在线上又同形了 —— R95b 回潮").not.toEqual(notInstalled.ccm);
  });

  it("★★ 第二环：那一态走到生产入口上 ⇒ 落到**后端渲**的那一串，且请求带对的那一格 `outer`", async () => {
    const rendered: string[] = [];
    const reqs: PayloadRenderRequest[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      // 探测**出错** ⇒ `ccm-probe.ts` 回 `{state:"unknown"}`（它不进缓存，下次会重探）。
      if (cmd === "probe_ccm_cli") return Promise.reject("ssh 抖了一下");
      // 后端照 wire 上那两态办事：拿不到能力集 ⇒ 诚实降级（**不是错误**）。
      if (cmd === "render_ccm_launch")
        return Promise.resolve({ ok: false, cmd: null, reason: "远端未装 ccm" });
      // ★ 这一格刻意**不**用那份镜像：本条要判的是「前端把后端交的那一串原样交出去」，
      //   所以给一个中性、认得出的哨兵 —— 拿一条「长得像 tmux 命令」的串会让断言
      //   在前端自己拼串时也蒙混过关（`brief` 第 12 条：别让夹具名字混进断言）。
      if (cmd === "render_launch_payload") {
        reqs.push((args as { req: PayloadRenderRequest }).req);
        return Promise.resolve(OUTER_FROM_BACKEND);
      }
      if (cmd === "launch_remote_terminal") {
        rendered.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResumeTmux("aya", "sid-u1", "/p", "claude", "u1-cc");
    expect(rendered, "生产入口一次拉起都没发起 —— 本条此刻什么都没量到").toHaveLength(1);
    // ① **那一串逐字节是后端交出来的** —— 前端不再自己拼，也不许加工。
    expect(rendered[0]).toBe(OUTER_FROM_BACKEND);
    // ② 送过去的是**那一格**：`create` ＋ 裸会话名 ＋ cwd 归外层（顶层恒 null）。
    expect(reqs, "一次 render_launch_payload 都没发 —— 这一格没接上后端").toHaveLength(1);
    expect(reqs[0].outer).toEqual({
      mode: "create",
      name: "u1-cc",
      quoting: "raw",
      cwd: "/p",
      ccmSid: "sid-u1",
    });
    expect(reqs[0].cwd, "顶层 cwd 必须是 null —— 两个都送后端会 fail-closed 拒").toBeNull();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// `W22B`：**`设计/90 §4 E` 生产切换那道闸** —— 它必须有一条会红的判据
// ─────────────────────────────────────────────────────────────────────────────
//
// # 它是哪道闸，以及为什么它此前**没有**判据
//
// 步 22b·A 的死值验 `M6` 逮到过一次这个形状：把 wire 上「attach 不许带载荷」那道闸
// **整个关掉**，1465 条 Rust 判据**全绿** —— 病因逐字是「既有判据喂的是**渲染器本体**
//（`render_tmux_outer` 自己也有一道同义的闸），**根本走不到 wire 那一层**」。
// 那一拍同时补了 `the_wire_refuses_an_attach_request_that_still_carries_a_payload`。
//
// 🔴 **切生产之后同一形上移了一层，而这一层此前一个判据都没有：**
// wire 那几道闸（attach 带载荷 · 两层 cwd · 空会话名 · `Raw` 越出白名单 · 控制符 ·
// 越界 `@ccm_sid` · 空串 cwd · create/send-into 少送载荷）今天全部**承重** ——
// 它们是生产路上唯一的那排门。而它们只在**请求真的带着 `outer` 走过这条 IPC** 时才生效。
// ⇒ 于是有两种改动能把整排门一次性变成 fail-open，而**两种都不会让任何一条既有判据红**：
//
// ① 最后那一格换回 `buildPayloadRenderRequest`（少送 `outer`）⇒ 后端老老实实渲一条
//    **只有内层载荷**的串（那是 `container:"none"` 的合法形态）⇒ 用户的会话**根本不在
//    tmux 里**，而两侧的闸一个都不响。切之前没有任何东西会红：金标准那两份夹具喂的是
//    `buildTmuxOuterRenderRequest` **本体**，走不到生产调用点 —— 与 `M6` 逐字同形。
// ② `catch` 里把 `renderFallback(plan)` 接回去 ⇒ Rust 那排 fail-closed 门被 TS 座
//    **照拼**过去（座头注逐字「不做校验/转义」）⇒ 一次拒绝变成一条会执行的命令。
//    切之前同样没有东西会红：功能不变砖、门禁全绿。
//
// ⇒ 本组就是那条判据。**三格逐格 ＋ 拒绝时不许有第二条路 ＋ 正控**。
//
// # ⚠ 它买不到什么（如实写）
//
// - **不**验那一串的字节 —— 那是 `fixtures/tmux-outer-golden.json`（外层 13 条）与
//   `payload-golden.json`（内层 10 条）两份入库夹具的跨语言逐字节对拍的活。
//   本组刻意用哨兵串（`OUTER_FROM_BACKEND`）当后端的产物，好让「前端偷偷自己拼一条」
//   在断言上分得开（长得像 `tmux …` 的期望值会让那种改动照样过）。
// - **不**验 Rust 那排门判得对（那由 `launch_tmux_outer_parity_tests.rs` 与
//   `payload_tests.rs` 负责，各自带正控）。本组验的是**它们在生产路上够得着**。
// - **不**做可达性分析：它走的是生产入口（`runRemoteResumeTmux` / `runRemoteAttach` /
//   `runRemoteResumeIntoExistingTmux`）真调一遍，而不是数代码里有没有那几个字。
describe("W22B 外层 tmux 三格的生产切换 —— 那道闸的判据", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  /** 按 cmd 路由：CLI 渲染器恒诚实降级（逼到载荷那条），载荷那条按参数作答。 */
  function routeOuter(payloadReply: () => Promise<unknown>): {
    reqs: PayloadRenderRequest[];
    launched: string[];
  } {
    const reqs: PayloadRenderRequest[] = [];
    const launched: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      if (cmd === "render_ccm_launch")
        return Promise.resolve({ ok: false, cmd: null, reason: "远端未装 ccm" });
      if (cmd === "render_launch_payload") {
        reqs.push((args as { req: PayloadRenderRequest }).req);
        return payloadReply();
      }
      if (cmd === "backend_send_into")
        // 「能证明没发出去」⇒ 回落去渲整串，那正是 `send-into` 那一格的生产路。
        return Promise.resolve({ typed: false, reason: "拿不到控制通道", mayFallBack: true });
      if (cmd === "launch_remote_terminal") {
        launched.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    return { reqs, launched };
  }

  it("★ create 那一格：请求带 `outer.mode==='create'`，cwd 归外层、顶层恒 null", async () => {
    const { reqs, launched } = routeOuter(() => Promise.resolve(OUTER_FROM_BACKEND));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeTmux("aya", "sid-w1", "/w22b", "claude", "w1-cc");
    expect(ok).toBe(true);
    expect(reqs, "没走后端 —— 这一格没接上").toHaveLength(1);
    expect(reqs[0].outer?.mode).toBe("create");
    expect(reqs[0].outer).toMatchObject({ name: "w1-cc", quoting: "raw", cwd: "/w22b" });
    // ★ 两层的 cwd 只许有一个非空 —— 两个都送后端会 fail-closed 拒。
    expect(reqs[0].cwd).toBeNull();
    // ★ 正控：后端交的那一串**原样**交给拉起那一跳（前端不加工、不自己拼）。
    expect(launched).toEqual([OUTER_FROM_BACKEND]);
  });

  it("★ send-into 那一格：请求带 `outer.mode==='send-into'`（回落去渲整串那一跳）", async () => {
    const { reqs, launched } = routeOuter(() => Promise.resolve(OUTER_FROM_BACKEND));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeIntoExistingTmux("aya", "sid-w2", "w2-cc", "claude");
    expect(ok).toBe(true);
    // 第一发是 `sendIntoViaBackend` 的内层载荷（没有 `outer`）；回落那一发才带 `outer`。
    const withOuter = reqs.filter((r) => r.outer !== undefined);
    expect(withOuter, "回落那一跳没带外层 —— 整串会退化成一条没有 tmux 的命令").toHaveLength(1);
    expect(withOuter[0].outer).toEqual({ mode: "send-into", name: "w2-cc", quoting: "raw" });
    expect(withOuter[0].cwd).toBeNull();
    expect(launched).toEqual([OUTER_FROM_BACKEND]);
  });

  it("★ attach 那一格：请求带 `outer.mode==='attach'`，且载荷三个字段全空", async () => {
    const { reqs, launched } = routeOuter(() => Promise.resolve(OUTER_FROM_BACKEND));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteAttach("aya", "w3-cc");
    expect(reqs, "没走后端 —— attach 这一格没接上").toHaveLength(1);
    // ⚠ `quoting` 是 **"quoted"** 不是 `"raw"`：`planAttach` 收的是用户可能自定义的会话名
    //   （「开新 Claude」那一族带空格），它过的是那条宽校验 ⇒ 渲染时要 posix quote。
    //   〔现打逼出来的：本条第一版写 `"raw"`，红了，而**它说对了**。〕
    expect(reqs[0].outer).toEqual({ mode: "attach", name: "w3-cc", quoting: "quoted" });
    // ★ attach 一个 agent 进程都不起 ⇒ 这三个字段必须是空的（带了后端会拒，而不是静默丢）。
    expect({ env: reqs[0].env, args: reqs[0].args, launcher: reqs[0].launcher }).toEqual({
      env: [],
      args: [],
      launcher: "",
    });
    expect(launched).toEqual([OUTER_FROM_BACKEND]);
  });

  it("★★ 后端拒（带 REFUSE 标）⇒ 诚实失败：一次拉起都不发起，也不往剪贴板塞串", async () => {
    const { launched } = routeOuter(() =>
      Promise.reject('REFUSE: tmux 会话名 "w4-cc;evil" 声明成 Raw（裸拼）却不在白名单里'),
    );
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    const ok = await runRemoteResumeTmux("aya", "sid-w4", "/w", "claude", "w4-cc");
    expect(ok).toBe(false);
    // ★★ 最要紧的一格：**没有第二条路**。回落到 TS 座 = 把 Rust 那排门整排变成 fail-open。
    expect(launched, `后端拒了却还是拉起了：${launched.join(" | ")}`).toEqual([]);
    expect(writeText, "把一条被拒的命令复制给用户，等于让他手动执行那一条").not.toHaveBeenCalled();
    expect(String(toastMock.mock.calls[0][0])).toContain("生成不了 tmux resume 命令");
    expect(String(toastMock.mock.calls[0][1])).toContain("后端生成不了 tmux 命令");
  });

  it("★★ 通道异常（不带 REFUSE 标）⇒ 同样诚实失败 —— 这一格已经没有第二条路了", async () => {
    // ⚠ 与 `container:"none"` 那一格的纪律相同，但理由更硬：那一格当年还能说
    //   「回落有 TS 版」，本格连那条都没有 ⇒ 带标不带标处置一致。
    const { launched } = routeOuter(() => Promise.reject("ipc closed"));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    await runRemoteAttach("aya", "w5-cc");
    expect(launched).toEqual([]);
    expect(writeText).not.toHaveBeenCalled();
    expect(String(toastMock.mock.calls[0][0])).toContain("生成不了 attach 命令");
  });
});

// ═══ `设计/80 §8` 步 1：**带启动期令牌的 plan 不许去试 `ccm …` 调用行那条路** ═══════
//
// 🔴 这一组是本件在**生产路上**的唯一一道闸，而它必须存在的理由是一条不对称：
//
//   · 〔LR1〕TS 那份渲染器（它会因令牌维度说不出 CLI 而放弃）已删 —— 它本来就不在生产路上，
//     生产的 CLI 渲染在 Rust 的 `ccm_invocation`；
//   · Rust 那侧的 `CliSpec` **没有** `rbind-token` 这个维度（本件没动它，见交回报告）
//     ⇒ 把一个带令牌的请求送过去，它会**照常渲成功**，只是渲出来的 `ccm …` 里没有令牌。
//
// 失效形态：命令能跑、会话能起、**只有 `↗` 从此拉不到窗口**，而归因指向别处 ——
// 正是 `§8.5 ②`/`§6.2` 那「四档猜」要治的病。⇒ 拒绝必须落在**分派**这一层。
//
// ⚠ 判据读的是**载荷里有没有那条 `EnvOp`**（不是「ctx 里有没有 rbindToken」）：
//   判据必须读渲染器真吃的那个对象，否则「维度没把它推进 plan」这一类回归在这里是隐形的。
describe("设计/80 §8 步 1：带启动期令牌 ⇒ 生产不走 ccm 调用行", () => {
  const TOK = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

  beforeEach(() => {
    vi.clearAllMocks();
  });

  /** ccm **装了而且能力齐全** —— 不这么桩的话 CLI 那条路本来就不会走，本组恒绿。 */
  function routeWithCcmInstalled(): { cmds: string[]; launched: string[] } {
    const cmds: string[] = [];
    const launched: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      cmds.push(cmd);
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({
          installed: true,
          version: "2",
          capabilities: [
            "new", "resume", "attach", "tmux", "account", "model", "cwd",
            "agent", "launcher", "ccm-sid", "print", "detach", "tmux-size",
          ],
        });
      if (cmd === "render_ccm_launch")
        return Promise.resolve({ ok: true, cmd: "<ccm-line-without-the-token>", reason: null });
      if (cmd === "render_launch_payload")
        return Promise.resolve(
          renderLaunchPayloadStub((args as { req: PayloadRenderRequest }).req),
        );
      if (cmd === "launch_remote_terminal") {
        launched.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    return { cmds, launched };
  }

  // 🔴 〔步 3 · 2026-09-23 改写〕**墓碑** —— 这一条原本是
  //    `await runRemoteResume("aya","sid-t0","/w","claude")`（不传 `mods`），
  //    断言它真的走了 `render_ccm_launch`。**步 3 之后那句话不成立了**：
  //    `runRemoteResume` 现在自己铸一个令牌（`withMintedRbindToken`）⇒ 恒走载荷渲染。
  //    ⚠ 但这一条**守的性质不许跟着消失** —— 它是本组的反空真锚：
  //    少了它，「`render_ccm_launch` 这条路整个死了 / 这个 stub 压根没接上」
  //    会让下面那两条读成一片绿。⇒ 换成**今天仍然不铸币的那一格**：`attach`。
  it("★ 对照组先立：`attach` 那一格仍走 `ccm …` 调用行（本组的反空真锚）", async () => {
    const { cmds, launched } = routeWithCcmInstalled();
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteAttach("aya", "t0-cc");
    expect(
      cmds,
      "连 `attach` 都不走 CLI 那条路了 —— 那下面那两条判据什么也没证明（空真）；" +
        "也说明 `render_ccm_launch` 那条路在生产上已经一格不剩，那是一件该被看见的事",
    ).toContain("render_ccm_launch");
    expect(launched).toEqual(["<ccm-line-without-the-token>"]);
  });

  it("★★ `attach` 永不铸币（它一个 agent 进程都不起，令牌无人消费）", async () => {
    const { launched } = routeWithCcmInstalled();
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteAttach("aya", "t0-cc");
    expect(launched).toHaveLength(1);
    expect(
      launched[0],
      "往 attach 里注了一个没人会读的令牌 —— 那只是白白多一处敏感值的落点（设计/80 §8.6 ③）",
    ).not.toContain("CCM_RBIND_TOKEN");
  });

  it("★★ 带令牌 ⇒ 一次 `render_ccm_launch` 都不发，改走后端载荷渲染，且令牌真在串里", async () => {
    const { cmds, launched } = routeWithCcmInstalled();
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResume("aya", "sid-t1", "/w", "claude", { rbindToken: TOK });
    expect(ok).toBe(true);
    expect(
      cmds.filter((c) => c === "render_ccm_launch"),
      "带令牌却去试了 `ccm …` 那条路 —— 后端那侧不认这个维度，会渲出一条**丢了令牌**的命令",
    ).toEqual([]);
    expect(cmds).toContain("render_launch_payload");
    // ★ 正控：交出去的那一串里**真的**有令牌（只验「没走 CLI」的话，
    //   「干脆把令牌整个丢掉」这个变异也会绿）。
    expect(launched).toHaveLength(1);
    expect(launched[0]).toContain(`export CCM_RBIND_TOKEN='${TOK}'; `);
  });

  it("★★ tmux 那一格同理（容器无关）：不试 CLI，令牌落在内层载荷里", async () => {
    const { cmds, launched } = routeWithCcmInstalled();
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResumeTmux("aya", "sid-t2", "/w", "claude", "t2-cc", {
      rbindToken: TOK,
    });
    expect(ok).toBe(true);
    expect(cmds.filter((c) => c === "render_ccm_launch")).toEqual([]);
    expect(launched).toHaveLength(1);
    // tmux 那一格整条内层载荷被 posix-quote 一层塞进 `send-keys` ⇒ 针跟着被 quote。
    expect(launched[0]).toContain(`export CCM_RBIND_TOKEN='\\''${TOK}'\\''; `);
    expect(launched[0]).toContain("tmux new-session -d -s t2-cc");
  });
});

// ═══════ 🔴 `设计/80 §8.7` 步 3：**铸币口** —— 生产真的在产令牌了 ═══════════════
//
// 步 1 落地时**零生产铸币口**（那一刀的收尾话逐字如此）：载荷侧的槽位铺到底了，
// 但没有任何生产代码给它赋值 ⇒ `RBIND_TOKEN_DIMENSION.applies` 恒假 ⇒ 载荷逐字节等于从前。
// 本组守的是那一格被填上了，而且填得**不可猜**。
//
// ⚠ 本组**买不到**：「↗ 真的用这个令牌拉起了那个窗口」。那要图形会话 + Windows，
//   而且还要 `§8.7` 的步 4（`↗` 改走 join）—— 那一步逐字被警告「不要先做」。
describe("设计/80 §8.7 步 3：启动期令牌的铸币口", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  /** 拉起一次、把交出去的那一串抓回来。ccm 恒「未装」⇒ 走载荷渲染器（本组不关心分派）。 */
  function routeLaunch(): { launched: string[] } {
    const launched: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "probe_ccm_cli") return Promise.resolve({ installed: false, version: null, capabilities: [] });
      if (cmd === "render_launch_payload")
        return Promise.resolve(renderLaunchPayloadStub((args as { req: PayloadRenderRequest }).req));
      if (cmd === "backend_send_into") return Promise.resolve({ typed: false, mayFallBack: true, reason: "无通道" });
      if (cmd === "list_remote_tmux") return Promise.resolve([]);
      if (cmd === "launch_remote_terminal") {
        launched.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    return { launched };
  }

  /** 从交出去的那一串里把令牌抠出来（两种引号形态：裸载荷 / 被 send-keys 再 quote 一层）。 */
  function tokenIn(cmd: string): string | null {
    const m = /export CCM_RBIND_TOKEN='(?:\\'')?([0-9a-fA-F]*)/.exec(cmd);
    return m ? m[1] : null;
  }

  // ─── ① 形状与熵 ───────────────────────────────────────────────────────────
  //
  // ⚠ 断言里那条正则是**手写字面量**，不是 `isValidRbindToken` ——
  //   用生产那个校验器的话，两侧同源：把它放宽成 `/^[0-9a-f]*$/` 判据跟着放宽，恒真。
  it("★ 铸出来的令牌是 32 个小写十六进制字符（判据里的形状是手写字面量，不共用生产校验器）", () => {
    for (let i = 0; i < 64; i += 1) {
      expect(mintRbindToken()).toMatch(/^[0-9a-f]{32}$/);
    }
  });

  it("★★ 不可猜 ①：熵**真的**来自平台 CSPRNG（桩掉 getRandomValues，看产物随它变）", () => {
    const real = globalThis.crypto.getRandomValues.bind(globalThis.crypto);
    try {
      // 桩成「全填 0xAB」⇒ 产物必须恰好是 "ab" × 16。
      // 这一条逮的是**最致命的那个变异**：把熵源换成 `Math.random()` / 换成常量 /
      // 换成时间戳 —— 那些改动全都**照样产出 32 个小写十六进制字符**，
      // 只验形状的判据对它们一格都不响。
      Object.defineProperty(globalThis.crypto, "getRandomValues", {
        value: (b: Uint8Array) => {
          b.fill(0xab);
          return b;
        },
        configurable: true,
      });
      expect(mintRbindToken()).toBe("ab".repeat(16));
      // 再换一个值，证明上一条不是碰巧（`0xab` 被写死在生产里也会过上一条）。
      Object.defineProperty(globalThis.crypto, "getRandomValues", {
        value: (b: Uint8Array) => {
          b.forEach((_, i) => {
            b[i] = i;
          });
          return b;
        },
        configurable: true,
      });
      expect(mintRbindToken()).toBe("000102030405060708090a0b0c0d0e0f");
    } finally {
      Object.defineProperty(globalThis.crypto, "getRandomValues", { value: real, configurable: true });
    }
  });

  it("★★ 不可猜 ②：拿不到 CSPRNG ⇒ **throw**，绝不回落 Math.random", () => {
    const real = globalThis.crypto;
    try {
      Object.defineProperty(globalThis, "crypto", { value: undefined, configurable: true });
      expect(() => mintRbindToken()).toThrow(/安全随机数/);
    } finally {
      Object.defineProperty(globalThis, "crypto", { value: real, configurable: true });
    }
    // 第二形：`crypto` 在但那个方法不在（老 jsdom / 裁过的 webview）。
    try {
      Object.defineProperty(globalThis, "crypto", { value: {}, configurable: true });
      expect(() => mintRbindToken()).toThrow(/安全随机数/);
    } finally {
      Object.defineProperty(globalThis, "crypto", { value: real, configurable: true });
    }
  });

  it("★ 不可猜 ③：128 位 ⇒ 1000 次铸币零重复（弱熵源会在这里撞）", () => {
    const seen = new Set<string>();
    for (let i = 0; i < 1000; i += 1) seen.add(mintRbindToken());
    expect(seen.size).toBe(1000);
  });

  // ─── ② 接线：五条「起 agent 进程」的路真的都带上了 ────────────────────────
  //
  // ⚠ 判据读的是**真正交给后端去执行的那一串**（`launch_remote_terminal` 的实参），
  //   不是 `ctx.rbindToken` —— 读中间态的判据看不见「维度没把它推进 plan」那一类回归。
  it("★★ 五条起 agent 进程的路，每一条交出去的串里都有一个新铸的令牌", async () => {
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const got: Record<string, string | null> = {};
    const runs: [string, () => Promise<unknown>][] = [
      ["resume-direct", () => runRemoteResume("aya", "abc-123", "/w", "claude")],
      ["resume-tmux", () => runRemoteResumeTmux("aya", "abc-123", "/w", "claude", "p-cc")],
      ["send-into", () => runRemoteResumeIntoExistingTmux("aya", "abc-123", "p-cc", "claude")],
      ["launcher", () => runRemoteLauncher("aya", "/w", "p-cc", "claude")],
      ["new-session", () => runNewSessionRemote("aya", "/w", "claude")],
    ];
    for (const [label, run] of runs) {
      const { launched } = routeLaunch();
      await run();
      expect(launched, `${label}：压根没交出去一条命令 —— 这一格的读数不可信`).toHaveLength(1);
      got[label] = tokenIn(launched[0]);
      expect(got[label], `${label}：交出去的串里没有启动期令牌`).toMatch(/^[0-9a-f]{32}$/);
    }
    // ★ 每一次拉起铸的是**新的**一个（同一个令牌复用到两个窗口上 ⇒ join 会拉错窗口）。
    const vals = Object.values(got);
    expect(new Set(vals).size, `五次拉起里有令牌重复：${JSON.stringify(got)}`).toBe(vals.length);
  });

  // ─── ③ 〔第二波 T4〕本地半的生产写入方：交给窗口去登记的令牌 == 注进远端环境的令牌 ─────
  //
  // 两侧异源：左 = `render_launch_payload` 收到的**渲染请求**里那条 `export-rbind-token`（远端进程
  // 环境里将会是它）；右 = `launch_remote_terminal` 收到的 `rbindToken`（本地窗口将以它为 marker 登记）。
  // 两边不是同一个 mock 的同一个字段 ⇒ 前端哪天从别处取令牌（另铸一个 / 取 ctx 里的旧值），这里当场分叉。
  function routeBoth(opts: { typed?: boolean } = {}): { rendered: (string | null)[]; handed: (string | null | undefined)[] } {
    const rendered: (string | null)[] = [];
    const handed: (string | null | undefined)[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") return Promise.resolve(null); // 〔RL1〕缺省不注入
      if (cmd === "probe_ccm_cli") return Promise.resolve({ installed: false, version: null, capabilities: [] });
      if (cmd === "render_launch_payload") {
        const req = (args as { req: PayloadRenderRequest }).req;
        const op = req.env.find((o) => o.kind === "export-rbind-token");
        rendered.push(op && "value" in op ? (op.value as string) : null);
        return Promise.resolve(renderLaunchPayloadStub(req));
      }
      if (cmd === "backend_send_into")
        return Promise.resolve(opts.typed ? { typed: true, mayFallBack: false, reason: null } : { typed: false, mayFallBack: true, reason: "无通道" });
      if (cmd === "render_ccm_launch") return Promise.resolve({ ok: true, cmd: "<ccm-attach-line>", reason: null });
      if (cmd === "list_remote_tmux") return Promise.resolve([]);
      if (cmd === "launch_remote_terminal") {
        handed.push((args as { rbindToken?: string | null }).rbindToken);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    return { rendered, handed };
  }

  it("★★ 五条起 agent 进程的路：交给本地窗口登记的令牌 == 渲进远端载荷的那一个（逐条相等）", async () => {
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const runs: [string, () => Promise<unknown>][] = [
      ["resume-direct", () => runRemoteResume("aya", "abc-123", "/w", "claude")],
      ["resume-tmux", () => runRemoteResumeTmux("aya", "abc-123", "/w", "claude", "p-cc")],
      ["send-into（回落整串）", () => runRemoteResumeIntoExistingTmux("aya", "abc-123", "p-cc", "claude")],
      ["launcher", () => runRemoteLauncher("aya", "/w", "p-cc", "claude")],
      ["new-session", () => runNewSessionRemote("aya", "/w", "claude")],
    ];
    for (const [label, run] of runs) {
      const { rendered, handed } = routeBoth();
      await run();
      expect(handed, `${label}：没交出去一个窗口 —— 这一格的读数不可信`).toHaveLength(1);
      expect(handed[0], `${label}：窗口没带令牌 —— 本地表永远收不到它，↗ 按令牌找不到窗口`).toMatch(/^[0-9a-f]{32}$/);
      expect(
        rendered,
        `${label}：交给窗口登记的令牌不是渲进载荷的那一个 —— join 会静默失配`,
      ).toContain(handed[0]);
    }
  });

  it("★★ send-into 已键入（`typed`）⇒ 开出去的那个 attach 窗口带的是**被键入那份载荷**的令牌", async () => {
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const { rendered, handed } = routeBoth({ typed: true });
    await runRemoteResumeIntoExistingTmux("aya", "abc-123", "p-cc", "claude");
    expect(rendered, "量具自检：send-into 那份载荷应当恰好渲了一次").toHaveLength(1);
    expect(rendered[0]).toMatch(/^[0-9a-f]{32}$/);
    expect(handed, "attach 窗口没开 / 开了两个").toHaveLength(1);
    expect(
      handed[0],
      "attach 窗口没带令牌，或带的不是刚被键进 tmux 的那个 claude 的令牌 —— 那个窗口登记不上",
    ).toBe(rendered[0]);
  });

  it("★ `attach` 不铸币 ⇒ 交给窗口的 `rbindToken` 是 `null`（不是一个没人消费的新令牌）", async () => {
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const { handed } = routeBoth();
    await runRemoteAttach("aya", "t0-cc");
    expect(handed).toEqual([null]);
  });

  it("★ 调用方显式传的令牌优先（不被铸币口顶掉）", async () => {
    const { launched } = routeLaunch();
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const given = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";
    await runRemoteResume("aya", "abc-123", "/w", "claude", { rbindToken: given });
    expect(tokenIn(launched[0])).toBe(given);
  });

  it("★★ **空令牌 ≠ 没有令牌**：显式传 `\"\"` 不许被悄悄补一个，必须诚实失败", async () => {
    const { launched } = routeLaunch();
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResume("aya", "abc-123", "/w", "claude", { rbindToken: "" });
    expect(ok, "空令牌被静默补成了一个新铸的 —— 那会把一次铸币 bug 藏起来（Z01 的支点）").toBe(false);
    expect(launched, "空令牌居然拉起来了").toHaveLength(0);
    expect(toastMock).toHaveBeenCalled();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 〔RL1 · 第四波〕R3：拉起之前问一次中转地址，拿到就进载荷；`null` 逐字节不变；attach 不问；拒了就不拉起
// ═════════════════════════════════════════════════════════════════════════════
//
// 异源在哪：地址是桩（「后端」）给的，断言它**原样**出现在交给 `render_launch_payload` 的请求里、
// 并经那份唯一的渲染镜像落进交出去的串；`null` 那一格拿**同一次调用关掉问询**的产物逐字节对拍。
describe("RL1 中转地址进远端载荷", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  const URL_FROM_BACKEND = "http://127.0.0.1:8788/s/claude-code/acct-a/11111111-2222-3333-4444-555555555555";

  function route(relay: (args: unknown) => Promise<unknown>): {
    asked: unknown[];
    reqs: PayloadRenderRequest[];
    launched: string[];
  } {
    const asked: unknown[] = [];
    const reqs: PayloadRenderRequest[] = [];
    const launched: string[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "relay_endpoint_for_launch") {
        asked.push(args);
        return relay(args);
      }
      if (cmd === "probe_ccm_cli")
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      if (cmd === "render_launch_payload") {
        const req = (args as { req: PayloadRenderRequest }).req;
        reqs.push(req);
        return Promise.resolve(renderLaunchPayloadStub(req));
      }
      if (cmd === "backend_send_into")
        return Promise.resolve({ typed: false, reason: "拿不到控制通道", mayFallBack: true });
      if (cmd === "launch_remote_terminal") {
        launched.push((args as { remoteCmd: string }).remoteCmd);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    return { asked, reqs, launched };
  }

  const relayOps = (r: PayloadRenderRequest) => r.env.filter((op) => op.kind === "export-relay-base-url");

  it("★ 四条远端起 agent 的路：后端给了地址 ⇒ 每一份载荷请求里恰好一条、值原样，交出去的串里有那句 export", async () => {
    const paths: [string, () => Promise<unknown>][] = [
      ["resume", () => runRemoteResume("aya", "sid-r1", "/w", "")],
      ["resume-tmux", () => runRemoteResumeTmux("aya", "sid-r2", "/w", "claude", "r2-cc")],
      ["resume-into", () => runRemoteResumeIntoExistingTmux("aya", "sid-r3", "r3-cc", "claude")],
      ["launcher", () => runRemoteLauncher("aya", "/w", "r4-cc", "claude")],
    ];
    for (const [what, go] of paths) {
      vi.clearAllMocks();
      const { asked, reqs, launched } = route(() => Promise.resolve(URL_FROM_BACKEND));
      stubClipboard(vi.fn().mockResolvedValue(undefined));
      await go();
      expect(asked, `${what}：没问中转地址`).toHaveLength(1);
      expect((asked[0] as { origin: string }).origin, what).toBe("aya");
      expect(reqs.length, `${what}：没走载荷渲染`).toBeGreaterThan(0);
      for (const r of reqs) {
        expect(relayOps(r), `${what}：载荷里不是恰好一条中转地址`).toEqual([
          { kind: "export-relay-base-url", value: URL_FROM_BACKEND },
        ]);
      }
      // tmux 那几格整条载荷被再 quote 一层塞进外层命令 ⇒ 只认「那个变量名 ＋ 那一串地址」都在。
      expect(launched.join("\n"), what).toContain("export ANTHROPIC_BASE_URL=");
      // 〔RK1〕地址在渲染串里拆成两半，中间是现读钥匙文件的命令替换（`payload.rs::relay_env_prefix_posix`）。
      const origin = URL_FROM_BACKEND.slice(0, "http://127.0.0.1:8788/".length);
      const routePart = URL_FROM_BACKEND.slice("http://127.0.0.1:8788".length);
      expect(launched.join("\n"), what).toContain(origin);
      expect(launched.join("\n"), what).toContain(routePart);
      expect(launched.join("\n"), what).toContain(".cc-monitor/relay-key");
    }
  });

  it("★ resume 交 sid、开新交 null；没选账号 ⇒ 账号那一格是 base（键名取生成物）", async () => {
    const { asked } = route(() => Promise.resolve(null));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResume("aya", "sid-s1", "/w", "");
    await runRemoteLauncher("aya", "/w", "s2-cc", "claude");
    expect(asked).toEqual([
      { origin: "aya", account: { kind: "base" }, sid: "sid-s1" },
      { origin: "aya", account: { kind: "base" }, sid: null },
    ]);
  });

  it("★ 具名账号 ⇒ 账号那一格带目录与名字", async () => {
    const { asked } = route(() => Promise.resolve(null));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResume("aya", "sid-n1", "/w", "", { configDir: "/home/u/.claude-accts/acct-a", accountName: "acct-a" });
    expect(asked).toEqual([
      {
        origin: "aya",
        account: { kind: "named", configDir: "/home/u/.claude-accts/acct-a", name: "acct-a" },
        sid: "sid-n1",
      },
    ]);
  });

  it("★ 后端说不注入（null）⇒ 载荷请求里零条中转地址", async () => {
    const { reqs } = route(() => Promise.resolve(null));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResumeTmux("aya", "sid-z1", "/w", "claude", "z1-cc");
    expect(reqs.length).toBeGreaterThan(0);
    for (const r of reqs) expect(relayOps(r)).toEqual([]);
  });

  it("★ attach 一个 agent 都不起 ⇒ 不问中转地址（执行器那一层 ＋ `withRelayEndpoint` 自己那一道闸）", async () => {
    const { asked } = route(() => Promise.resolve(URL_FROM_BACKEND));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteAttach("aya", "a1-cc");
    expect(asked).toEqual([]);
    // 执行器今天不经它，但它是导出的口 —— 拿 attach 的 plan 直接喂，闸本身要挡得住（死值验 T2 第一版只量到上面那半）。
    const { ctx, plan } = planAttach("a2-cc");
    const out = await withRelayEndpoint("aya", ctx, plan);
    expect(asked).toEqual([]);
    expect(out).toBe(plan);
  });

  it("★ 后端拒了（apikey 号的中转起不来）⇒ 不拉起、toast 带那句理由", async () => {
    const WHY = "apikey 端点改写不可用：账号 \"acct-a\" 在 [aya] 的 apikey 表里有一行";
    const { launched } = route(() => Promise.reject(WHY));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    const ok = await runRemoteResume("aya", "sid-x1", "/w", "");
    expect(ok).toBe(false);
    expect(launched).toEqual([]);
    expect(toastMock.mock.calls[0][0]).toBe("生成不了 resume 命令");
    expect(String(toastMock.mock.calls[0][1])).toContain(WHY);
  });

  it("★ 本机「就地 resume」也问（origin = 本机），拿到的地址进键入的那份载荷", async () => {
    const { asked, reqs } = route(() => Promise.resolve(URL_FROM_BACKEND));
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runLocalResumeIntoExistingTmux("sid-l9", "l9-cc", "");
    expect(asked).toHaveLength(1);
    expect((asked[0] as { origin: string }).origin).toBe("<local>");
    expect(reqs).toHaveLength(1);
    expect(relayOps(reqs[0])).toEqual([{ kind: "export-relay-base-url", value: URL_FROM_BACKEND }]);
  });
});
