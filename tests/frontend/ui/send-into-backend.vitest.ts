/**
 * U8a-2c-1：**`send-into` 那一格的 `send-keys` 半边真的切到后端了** —— 两个分支各有判据。
 *
 * 为什么必须有这套：它切的是**活的远端主路**（#76 的 idle-tmux 就地复用），而本机没有真远端。
 * 复盘的账早算过 —— 生产切换只要没有判据，变异就成片存活（U8c-2a 那次 4 个、
 * U8a-2c-pre 那次 2 个，其中两个是静默串号）。
 *
 * 盯三件事：
 *  ① backend 说键入了 ⇒ 终端那条命令**只 attach**（还带 send-keys 就会把载荷键两遍）；
 *  ② 拿不到通道 / backend 说没键入 / IPC 抛了 ⇒ **逐字回落**到今天那条整串（零行为变化的判据）；
 *  ③ 发给后端的是 `render_launch_payload` 的产物 + **裸会话名**
 *     （`=name:` 的精确匹配形态由后端侧加，两侧各加一次就成了 `==name::`）。
 *
 * 桩法照 `remote-launch-run.vitest.ts`：mock `@tauri-apps/api/core::invoke` 按 cmd 路由 ——
 * 这样走的是**真的 `commands` 包装层**，包装层写错了这套会红。
 */
import { isChanCall, UNSUPPORTED } from "../../test-support/chan-fake";
import { describe, it, expect, vi, beforeEach } from "vitest";

// 就地 resume 那一次键入从 Tauri 命令 `backend_send_into`〔散文墓碑〕改成界面经通道直接说后端的
//   `launch{mode:"send-into"}`（`src/frontend/ui/tmux-control.ts::sendInto`）。本文件判的是起会话那几条路的**编排**与 F14 的三态处置 ⇒
//   生产 `invoke` 换成一层翻译（`chan-fake.ts::tmuxControlShim`）：那一发 `chan_call` 照旧按旧名字 `backend_send_into`
//   交给 `invokeMock`，旧回包（`{typed, mayFallBack, reason}`）译成通道那一跳的结局。
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", async () => {
  const { tmuxControlShim, launchRenderShim } = await import("../../test-support/chan-fake");
  return { invoke: tmuxControlShim(launchRenderShim(invokeMock), "backend_send_into") };
});
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
// 原来这里 mock 了 `../src/behavior`（只为那个已删的逃生口）；`remote-launch-run.ts` 不再读行为配置。

import { runRemoteResumeIntoExistingTmux } from "../../../src/frontend/ui/remote-launch-run";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { renderLaunchPayloadStub } from "../../test-support/launch-render-ipc-stub.ts";
import type { PayloadRenderRequest } from "../../../src/frontend/ui/launch-cli-wire.ts";


const SID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const NAME = "aaaaaaaa-cc";
const PAYLOAD = "unset CLAUDE_CODE_ENTRYPOINT; claude --resume s-1";

let seen: { cmd: string; args: unknown }[] = [];
let sendIntoReply: unknown = { typed: true, reason: null };
let sendIntoThrows = false;

function route(): void {
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    seen.push({ cmd, args });
    // 探测恒答「未装 ccm」⇒ send-into 照常走兜底渲染器（本来也是：CLI 渲染器对
    // send-into 恒 `ok:false`，#76 防线）。探测经通道 `ccm-probe`，「未装」＝ 对端不认这条。
    if (isChanCall(cmd, args, "ccm-probe")) return Promise.reject(UNSUPPORTED);
    switch (cmd) {
      // 🔴 **这一格原来恒返回常量 `PAYLOAD`。**
      // 收官之后，`send-into` 与 `attach` 两格的**外层 tmux 命令**
      // 也从这条命令出来 ⇒ 恒返回内层载荷等于把外层那一层从桩里抹掉，
      // 于是本文件 ①② 那三条（「终端串里只 attach」「逐字回落到 send-keys + attach」）
      // 会一律读到一条没有 tmux 的串 —— 而它们要判的正是那一层。
      // ⇒ 带 `outer` 的请求交给镜像拼（唯一的家：`test-support/launch-render-ipc-stub.ts`）。
      // ⚠ **不带 `outer` 的那一格仍然恒返回 `PAYLOAD`** —— ③ 那条
      //   「发给后端的载荷 == `render_launch_payload` 的产物」拿它当期望值，
      //   换成现渲的串就变成「桩自己和桩自己比」，那条当场失去意义。
      case "render_launch_payload": {
        const req = (args as { req: PayloadRenderRequest }).req;
        return Promise.resolve(
          req.outer === undefined ? PAYLOAD : renderLaunchPayloadStub(req),
        );
      }
      case "backend_send_into":
        return sendIntoThrows
          ? Promise.reject(new Error("控制通道炸了"))
          : Promise.resolve(sendIntoReply);
      // CLI 渲染器对 send-into 恒诚实降级（#76 防线）—— 给一个形状对的应答，
      // 免得桩返回 null 时把噪音混进来。
      case "render_ccm_launch":
        return Promise.resolve({ ok: false, cmd: null, reason: "send-into 无 CLI 等价语法" });
      case "launch_remote_terminal":
        return Promise.resolve();
      default:
        return Promise.resolve(null);
    }
  });
}

function launchedCmd(): string | undefined {
  const hit = seen.find((s) => s.cmd === "launch_remote_terminal");
  return (hit?.args as { remoteCmd?: string } | undefined)?.remoteCmd;
}

beforeEach(() => {
  seen = [];
  sendIntoReply = { typed: true, reason: null };
  sendIntoThrows = false;
  invokeMock.mockReset();
  route();
});

