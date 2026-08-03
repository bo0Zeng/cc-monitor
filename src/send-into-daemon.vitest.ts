/**
 * U8a-2c-1：**`send-into` 那一格的 `send-keys` 半边真的切到 daemon 了** —— 两个分支各有判据。
 *
 * 为什么必须有这套：它切的是**活的远端主路**（#76 的 idle-tmux 就地复用），而本机没有真远端。
 * 复盘的账早算过 —— 生产切换只要没有判据，变异就成片存活（U8c-2a 那次 4 个、
 * U8a-2c-pre 那次 2 个，其中两个是静默串号）。
 *
 * 盯三件事：
 *  ① daemon 说键入了 ⇒ 终端那条命令**只 attach**（还带 send-keys 就会把载荷键两遍）；
 *  ② 拿不到通道 / daemon 说没键入 / IPC 抛了 ⇒ **逐字回落**到今天那条整串（零行为变化的判据）；
 *  ③ 发给 daemon 的是 `render_launch_payload` 的产物 + **裸会话名**
 *     （`=name:` 的精确匹配形态由 daemon 侧加，两侧各加一次就成了 `==name::`）。
 *
 * 桩法照 `remote-launch-run.vitest.ts`：mock `@tauri-apps/api/core::invoke` 按 cmd 路由 ——
 * 这样走的是**真的 `commands` 包装层**，包装层写错了这套会红。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("./behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ forceLegacyLaunchRenderer: false }),
}));

import { invoke } from "@tauri-apps/api/core";
import { runRemoteResumeIntoExistingTmux } from "./remote-launch-run";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const SID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const NAME = "aaaaaaaa-cc";
const PAYLOAD = "unset CLAUDE_CODE_ENTRYPOINT; claude --resume s-1";

let seen: { cmd: string; args: unknown }[] = [];
let sendIntoReply: unknown = { typed: true, reason: null };
let sendIntoThrows = false;

function route(): void {
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    seen.push({ cmd, args });
    switch (cmd) {
      // 探测恒答「未装 ccm」⇒ send-into 照常走兜底渲染器（本来也是：CLI 渲染器对
      // send-into 恒 `ok:false`，#76 防线）。
      case "probe_ccm_cli":
        return Promise.resolve({ installed: false, version: null, capabilities: [] });
      case "render_launch_payload":
        return Promise.resolve(PAYLOAD);
      case "daemon_send_into":
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

describe("U8a-2c-1 send-into：send-keys 半边走 daemon", () => {
  it("① daemon 键入了 ⇒ 终端那条命令只 attach，绝不再带 send-keys", async () => {
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok).toBe(true);
    expect(
      seen.some((s) => s.cmd === "daemon_send_into"),
      "根本没调 daemon —— 生产切换没生效",
    ).toBe(true);
    const cmd = launchedCmd();
    expect(cmd, "没起终端").toBeTruthy();
    expect(cmd).toContain("attach");
    // ★ 要紧的那条：终端串里若仍有 send-keys，载荷会被键两遍。
    expect(cmd, `终端串里还带着 send-keys：${cmd}`).not.toContain("send-keys");
  });

  it("② daemon 说没键入 ⇒ 逐字回落到今天那条整串（send-keys + attach）", async () => {
    sendIntoReply = { typed: false, reason: "会话已不存在" };
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok).toBe(true);
    expect(launchedCmd()).toContain("send-keys");
    expect(launchedCmd()).toContain("attach");
  });

  it("② IPC 整个抛了也回落，不是报错收场", async () => {
    sendIntoThrows = true;
    const ok = await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    expect(ok, "回落之后应当仍然拉起来了").toBe(true);
    expect(launchedCmd()).toContain("send-keys");
  });

  it("③ 发给 daemon 的载荷 == render_launch_payload 的产物；会话名是裸名", async () => {
    await runRemoteResumeIntoExistingTmux("h1", SID, NAME, "claude");
    const sent = seen.find((s) => s.cmd === "daemon_send_into") as
      | { args: { req: { origin: string; name: string; payload: string } } }
      | undefined;
    expect(sent, "没发 daemon_send_into").toBeTruthy();
    expect(sent!.args.req.payload).toBe(PAYLOAD);
    expect(sent!.args.req.origin).toBe("h1");
    expect(sent!.args.req.name).toBe(NAME);
    // 载荷必须先渲染出来再发 —— 否则就是拿着空载荷去 send-into。
    const order = seen.map((s) => s.cmd).filter((c) => c !== "probe_ccm_cli");
    expect(order.indexOf("render_launch_payload")).toBeLessThan(
      order.indexOf("daemon_send_into"),
    );
  });
});
