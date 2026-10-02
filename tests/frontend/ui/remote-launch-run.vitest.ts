// 起会话执行器（`remote-launch-run.ts`）：每条起会话路径问那台后端要那一行 `ccm …`、原样交给终端（或键进 pane），
// 失败怎么说、令牌从哪来。渲得对不对归 Rust（`cli-golden.json` 逐字节 ＋ 每条路径都以 `ccm ` 开头那条判据）；
// 这里判的是「前端交出去的就是后端那一行，一个字不加、不拼」与「每条路径问的是哪一形」。
import { describe, it, expect, vi, beforeEach } from "vitest";

const render = vi.hoisted(() => ({
  renderCli: vi.fn(),
  planLocalLaunch: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/launch-render", () => ({
  renderCli: render.renderCli,
  planLocalLaunch: render.planLocalLaunch,
  isRefusal: (e: unknown) => (e as { refused?: boolean } | null)?.refused === true,
}));
const term = vi.hoisted(() => ({ openTerminal: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", () => ({ openTerminal: term.openTerminal }));
const tmux = vi.hoisted(() => ({ sendInto: vi.fn() }));
vi.mock("../../../src/frontend/ui/tmux-control", () => ({ sendInto: tmux.sendInto }));
const mint = vi.hoisted(() => ({ mintFreshTmuxName: vi.fn(), refuseUnmintable: vi.fn() }));
vi.mock("../../../src/frontend/ui/tmux-name-mint", () => mint);
vi.mock("../../../src/frontend/ui/resync", () => ({ offerResyncRetry: vi.fn() }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/launch-arrival", () => ({
  expectArrival: vi.fn(),
  awaitArrival: vi.fn().mockResolvedValue(true),
  arrivedBody: (o: string) => `报出了@${o}`,
}));

import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { expectArrival } from "../../../src/frontend/ui/launch-arrival";
import {
  runRemoteResume,
  runRemoteResumeTmuxAndWait,
  runNewSessionRemote,
  runRemoteLauncher,
  runRemoteAttach,
  mintRbindToken,
  POSIX_NO_WINDOW_MARKER,
} from "../../../src/frontend/ui/remote-launch-run";
import type { CliRenderRequest } from "../../../src/frontend/ui/launch-cli-wire";

const toastMock = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;
const arrivalMock = expectArrival as unknown as ReturnType<typeof vi.fn>;

/** 后端那一行的替身：中性、认得出（判的是「交出去的就是它」，不是它长得像不像一条真命令）。 */
const lineFor = (req: CliRenderRequest): string => `ccm <rendered:${JSON.stringify(req)}>`;

function stubClipboard(writeText: (t: string) => Promise<void>): void {
  Object.defineProperty(globalThis.navigator, "clipboard", { value: { writeText }, configurable: true });
}

function requests(): CliRenderRequest[] {
  return render.renderCli.mock.calls.map((c) => c[1] as CliRenderRequest);
}

beforeEach(() => {
  vi.clearAllMocks();
  render.renderCli.mockImplementation(async (_o: string, req: CliRenderRequest) => lineFor(req));
  render.planLocalLaunch.mockResolvedValue({ cmd: "ccm -- --attach n-cc", launchId: null });
  term.openTerminal.mockResolvedValue(undefined);
  tmux.sendInto.mockResolvedValue({ verdict: "typed" });
  mint.mintFreshTmuxName.mockResolvedValue({ ok: true, name: "w-cc" });
  stubClipboard(vi.fn().mockResolvedValue(undefined));
});

describe("每条远端起会话路径：问那台要那一行，原样交给终端", () => {
  it("直连 resume：容器 none · 带令牌；交给终端的就是那一行、令牌同一个", async () => {
    expect(await runRemoteResume("devbox", "sid-1", "/p", "")).toBe(true);
    const [req] = requests();
    expect(req.container).toEqual({ kind: "none" });
    expect(req.action).toEqual({ kind: "resume", sid: "sid-1" });
    expect(req.rbindToken).toMatch(/^[0-9a-f]{32}$/);
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req), req.rbindToken);
    expect(arrivalMock).toHaveBeenCalledWith(expect.objectContaining({ origin: "devbox", match: { sid: "sid-1" }, tmuxName: null }));
  });

  it("tmux 建会话 resume（换号重启 · 分叉的远端那一跳）：容器 create · 身份标记 · 账号带名字与目录", async () => {
    const r = await runRemoteResumeTmuxAndWait("devbox", "sid-1", "/p", "claude", "cc-sid1", {
      configDir: "/h/.claude-alt/z",
      accountName: "z",
    });
    expect(r).toBe("arrived");
    const [req] = requests();
    expect(req.container).toEqual({ kind: "tmux", name: "cc-sid1", send_into: false });
    expect(req.ccmSid).toBe("sid-1");
    expect(req.account).toEqual({ kind: "account", name: "z", configDir: "/h/.claude-alt/z" });
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req), req.rbindToken);
  });

  it("开新会话（机器卡片 · 历史页）：名字问那台铸、动作 new、按令牌认", async () => {
    await runNewSessionRemote("devbox", "/p", "");
    expect(mint.mintFreshTmuxName).toHaveBeenCalledWith("devbox", "/p");
    const [req] = requests();
    expect(req.action).toEqual({ kind: "new" });
    expect(req.container).toEqual({ kind: "tmux", name: "w-cc", send_into: false });
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req), req.rbindToken);
    expect(arrivalMock).toHaveBeenCalledWith(expect.objectContaining({ match: { token: req.rbindToken } }));
  });

  it("铸不出名字 ⇒ 不起、出声，不自己拼一个", async () => {
    mint.mintFreshTmuxName.mockResolvedValue({ ok: false, why: "那台不可达" });
    await runNewSessionRemote("devbox", "/p", "");
    expect(mint.refuseUnmintable).toHaveBeenCalledWith("devbox", "那台不可达");
    expect(render.renderCli).not.toHaveBeenCalled();
    expect(term.openTerminal).not.toHaveBeenCalled();
  });

  it("接回：动作 attach · 不带令牌 · 窗口不做令牌握手", async () => {
    await runRemoteAttach("devbox", "cc-x");
    const [req] = requests();
    expect(req.action).toEqual({ kind: "attach", name: "cc-x" });
    expect(req.rbindToken).toBeNull();
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req), null);
  });
});

