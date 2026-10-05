/**
 * 分叉之后起会话的**编排**（推断在那台后端，这里只剩「不知道的问一次 → 交 `sessions-start`」）。
 *
 * 要紧的几条：
 * ① 知道就别问（否则每分叉一次弹一次窗，功能没人用）
 * ② 不知道就**必须**问，且**绝不**自己填一个（号没答 ⇒ 账号 0）
 * ③ 交出去的那一项恒是新 sid、带 `fresh_terminal` 与源会话 `fork_of`（新终端名那台铸）
 */
import { describe, it, expect, vi } from "vitest";
import { slotsToAsk, startForkedSession, type ForkStartDeps } from "../../../src/frontend/ui/fork-start";
import type { ForkLaunch } from "../../../src/frontend/ui/session-writes";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";

type AskFn = ForkStartDeps["ask"];
type StartFn = ForkStartDeps["start"];

function deps(over: { ask?: AskFn; start?: StartFn } = {}) {
  return {
    ask: vi.fn<AskFn>(over.ask ?? (async () => ({}))),
    start: vi.fn<StartFn>(over.start ?? (async () => true)),
  };
}

/** 源会话活着、号是 `z`、在终端里。 */
const LIVE: ForkLaunch = {
  cwd: { kind: "known", value: "/home/u/p", from: "record" },
  account: { kind: "known", value: "z", from: "process" },
  terminal: { kind: "known", value: { host: "tmux", terminal: "tmux-3-7" }, from: "terminal_list" },
};
/** 源会话已退出。 */
const DEAD: ForkLaunch = {
  cwd: { kind: "known", value: "/home/u/p", from: "record" },
  account: { kind: "unknown", why: "exited" },
  terminal: { kind: "unknown", why: "exited" },
};

const run = (origin: string, launch: ForkLaunch, d: ReturnType<typeof deps>) =>
  startForkedSession({ newSessionId: "NEW", sourceSessionId: "SRC", origin, launch }, d);

describe("什么时候问", () => {
  it("★ 三格都知道 → **一次都不问**，直接起", async () => {
    const d = deps();
    expect(await run("devbox", LIVE, d)).toBe("started");
    expect(d.ask).not.toHaveBeenCalled();
  });

  it("★ 源会话已退出 → 问，且**只问一次**；要问的格顺序稳定（号 · 终端）", async () => {
    const d = deps({ ask: async () => ({ account: null, useTmux: false }) });
    await run("devbox", DEAD, d);
    expect(d.ask).toHaveBeenCalledTimes(1);
    expect(d.ask.mock.calls[0][1]).toEqual(["account", "terminal"]);
  });

  it("★ 用户取消 → **什么都不起**，回 cancelled（不是 failed）", async () => {
    const d = deps({ ask: async () => null });
    expect(await run(LOCAL_ORIGIN, DEAD, d)).toBe("cancelled");
    expect(d.start).not.toHaveBeenCalled();
  });

  it("工作目录不知道 → 也问那一格，答的目录交下去", async () => {
    const d = deps({ ask: async () => ({ cwd: "/answered" }) });
    await run(LOCAL_ORIGIN, { ...LIVE, cwd: { kind: "unknown", why: "no_cwd" } }, d);
    expect(d.ask.mock.calls[0][1]).toEqual(["cwd"]);
    expect(d.start.mock.calls[0][0].item.cwd).toBe("/answered");
  });
});

