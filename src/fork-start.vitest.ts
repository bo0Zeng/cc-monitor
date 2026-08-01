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
    startRemote: vi.fn(async (_a: RemoteArgs) => true),
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

  // G6 起 origin 从 null 改成 "aya"：本机那条路会把 tmux 这格摘掉（见下面那个 describe），
  // 而这条要守的是「问，且只问一次 + 顺序稳定」，所以换到两格都会问的远端上。
  it("★ 源会话已退出（账号/tmux 未知）→ 问，且**只问一次**", async () => {
    const d = deps({ ask: vi.fn(async () => ({ configDir: null, useTmux: false })) });
    await startForkedSession({ newSessionId: "n", origin: "aya", source: DEAD }, asDeps(d));
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

describe("G6：本机那条路不问 tmux", () => {
  /**
   * ★ `startLocal` 根本不看 tmux（`resume_history_session` 交给用户自配的拉起器，
   * tmux 与否不在它的表达能力里）。问一个**答案会被忽略**的问题比不问更坏 ——
   * 用户会以为自己选了。
   */
  it("★ 本机 + 源会话已退出 → 只问账号，不问 tmux", async () => {
    const d = deps({ ask: vi.fn(async () => ({ configDir: null })) });
    await startForkedSession({ newSessionId: "n", origin: null, source: DEAD }, asDeps(d));
    expect(d.ask.mock.calls[0][1]).toEqual(["account"]);
  });

  it("远端仍然要问 tmux（那条路真能表达）", async () => {
    const d = deps({ ask: vi.fn(async () => ({ configDir: null, useTmux: true })) });
    await startForkedSession({ newSessionId: "n", origin: "aya", source: DEAD }, asDeps(d));
    expect(d.ask.mock.calls[0][1]).toEqual(["account", "tmux"]);
  });

  it("★ 本机 + 什么都不缺 → 一次都不问（tmux 那格被摘掉后清单为空）", async () => {
    // LIVE 的 tmux 是 known，本来就不会问；这里守的是「摘 tmux 不会误伤 account」。
    const d = deps();
    await startForkedSession({ newSessionId: "n", origin: null, source: LIVE }, asDeps(d));
    expect(d.ask).not.toHaveBeenCalled();
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
    // Phase G：撞名后缀改成**追加在最后**（`p-fork-cc-2`），与 `pickFreshTmuxName`
    // 写下的同一条规则对齐（「让『第几个』始终是名字的末段」）。原来是 `p-fork2-cc`。
    expect(d.startRemote.mock.calls[0][0].tmuxName).toBe("p-fork-cc-2");
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


describe("Phase G：远端拉起失败不许被读成成功", () => {
  /**
   * ★★ `runRemoteResume*` 失败时**不抛** —— 它自己弹 toast + 回退剪贴板，然后 `return false`。
   * 丢掉那个布尔，编排器就会回 `"started"`，调用点接着弹「✓ 已起来，两条都活着」，
   * 用户同屏看到一条失败 toast 和一条成功 toast。account-ux Phase G 已经栽过一次同形的。
   */
  it("★★ startRemote 回 false → outcome 是 failed，不是 started", async () => {
    const d = deps();
    d.startRemote.mockImplementation(async () => false);
    const r = await startForkedSession(
      { newSessionId: "n", origin: "aya", source: LIVE, sourceTmuxName: "p-cc" },
      asDeps(d),
    );
    expect(r).toBe("failed");
  });

  it("startRemote 回 true → started", async () => {
    const d = deps();
    const r = await startForkedSession(
      { newSessionId: "n", origin: "aya", source: LIVE, sourceTmuxName: "p-cc" },
      asDeps(d),
    );
    expect(r).toBe("started");
  });

  it("failed 与 cancelled 是两回事（取消不该被当成失败去报错）", async () => {
    const d = deps({ ask: async () => null });
    const r = await startForkedSession(
      { newSessionId: "n", origin: "aya", source: DEAD },
      asDeps(d),
    );
    expect(r).toBe("cancelled");
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

  it("★ 全程不碰原会话 —— 传下去的恒是**新** sid", async () => {
    const d = deps();
    await startForkedSession(
      { newSessionId: "NEW", origin: "aya", source: LIVE, sourceTmuxName: "p-cc" },
      asDeps(d),
    );
    expect(d.startRemote.mock.calls[0][0].sessionId).toBe("NEW");
    // Phase G 审计（工程视角）指出这里原本还有一条
    // `expect(Object.keys(d)).toEqual(["ask","startLocal","startRemote"])` —— `d` 就是本文件
    // `deps()` 造的夹具，那是**断言夹具等于它自己，恒真**；而且 `asDeps` 经 `unknown` 强转，
    // 真接口新增字段它也不会红。要守「没有作用于原会话的口子」得从**类型侧**守：
    // 下面这行让 `ForkStartDeps` 一旦多出第四个成员就编译不过（`Exclude` 结果非 never）。
    type Extra = Exclude<keyof ForkStartDeps, "ask" | "startLocal" | "startRemote">;
    const noExtraMembers: Extra extends never ? true : never = true;
    expect(noExtraMembers).toBe(true);
  });
});
