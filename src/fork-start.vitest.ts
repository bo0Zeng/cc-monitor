/**
 * G3b-2：分叉之后起会话的**编排**。
 *
 * 三条最要紧的性质：
 * ① 知道就别问（否则每分叉一次弹一次窗，功能没人用）
 * ② 不知道就**必须**问，且**绝不**自己填一个
 * ③ tmux 名必须与原会话不同 —— 同名 = 新会话 attach 进原窗口 = 毁掉「两条都活着」
 */
import { describe, it, expect, vi } from "vitest";
import { startForkedSession, type ForkStartDeps } from "./fork-start";

/**
 * 造一组注入依赖。返回值刻意**不**收窄成 `ForkStartDeps` —— 测试要读 `.mock.calls`，
 * 收窄之后那些属性在类型上就没了（`vi.fn` 的 `Mock` 信息会被接口签名吃掉）。
 */
type LocalArgs = Parameters<ForkStartDeps["startLocal"]>[0];
type RemoteArgs = Parameters<ForkStartDeps["startRemote"]>[0];
type AskFn = ForkStartDeps["ask"];

function deps(over: { ask?: AskFn } = {}) {
  return {
    ask: vi.fn<AskFn>(over.ask ?? (async () => ({}))),
    startLocal: vi.fn(async (_a: LocalArgs) => {}),
    startRemote: vi.fn(async (_a: RemoteArgs) => {}),
  };
}
const asDeps = (d: ReturnType<typeof deps>): ForkStartDeps => d as unknown as ForkStartDeps;

const LIVE = {
  sourceIsLive: true,
  sourceCwd: "/home/u/p",
  liveConfigDir: "/home/u/.claude-accts/z",
  liveTmuxName: "p-cc",
};
const DEAD = { sourceIsLive: false, sourceCwd: "/home/u/p" };

describe("什么时候问", () => {
  it("★ 信息齐全（源会话活着）→ **一次都不问**，直接起", async () => {
    const d = deps();
    const r = await startForkedSession(
      { newSessionId: "new1", origin: "aya", source: LIVE, sourceTmuxName: "p-cc" },
      asDeps(d),
    );
    expect(r).toBe("started");
    expect(d.ask).not.toHaveBeenCalled();
  });

  it("★ 源会话已退出（账号/tmux 未知）→ 问，且**只问一次**", async () => {
    const d = deps({ ask: vi.fn(async () => ({ configDir: null, useTmux: false })) });
    await startForkedSession({ newSessionId: "n", origin: null, source: DEAD }, asDeps(d));
    expect(d.ask).toHaveBeenCalledTimes(1);
    // 传给弹窗的是「要问哪几格」，顺序稳定
    expect(d.ask.mock.calls[0][1]).toEqual(["account", "tmux"]);
  });

  it("★ 用户取消 → **什么都不起**", async () => {
    const d = deps({ ask: async () => null });
    const r = await startForkedSession({ newSessionId: "n", origin: null, source: DEAD }, asDeps(d));
    expect(r).toBe("cancelled");
    expect(d.startLocal).not.toHaveBeenCalled();
    expect(d.startRemote).not.toHaveBeenCalled();
  });
});

