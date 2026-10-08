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
  isRefusal: (e: unknown) =>
    (e as { refused?: boolean } | null)?.refused === true,
}));
const term = vi.hoisted(() => ({ openTerminal: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-open")>()),
  openTerminal: term.openTerminal,
}));
const mint = vi.hoisted(() => ({ mintFreshTmuxName: vi.fn(), refuseUnmintable: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-name-mint", () => mint);
vi.mock("../../../src/frontend/ui/resync", () => ({
  offerResyncRetry: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
vi.mock("../../../src/frontend/ui/launch-arrival", () => ({
  expectArrival: vi.fn(),
  awaitArrival: vi.fn().mockResolvedValue(true),
  arrivedBody: (o: string) => `报出了@${o}`,
}));

import { toast as showActionFailureToast } from "../../../src/frontend/ui/kit/toast";
import { expectArrival } from "../../../src/frontend/ui/launch-arrival";
import {
  runRemoteResume,
  runRemoteAttach,
} from "../../../src/frontend/ui/remote-launch-run";
import { NoTerminalWindow } from "../../../src/frontend/ui/terminal-open";
import type { CliRenderRequest } from "../../../src/frontend/ui/launch-cli-wire";
import { configuredLauncherFor } from "../../../src/frontend/ui/launch-requests";
import { ControlError } from "../../../src/frontend/ui/control-said";
import { copyText } from "../../../src/frontend/ui/copy-table";

const toastMock = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;
const arrivalMock = expectArrival as unknown as ReturnType<typeof vi.fn>;

/** 后端那一行的替身：中性、认得出（判的是「交出去的就是它」，不是它长得像不像一条真命令）。 */
const lineFor = (req: CliRenderRequest): string =>
  `ccm <rendered:${JSON.stringify(req)}>`;

function stubClipboard(writeText: (t: string) => Promise<void>): void {
  Object.defineProperty(globalThis.navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

function requests(): CliRenderRequest[] {
  return render.renderCli.mock.calls.map((c) => c[1] as CliRenderRequest);
}

beforeEach(() => {
  vi.clearAllMocks();
  render.renderCli.mockImplementation(
    async (_o: string, req: CliRenderRequest) => ({
      cmd: lineFor(req),
      account: null,
    }),
  );
  render.planLocalLaunch.mockResolvedValue({
    cmd: "ccm -- --attach n-cc",
    account: null,
  });
  term.openTerminal.mockResolvedValue(undefined);
  mint.mintFreshTmuxName.mockResolvedValue({ ok: true, name: "w-cc" });
  stubClipboard(vi.fn().mockResolvedValue(undefined));
});

describe("那台说「要的号选不了」⇒ 不开窗、说清、给显式选择", () => {
  it("★ 跟随 ⇒ 那台回 account_unavailable ⇒ 零次开窗 ＋ 一条可点提示；点了 ⇒ 点名替代号再问一次、开窗", async () => {
    const refusal = Object.assign(
      new ControlError(
        "号选不了",
        "launch-render-cli refused: account_unavailable",
      ),
      {},
    );
    Object.defineProperty(refusal, "error", {
      value: {
        layer: "peer",
        why: "refused",
        body: new TextEncoder().encode(
          JSON.stringify({
            code: "account_unavailable",
            message: "m",
            data: {
              requested: "z",
              pinned: true,
              listKnown: true,
              alternative: "b",
            },
          }),
        ),
      },
    });
    render.renderCli.mockImplementation(
      async (_o: string, req: CliRenderRequest) => {
        if (req.account.kind === "follow") throw refusal;
        return {
          cmd: lineFor(req),
          account: { name: "b", configDir: "/h/b", model: null },
        };
      },
    );
    expect(await runRemoteResume("devbox", "claude", "sid-1", "/p", "")).toBe(
      false,
    );
    expect(term.openTerminal).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledTimes(1);
    const [title, body, opts] = toastMock.mock.calls[0] as [
      string,
      string,
      { onClick?: () => void },
    ];
    expect(title).toBe(copyText("accountPick.refused.title"));
    expect(body).toBe(copyText("accountPick.refused.pinGoneToCurrent", { name: "z", current: "b" }));
    opts.onClick!();
    await vi.waitFor(() => expect(term.openTerminal).toHaveBeenCalledTimes(1));
    expect(requests().map((r) => r.account)).toEqual([
      { kind: "follow" },
      { kind: "named", name: "b" },
    ]);
  });

  it("开窗之前问 preflight：收的是那台判出来的号的目录；说不起 ⇒ 不开窗", async () => {
    render.renderCli.mockImplementation(
      async (_o: string, req: CliRenderRequest) => ({
        cmd: lineFor(req),
        account: { name: "z", configDir: "/h/z", model: null },
      }),
    );
    const preflight = vi.fn().mockResolvedValue(false);
    expect(
      await runRemoteResume("devbox", "claude", "sid-1", "/p", "", { preflight }),
    ).toBe(false);
    expect(preflight).toHaveBeenCalledWith("/h/z");
    expect(term.openTerminal).not.toHaveBeenCalled();
  });
});

describe("每条远端起会话路径：问那台要那一行，原样交给终端", () => {
  it("直连 resume：容器 none；交给终端的就是那一行", async () => {
    expect(await runRemoteResume("devbox", "claude", "sid-1", "/p", "")).toBe(
      true,
    );
    const [req] = requests();
    expect(req.container).toEqual({ kind: "none" });
    expect(req.action).toEqual({ kind: "resume", sid: "sid-1" });
    expect(req.agent).toBe("claude");
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req));
    expect(arrivalMock).toHaveBeenCalledWith(
      expect.objectContaining({
        origin: "devbox",
        match: { sid: "sid-1" },
        tmuxName: null,
      }),
    );
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
    expect(await runRemoteResume("devbox", "claude", "-x", "/p", "claude")).toBe(
      false,
    );
    expect(term.openTerminal).not.toHaveBeenCalled();
    expect(writeText).not.toHaveBeenCalled();
    expect(toastMock).toHaveBeenCalledWith(
      expect.any(String),
      expect.stringContaining("会话 ID 不合法"),
    );
  });

  it("开不了终端 ⇒ 剪贴板里是那一行 ＋ 提示里带原因与那一行", async () => {
    term.openTerminal.mockRejectedValue(new Error("wt 起不来"));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    expect(await runRemoteResume("devbox", "claude", "sid-1", "/p", "")).toBe(
      false,
    );
    const line = lineFor(requests()[0]);
    expect(writeText).toHaveBeenCalledWith(line);
    expect(String(toastMock.mock.calls[0][1])).toContain(line);
  });

  it("壳回「不开窗」那个结局（POSIX 既定设计）⇒ 标题是「已复制 · 本机不开终端窗口」那一句，不叫失败", async () => {
    term.openTerminal.mockRejectedValue(new NoTerminalWindow());
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResume("<local>", "claude", "sid-1", "/p", "claude");
    expect(String(toastMock.mock.calls[0][0])).toBe(
      copyText("remoteLaunchRun.copyFallback.noWindowCopied"),
    );
  });

  it("按结局判、不按字判：真失败的话里就算带着「不开窗」那一句的原文，也照样叫失败", async () => {
    term.openTerminal.mockRejectedValue(
      new Error(copyText("rsLaunch.posix.noTerminalWindow")),
    );
    stubClipboard(vi.fn().mockResolvedValue(undefined));
    await runRemoteResume("<local>", "claude", "sid-1", "/p", "claude");
    const head = String(toastMock.mock.calls[0][0]);
    expect(head).not.toBe(
      copyText("remoteLaunchRun.copyFallback.noWindowCopied"),
    );
    expect(head).not.toBe(
      copyText("remoteLaunchRun.copyFallback.noWindowManual"),
    );
  });
});