// 〔散文墓碑〕「就地 resume」那组（远端 · 本机各一条编排，键入 ＋ 回落规则）删了：在 tmux 里起与批量收成一条，
//   空 tmux 就地键入由那台后端做（`control/session_batch.rs`，键的仍是直路那一行），界面只交一个 sid、照回答接进去。

describe("失败怎么说", () => {
  it("那台拒了 ⇒ 构造失败提示、不起终端、不往剪贴板塞东西", async () => {
    render.renderCli.mockRejectedValue(new Error("那台说：会话 ID 不合法"));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    expect(await runRemoteResumeTmuxAndWait("devbox", "-x", "/p", "claude", "cc-x")).toBe("unsent");
    expect(term.openTerminal).not.toHaveBeenCalled();
    expect(writeText).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledWith(expect.any(String), expect.stringContaining("会话 ID 不合法"));
  });

  it("开不了终端 ⇒ 剪贴板里是那一行 ＋ 提示里带原因与那一行", async () => {
    term.openTerminal.mockRejectedValue(new Error("wt 起不来"));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    expect(await runRemoteResume("devbox", "sid-1", "/p", "")).toBe(false);
    const line = lineFor(requests()[0]);
    expect(writeText).toHaveBeenCalledWith(line);
    expect(String(toastMock.mock.calls[0][1])).toContain(line);
  });

  it("后端说这是既定设计（POSIX 不开窗口）⇒ 标题不叫失败", async () => {
    term.openTerminal.mockRejectedValue(new Error(`${POSIX_NO_WINDOW_MARKER}，命令交给你`));
    await runRemoteLauncher("<local>", "/p", "w-cc", "claude");
    expect(String(toastMock.mock.calls[0][0])).not.toMatch(/失败/);
  });
});

describe("启动期令牌的铸币口", () => {
  it("★ 铸出来的令牌是 32 个小写十六进制字符（形状是手写字面量）", () => {
    for (let i = 0; i < 64; i += 1) expect(mintRbindToken()).toMatch(/^[0-9a-f]{32}$/);
  });

  it("★★ 熵真的来自平台 CSPRNG（桩掉 getRandomValues，看产物随它变）", () => {
    const real = globalThis.crypto.getRandomValues.bind(globalThis.crypto);
    try {
      Object.defineProperty(globalThis.crypto, "getRandomValues", {
        value: (b: Uint8Array) => b.fill(0xab),
        configurable: true,
      });
      expect(mintRbindToken()).toBe("b".repeat(32));
      Object.defineProperty(globalThis.crypto, "getRandomValues", {
        value: (b: Uint8Array) => {
          b.forEach((_, i) => {
            b[i] = i;
          });
          return b;
        },
        configurable: true,
      });
      expect(mintRbindToken()).toBe("0123456789abcdef0123456789abcdef");
    } finally {
      Object.defineProperty(globalThis.crypto, "getRandomValues", { value: real, configurable: true });
    }
  });

  it("★★ 拿不到 CSPRNG ⇒ throw，绝不回落 Math.random", () => {
    const real = globalThis.crypto;
    try {
      Object.defineProperty(globalThis, "crypto", { value: undefined, configurable: true });
      expect(() => mintRbindToken()).toThrow(/安全随机数/);
      Object.defineProperty(globalThis, "crypto", { value: {}, configurable: true });
      expect(() => mintRbindToken()).toThrow(/安全随机数/);
    } finally {
      Object.defineProperty(globalThis, "crypto", { value: real, configurable: true });
    }
  });

  it("★ 1000 次铸币零重复", () => {
    const seen = new Set<string>();
    for (let i = 0; i < 1000; i += 1) seen.add(mintRbindToken());
    expect(seen.size).toBe(1000);
  });

  it("★★ 起 agent 进程的三条远端路各铸一个新令牌；交给窗口登记的 == 交给那台 ccm 的", async () => {
    await runRemoteResume("devbox", "s1", "/p", "");
    await runRemoteResumeTmuxAndWait("devbox", "s1", "/p", "claude", "cc-s1");
    await runRemoteLauncher("devbox", "/p", "w-cc", "claude");
    const startReqs = requests().filter((r) => r.action.kind !== "attach");
    const tokens = startReqs.map((r) => r.rbindToken);
    expect(tokens.every((t) => typeof t === "string" && /^[0-9a-f]{32}$/.test(t))).toBe(true);
    expect(new Set(tokens).size).toBe(3);
    const handed = term.openTerminal.mock.calls.map((c) => c[2]);
    expect(handed).toEqual(tokens);
  });

  it("★ 调用方显式传的令牌优先；空令牌 ≠ 没有令牌（原样交给那台，由它拒）", async () => {
    await runRemoteResume("devbox", "s1", "/p", "", { rbindToken: "f".repeat(32) });
    expect(requests()[0].rbindToken).toBe("f".repeat(32));
    await runRemoteResume("devbox", "s1", "/p", "", { rbindToken: "" });
    expect(requests()[1].rbindToken).toBe("");
  });
});
