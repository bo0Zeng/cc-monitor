// 起会话执行器（`remote-launch-run.ts`）：每条起会话路径问那台后端要那一行 `ccm …`、原样交给终端（或键进 pane），
// 失败怎么说。渲得对不对归 Rust（`cli-golden.json` 逐字节 ＋ 每条路径都以 `ccm ` 开头那条判据）；
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
vi.mock("../../../src/frontend/ui/terminal-name-mint", () => mint);
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
  POSIX_NO_WINDOW_MARKER,
} from "../../../src/frontend/ui/remote-launch-run";
import type { CliRenderRequest } from "../../../src/frontend/ui/launch-cli-wire";
import { configuredLauncherFor } from "../../../src/frontend/ui/launch-requests";

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
  it("直连 resume：容器 none；交给终端的就是那一行", async () => {
    expect(await runRemoteResume("devbox", "claude", "sid-1", "/p", "")).toBe(true);
    const [req] = requests();
    expect(req.container).toEqual({ kind: "none" });
    expect(req.action).toEqual({ kind: "resume", sid: "sid-1" });
    expect(req.agent).toBe("claude");
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req));
    expect(arrivalMock).toHaveBeenCalledWith(expect.objectContaining({ origin: "devbox", match: { sid: "sid-1" }, tmuxName: null }));
  });

  it("tmux 建会话 resume（换号重启 · 分叉的远端那一跳）：容器 create · 身份标记 · 账号带名字与目录", async () => {
    const r = await runRemoteResumeTmuxAndWait("devbox", "claude", "sid-1", "/p", "claude", "cc-sid1", {
      configDir: "/h/.claude-alt/z",
      accountName: "z",
    });
    expect(r).toBe("arrived");
    const [req] = requests();
    expect(req.container).toEqual({ kind: "tmux", name: "cc-sid1", send_into: false });
    expect(req.ccmSid).toBe("sid-1");
    expect(req.account).toEqual({ kind: "account", name: "z", configDir: "/h/.claude-alt/z" });
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req));
  });

  it("开新会话（机器卡片 · 历史页）：名字问那台铸、动作 new、按工作目录认", async () => {
    await runNewSessionRemote("devbox", "claude", "/p", "");
    expect(mint.mintFreshTmuxName).toHaveBeenCalledWith("devbox", "/p");
    const [req] = requests();
    expect(req.action).toEqual({ kind: "new" });
    expect(req.container).toEqual({ kind: "tmux", name: "w-cc", send_into: false });
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req));
    expect(arrivalMock).toHaveBeenCalledWith(expect.objectContaining({ match: { cwd: "/p" } }));
  });

  it("铸不出名字 ⇒ 不起、出声，不自己拼一个", async () => {
    mint.mintFreshTmuxName.mockResolvedValue({ ok: false, why: "那台不可达" });
    await runNewSessionRemote("devbox", "claude", "/p", "");
    expect(mint.refuseUnmintable).toHaveBeenCalledWith("devbox", "那台不可达");
    expect(render.renderCli).not.toHaveBeenCalled();
    expect(term.openTerminal).not.toHaveBeenCalled();
  });

  it("接回：动作 attach，交给终端的就是那一行", async () => {
    await runRemoteAttach("devbox", "claude", "cc-x");
    const [req] = requests();
    expect(req.action).toEqual({ kind: "attach", name: "cc-x" });
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req));
  });
});

// 〔散文墓碑〕「就地 resume」那组（远端 · 本机各一条编排，键入 ＋ 回落规则）删了：在 tmux 里起与批量收成一条，
//   空 tmux 就地键入由那台后端做（`control/session_batch.rs`，键的仍是直路那一行），界面只交一个 sid、照回答接进去。

describe("按会话的那一家起", () => {
  it("Codex 会话：请求说的是 codex，启动器缺省是它自己的那一个（不是 claude）", async () => {
    expect(await runRemoteResume("devbox", "codex", "sid-1", "/p", "")).toBe(true);
    const [req] = requests();
    expect(req.agent).toBe("codex");
    expect(req.launcher).toBe("codex");
    expect(req.defaultLauncher).toBe("codex");
  });

  it("设置里配的 resume 命令（cc / cct 这类）只用在默认那一家的会话上", () => {
    expect(configuredLauncherFor("codex", "cct")).toBe("");
    expect(configuredLauncherFor("claude", "cct")).toBe("cct");
  });
});

describe("失败怎么说", () => {
  it("那台拒了 ⇒ 构造失败提示、不起终端、不往剪贴板塞东西", async () => {
    render.renderCli.mockRejectedValue(new Error("那台说：会话 ID 不合法"));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    expect(await runRemoteResumeTmuxAndWait("devbox", "claude", "-x", "/p", "claude", "cc-x")).toBe("unsent");
    expect(term.openTerminal).not.toHaveBeenCalled();
    expect(writeText).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledWith(expect.any(String), expect.stringContaining("会话 ID 不合法"));
  });

  it("开不了终端 ⇒ 剪贴板里是那一行 ＋ 提示里带原因与那一行", async () => {
    term.openTerminal.mockRejectedValue(new Error("wt 起不来"));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    expect(await runRemoteResume("devbox", "claude", "sid-1", "/p", "")).toBe(false);
    const line = lineFor(requests()[0]);
    expect(writeText).toHaveBeenCalledWith(line);
    expect(String(toastMock.mock.calls[0][1])).toContain(line);
  });

  it("后端说这是既定设计（POSIX 不开窗口）⇒ 标题不叫失败", async () => {
    term.openTerminal.mockRejectedValue(new Error(`${POSIX_NO_WINDOW_MARKER}，命令交给你`));
    await runRemoteLauncher("<local>", "claude", "/p", "w-cc", "claude");
    expect(String(toastMock.mock.calls[0][0])).not.toMatch(/失败/);
  });
});