describe("U8a-2c-1 send-into：send-keys 半边走 backend", () => {
  it("① backend 键入了 ⇒ 终端那条命令只 attach，绝不再带 send-keys", async () => {
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok).toBe(true);
    expect(
      seen.some((s) => s.cmd === "backend_send_into"),
      "根本没调 backend —— 生产切换没生效",
    ).toBe(true);
    const cmd = launchedCmd();
    expect(cmd, "没起终端").toBeTruthy();
    expect(cmd).toContain("attach");
    // ★ 要紧的那条：终端串里若仍有 send-keys，载荷会被键两遍。
    expect(cmd, `终端串里还带着 send-keys：${cmd}`).not.toContain("send-keys");
  });

  // ★★ F14 把这条一分为二。原来它写「backend 说没键入 ⇒ 逐字回落」，而 F14 之后
  // 「没键入」分成两种：**能证明没发出去**（可回落）与 **backend 说过话 / 无法证明**（不许回落）。
  // 原夹具 `{typed:false, reason}` 缺 `mayFallBack` ⇒ 改完当天它**红了**，那是设计：
  // 生产上 Rust 恒设该字段，所以缺字段的夹具是**生产不可能的形状**（同 F03 那次 `e2e-si-fixed`）。
  it("② 能证明没发出去（mayFallBack:true）⇒ 逐字回落到今天那条整串（send-keys + attach）", async () => {
    sendIntoReply = { typed: false, reason: "没有可用的控制通道", mayFallBack: true };
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok).toBe(true);
    expect(launchedCmd()).toContain("send-keys");
    expect(launchedCmd()).toContain("attach");
  });

  it("★ ②b backend 说过话（mayFallBack:false）⇒ **绝不回落**，就地失败", async () => {
    sendIntoReply = { typed: false, reason: "拒绝就地 resume：目标未通过身份守卫", mayFallBack: false };
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok, "被门拒绝时不该报成功").toBe(false);
    // 实测：一次都没发起 launch 时 `launchedCmd()` 是 `undefined`（不是空串）。
    expect(
      launchedCmd(),
      "回落了 —— 那条整串没有 §34 的门，等于把一次门拒绝洗成另一条路的成功",
    ).toBeUndefined();
  });

  // 拒绝提示带「对齐后重试」只在**关卡 2** 拒的那一形；「拿不准执行没有」那一形不带
  //   （重试会把载荷再键一遍）。两形都照旧不回落。
  it("★ ②d 关卡 2 拒的 ⇒ 提示可点（对齐后重试）；拿不准执行没有 ⇒ 提示不可点", async () => {
    const toast = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;
    const clickable = async (reply: unknown): Promise<boolean> => {
      toast.mockClear();
      sendIntoReply = reply;
      expect(await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude")).toBe(false);
      expect(launchedCmd()).toBeUndefined();
      expect(toast).toHaveBeenCalledTimes(1);
      return typeof (toast.mock.calls[0][2] as { onClick?: unknown } | undefined)?.onClick === "function";
    };
    expect([
      await clickable({ typed: false, mayFallBack: false, code: "wrong_owner", reason: "不是本工具的会话" }),
      await clickable({ typed: false, mayFallBack: false, reason: "拿不准" }),
    ]).toEqual([true, false]);
  });

  it("★ ②c 读不出 mayFallBack ⇒ **按 fail-closed 当成不许回落**", async () => {
    // 生产上 Rust 恒设该字段；但协议漂移/旧版本时读不出来。
    // 这一档必须**保守**：宁可就地失败，也不要用一条无门的路重做一次可能已经执行的动作。
    sendIntoReply = { typed: false, reason: "字段缺失" };
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok, "读不出该字段时不该乐观回落").toBe(false);
    expect(launchedCmd()).toBeUndefined();
  });

  // 这一条**翻了面**，而且是往严里翻。原来是「IPC 整个抛了也回落」：那时键入走 Tauri 命令
  //   `backend_send_into`〔散文墓碑〕，它自己从不回错，IPC 一抛就说明 monitor 那条命令根本没跑 ⇒ 能证明没发出去。
  //   今天键入经通道（`chan_call`）说，monitor 那一跳交回的失败**一律是分好层的**；解不出层的那一种
  //   （`ours/Broken`：那一跳自己坏了）**拿不准**到没到后端 ⇒ 按最坏算、**不回落**（F14：回落那条整串没有门，
  //   「可能已经键入过」再键一遍就是把载荷第二次提交给正在跑的 claude）。能证明没发出去的那一档（没有控制通道）
  //   照旧回落 —— 见上面 ② 那一条。
  it("② 通道那一跳自己坏了（解不出层）⇒ 拿不准有没有到后端 ⇒ 不回落，诚实失败", async () => {
    sendIntoThrows = true;
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok, "拿不准的时候不许换一条无门的路重做").toBe(false);
    expect(seen.map((x) => x.cmd)).not.toContain("launch_remote_terminal");
  });

  it("③ 发给后端的载荷 == render_launch_payload 的产物；会话名是裸名", async () => {
    await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    const sent = seen.find((s) => s.cmd === "backend_send_into") as
      | { args: { req: { origin: string; name: string; payload: string } } }
      | undefined;
    expect(sent, "没发 backend_send_into").toBeTruthy();
    expect(sent!.args.req.payload).toBe(PAYLOAD);
    expect(sent!.args.req.origin).toBe("h1");
    expect(sent!.args.req.name).toBe(NAME);
    // 载荷必须先渲染出来再发 —— 否则就是拿着空载荷去 send-into。
    const order = seen.filter((s) => !isChanCall(s.cmd, s.args, "ccm-probe")).map((s) => s.cmd);
    expect(order.indexOf("render_launch_payload")).toBeLessThan(
      order.indexOf("backend_send_into"),
    );
  });
});