describe("号：知道就照搬，不知道就用用户答的，**绝不自己填**", () => {
  it("活着 → 照搬那台推出的号名", async () => {
    const d = deps();
    await run(LOCAL_ORIGIN, LIVE, d);
    expect(d.start.mock.calls[0][0].item.account).toEqual({ kind: "named", name: "z" });
  });

  it("★ 活着且确认是账号 0 → base", async () => {
    const d = deps();
    await run(LOCAL_ORIGIN, { ...LIVE, account: { kind: "known", value: null, from: "process" } }, d);
    expect(d.start.mock.calls[0][0].item.account).toEqual({ kind: "base" });
  });

  it("活着但说不出号 → 问号那一格", async () => {
    const d = deps({ ask: async () => ({ account: "b" }) });
    await run(LOCAL_ORIGIN, { ...LIVE, account: { kind: "unknown", why: "live_no_account" } }, d);
    expect(d.ask.mock.calls[0][1]).toEqual(["account"]);
    expect(d.start.mock.calls[0][0].item.account).toEqual({ kind: "named", name: "b" });
  });

  it("★ 已退出 + 用户选了某号 → 用它；选账号 0 → base", async () => {
    const picked = deps({ ask: async () => ({ account: "b" }) });
    await run(LOCAL_ORIGIN, DEAD, picked);
    expect(picked.start.mock.calls[0][0].item.account).toEqual({ kind: "named", name: "b" });
    const zero = deps({ ask: async () => ({ account: null }) });
    await run(LOCAL_ORIGIN, DEAD, zero);
    expect(zero.start.mock.calls[0][0].item.account).toEqual({ kind: "base" });
  });

  it("★ 已退出 + 弹窗没答号（只答了别的）→ 落账号 0，**不猜一个**", async () => {
    const d = deps({ ask: async () => ({ useTmux: true }) });
    await run("devbox", DEAD, d);
    expect(d.start.mock.calls[0][0].item.account).toEqual({ kind: "base" });
  });
});

describe("终端那一格", () => {
  it("★ 本机 → 不问终端那一格（本机一律开终端），号照问", async () => {
    const d = deps({ ask: async () => ({ account: null }) });
    await run(LOCAL_ORIGIN, DEAD, d);
    expect(d.ask.mock.calls[0][1]).toEqual(["account"]);
    expect(d.start.mock.calls[0][0].mode).toBe("window");
    expect(slotsToAsk(DEAD, LOCAL_ORIGIN)).toEqual(["account"]);
    expect(slotsToAsk(DEAD, "devbox")).toEqual(["account", "terminal"]);
  });

  it("★ 本机 + 源会话在终端里 → 照旧开终端（不走后台起）", async () => {
    const d = deps();
    await run(LOCAL_ORIGIN, LIVE, d);
    expect(d.start.mock.calls[0][0].mode).toBe("window");
  });

  it("远端：源会话在终端里 → tmux；不在任何终端里 → window", async () => {
    const a = deps();
    await run("devbox", LIVE, a);
    expect(a.start.mock.calls[0][0].mode).toBe("tmux");
    const b = deps();
    await run("devbox", { ...LIVE, terminal: { kind: "known", value: { host: "none" }, from: "terminal_list" } }, b);
    expect(b.start.mock.calls[0][0].mode).toBe("window");
  });

  it("远端 + 已退出 → 照用户勾的（勾 ⇒ tmux，不勾 ⇒ window）", async () => {
    const yes = deps({ ask: async () => ({ account: null, useTmux: true }) });
    await run("devbox", DEAD, yes);
    expect(yes.start.mock.calls[0][0].mode).toBe("tmux");
    const no = deps({ ask: async () => ({ account: null, useTmux: false }) });
    await run("devbox", DEAD, no);
    expect(no.start.mock.calls[0][0].mode).toBe("window");
  });
});

describe("交出去的那一项", () => {
  it("★ 恒是新 sid、带 fresh_terminal 与源会话 fork_of（不碰原会话）", async () => {
    const d = deps();
    await run("devbox", LIVE, d);
    expect(d.start.mock.calls[0][0].item).toEqual({
      sid: "NEW",
      cwd: "/home/u/p",
      account: { kind: "named", name: "z" },
      fresh_terminal: true,
      fork_of: "SRC",
    });
  });

  it("★ 起失败（那一路已出声、回 false）→ failed，不是 started", async () => {
    const d = deps({ start: async () => false });
    expect(await run("devbox", LIVE, d)).toBe("failed");
  });
});