describe("账号：知道就照搬，不知道就用用户答的，**绝不自己填**", () => {
  it("活着 → 照搬源会话的 configDir", async () => {
    const d = deps();
    await startForkedSession({ newSessionId: "n", origin: null, source: LIVE }, asDeps(d));
    expect(d.startLocal.mock.calls[0][0].configDir).toBe("/home/u/.claude-accts/z");
  });

  it("★ 已退出 + 用户选账号 0 → 传 null（= 一个字都不注入）", async () => {
    const d = deps({ ask: vi.fn(async () => ({ configDir: null, useTmux: false })) });
    await startForkedSession({ newSessionId: "n", origin: null, source: DEAD }, asDeps(d));
    expect(d.startLocal.mock.calls[0][0].configDir).toBeNull();
  });

  it("★ 已退出 + 用户选了某账号 → 用用户选的那个", async () => {
    const d = deps({ ask: async () => ({ configDir: "/acct/b", useTmux: false }) });
    await startForkedSession({ newSessionId: "n", origin: null, source: DEAD }, asDeps(d));
    expect(d.startLocal.mock.calls[0][0].configDir).toBe("/acct/b");
  });

  /**
   * ★★ 本文件最要紧的一条：**已退出的会话，即使调用方把 `liveConfigDir` 塞了进来，也不许用它**。
   *
   * 这正是「拿当前账号顶替」的实际形状 —— 调用方（比如从 chip 拿当前账号）很自然会把它填上。
   * `fork-launch.ts` 的 P1 守的是推断层；这条守的是**编排层**。
   *
   * 上面那条「弹窗没给账号」的夹具**漏了这个**（DEAD 里没有 `liveConfigDir`，
   * 于是「顶替」与「落 null」结果相同）—— 变异 S4 报绿把它揪出来的。
   */
  it("★★ 已退出 + 调用方塞了 liveConfigDir + 弹窗没答账号 → 仍然是 null，绝不顶替", async () => {
    const d = deps({ ask: async () => ({ useTmux: false }) });
    await startForkedSession(
      {
        newSessionId: "n",
        origin: null,
        // ← 陷阱：会话已退出，但调用方顺手把「当前账号」传了进来
        source: { sourceIsLive: false, sourceCwd: "/p", liveConfigDir: "/acct/当前" },
      },
      asDeps(d),
    );
    expect(
      d.startLocal.mock.calls[0][0].configDir,
      "已退出的会话账号是 unknown —— 用调用方塞进来的值等于静默换了个身份",
    ).toBeNull();
  });

  it("★ 已退出 + 弹窗没给账号（用户只答了别的）→ 落到账号 0，**不猜一个**", async () => {
    // 这里的关键是「不去拿当前账号顶替」。落账号 0 是**保守**：不注入任何身份。
    const d = deps({ ask: vi.fn(async () => ({ useTmux: false })) });
    await startForkedSession({ newSessionId: "n", origin: null, source: DEAD }, asDeps(d));
    expect(d.startLocal.mock.calls[0][0].configDir).toBeNull();
  });
});

describe("tmux 名", () => {
  it("★ 必须与原会话不同 —— 同名会 attach 进原窗口，毁掉「两条都活着」", async () => {
    const d = deps();
    await startForkedSession(
      { newSessionId: "n", origin: "aya", source: LIVE, sourceTmuxName: "p-cc" },
      asDeps(d),
    );
    const name = d.startRemote.mock.calls[0][0].tmuxName;
    expect(name).not.toBe("p-cc");
    expect(name).toBe("p-fork-cc");
  });

  it("避让已占用的名字", async () => {
    const d = deps();
    await startForkedSession(
      {
        newSessionId: "n",
        origin: "aya",
        source: LIVE,
        sourceTmuxName: "p-cc",
        takenTmuxNames: ["p-fork-cc"],
      },
      asDeps(d),
    );
    expect(d.startRemote.mock.calls[0][0].tmuxName).toBe("p-fork2-cc");
  });

  it("源会话不在 tmux 里 → 新的也不进 tmux（tmuxName 为 null）", async () => {
    const d = deps();
    await startForkedSession(
      { newSessionId: "n", origin: "aya", source: { ...LIVE, liveTmuxName: "" } },
      asDeps(d),
    );
    expect(d.startRemote.mock.calls[0][0].tmuxName).toBeNull();
  });
});

describe("本机 / 远端分流", () => {
  it("origin=null → 走本机；非 null → 走远端，且带上 origin", async () => {
    const a = deps();
    await startForkedSession({ newSessionId: "n", origin: null, source: LIVE }, asDeps(a));
    expect(a.startLocal).toHaveBeenCalledTimes(1);
    expect(a.startRemote).not.toHaveBeenCalled();

    const b = deps();
    await startForkedSession({ newSessionId: "n", origin: "aya", source: LIVE }, asDeps(b));
    expect(b.startRemote.mock.calls[0][0].origin).toBe("aya");
    expect(b.startLocal).not.toHaveBeenCalled();
  });

  it("★ 全程不碰原会话 —— 没有任何 kill / attach / resume 原 sid 的出口", async () => {
    // 结构性断言：deps 里根本没有能作用于原会话的口子，只有 startLocal/startRemote(新 sid)。
    const d = deps();
    await startForkedSession(
      { newSessionId: "NEW", origin: "aya", source: LIVE, sourceTmuxName: "p-cc" },
      asDeps(d),
    );
    expect(d.startRemote.mock.calls[0][0].sessionId).toBe("NEW");
    expect(Object.keys(d)).toEqual(["ask", "startLocal", "startRemote"]);
  });
});
