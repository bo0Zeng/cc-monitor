/**
 * G6：源会话事实的推断（`deriveForkSource`）。
 *
 * 这里守的是**两个信号别互相顶替**：tmux 清单答「活没活 / 在哪个 tmux」，
 * pidfile 答「哪个账号」。tmux 清单里**根本没有账号信息**，所以「在 tmux 里找到了」
 * 绝不能顺势推出「账号是 0」。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// ── `K-R46`：文件末尾那一组是**行为**判据，要驱动真的 `runForkFlow` → `startLocal`。
//    ⚠ 这几条 mock 是**文件级**的，而本文件其余判据要么是纯函数（`deriveForkSource`）、
//      要么是读源码文本（E78 那组）—— 一条都不经过被 mock 的那几个模块。
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: () => ({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../../../src/frontend/ui/fork-ask", () => ({ askForkLaunch: vi.fn() }));
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({
  // 分叉走「等到了没有」那一形。
  runRemoteResumeAndWait: vi.fn().mockResolvedValue("arrived"),
  runRemoteResumeTmuxAndWait: vi.fn().mockResolvedValue("arrived"),
}));
vi.mock("../../../src/frontend/ui/launch-arrival", () => ({ awaitArrival: vi.fn().mockResolvedValue(true), expectArrival: vi.fn(), arrivedBody: () => "" }));

import { invoke } from "@tauri-apps/api/core";
import { awaitArrival } from "../../../src/frontend/ui/launch-arrival";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { deriveForkSource, runForkFlow } from "../../../src/frontend/ui/fork-flow";
import { askForkLaunch } from "../../../src/frontend/ui/fork-ask";
import type { SessionAccount } from "../../../src/frontend/ui/accounts";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { isChanCall, launchRenderShim, linesReply, localLaunchCalls } from "../../test-support/chan-fake";
import { copyTableTextsIn } from "../../test-support/copy-refs.ts";

type TmuxRow = {
  name: string;
  path: string;
  command: string;
  attached: boolean;
  windows: number;
  sid: string | null;
  agent: boolean;
};
// `agent` 那一格是那台后端判的（Claude 是 `claude` / `node`）；夹具照那条规则手写。
const T = (name: string, sid: string | null, command = "claude"): TmuxRow => ({
  name,
  path: "/p",
  command,
  attached: false,
  windows: 1,
  sid,
  agent: command === "claude" || command === "node",
});
const A = (sessionId: string, configDir: string | null, alive = true): SessionAccount => ({
  pid: 1,
  sessionId,
  cwd: "/p",
  configDir,
  account: configDir ? "z" : null,
  bare: configDir === null,
  alive,
});

describe("deriveForkSource", () => {
  it("两个信号都在 → 活着、账号已知、tmux 名已知", () => {
    const f = deriveForkSource([A("s1", "/acct/z")], [T("p-cc", "s1")], "s1", "/p");
    expect(f.source.sourceIsLive).toBe(true);
    expect(f.source.liveConfigDir).toBe("/acct/z");
    expect(f.source.liveTmuxName).toBe("p-cc");
    expect(f.sourceTmuxName).toBe("p-cc");
  });

  /**
   * ★★ 本文件最要紧的一条。账号功能没启用 / 账号库没建时 `rows` 是空的，
   * 但 tmux 清单照样能证明会话活着。此时账号必须是 **undefined（不知道）**，
   * 落成 `null` 就等于宣称「确认是账号 0」—— 分叉会静默起在账号 0 上。
   */
  it("★★ tmux 里找到了但账号查不到 → 账号是 undefined，**不是 null**", () => {
    const f = deriveForkSource([], [T("p-cc", "s1")], "s1", "/p");
    expect(f.source.sourceIsLive, "tmux 命中即证明它活着").toBe(true);
    expect(
      f.source.liveConfigDir,
      "null 会被 inferForkLaunch 读成「确认是账号 0」——那是编出来的",
    ).toBeUndefined();
  });

  it("账号确实是账号 0（pidfile 说 configDir=null）→ 落 null（这次是真的知道）", () => {
    const f = deriveForkSource([A("s1", null)], [T("p-cc", "s1")], "s1", "/p");
    expect(f.source.liveConfigDir).toBeNull();
  });

  it("★ pidfile 说该会话已死 → 那行不算数（`alive:false` 不能当活的用）", () => {
    const f = deriveForkSource([A("s1", "/acct/z", false)], [], "s1", "/p");
    expect(f.source.sourceIsLive).toBe(false);
    expect(f.source.liveConfigDir).toBeUndefined();
  });

  it("★ tmux 里那条前台不是 claude（idle-tmux）→ 不算活着", () => {
    // 判据来自 `findClaudeTmuxMatches`（INVARIANTS §30），这里不另算一份。
    const f = deriveForkSource([], [T("p-cc", "s1", "zsh")], "s1", "/p");
    expect(f.source.sourceIsLive).toBe(false);
    expect(f.sourceTmuxName).toBeNull();
  });

  it("★ 同目录别的 claude（sid 不同）不许被认成本会话", () => {
    const f = deriveForkSource([], [T("other-cc", "别的-sid")], "s1", "/p");
    expect(f.source.sourceIsLive).toBe(false);
    expect(f.sourceTmuxName).toBeNull();
    // 「它的名字仍要进已占用」那一格随避让搬进后端（`tmux-name-mint` 问那台自己的会话快照）。
  });

  it("两份快照都取不到（远端不可达）→ 全落「不知道」，不落具体值", () => {
    const f = deriveForkSource(null, null, "s1", "/p");
    expect(f.source.sourceIsLive).toBe(false);
    expect(f.source.liveConfigDir).toBeUndefined();
    expect(f.sourceTmuxName).toBeNull();
    expect(f.source.sourceCwd, "cwd 来自 jsonl，与远端可达性无关").toBe("/p");
  });
});

/**
 * E78：**「分叉完怎么起」只能有一份。**
 *
 * 此前 `collectForkSource` → `runForkFlow` → 成功 toast 这三步由两个调用点各写一遍，
 * 连文案都是逐字重复的双写点、无守卫。Phase G 审计点名：`fork-flow.ts` 自称
 * 「唯一生产接线」而真正共享的只有中段 —— **名不副实的抽象比没有抽象更坏**，
 * 它让人以为改一处就够了。
 *
 * 这条守卫按**源码结构**判，不按行为判：行为测试证明不了「没有第二份」。
 */
describe("E78：两个调用点不许各自再拼一遍", () => {
  const read = (p: string) => readFileSync(resolve(__dirname, "../../..", p), "utf8");
  const CALL_SITES = ["src/frontend/ui/tabs.ts", "src/frontend/ui/views/session-viewer.ts"];

  it("★ 成功 toast 的文案只出现在 fork-flow.ts 里", () => {
    const marker = "已从这一轮分叉并起新会话";
    // 那句话进了文案表：「一个文件说的话」= 它的源码 ＋ 它经 copyText 取的表条目。
    const spoken = (f: string): string => [read(f), ...copyTableTextsIn(read(f))].join("\n");
    expect(spoken("src/frontend/ui/fork-flow.ts"), "接线层自己得有它，否则这条守卫在守空气").toContain(marker);
    for (const f of CALL_SITES) {
      expect(spoken(f), `${f} 又自己拼了一遍成功 toast`).not.toContain(marker);
    }
  });

  it("★ 调用点不许自己调 collectForkSource（那是接线层的活）", () => {
    for (const f of CALL_SITES) {
      expect(read(f), `${f} 绕过接线层自己查事实了`).not.toContain("collectForkSource");
    }
  });

  it("★ 反向自检：读文件这条路真的通（否则上面两条恒绿）", () => {
    expect(read("src/frontend/ui/fork-flow.ts").length).toBeGreaterThan(2000);
    for (const f of CALL_SITES) {
      expect(read(f), `${f} 应当仍在调 runForkFlow`).toContain("runForkFlow");
    }
  });
});

/**
 * E79：**本机侧现在有对侧探针了。**
 *
 * 此前 `collectForkSource(null, …)` 硬编码「查不出来」，于是分叉一个**正跑着的**本机会话
 * 也要白弹一次追问小窗 —— 而那个 pidfile 就在本机、monitor 明明够得着。
 *
 * 这一组测的是 `deriveForkSource` 在「只有账号那一半、没有 tmux 那一半」时的行为，
 * 也就是本机那条路喂给它的形状。
 */
describe("E79：本机只有账号那一半时的推断", () => {
  it("★ 账号查到了（进程活着）→ 账号已知，而 tmux 那一格不许被顺势推成 known", () => {
    const f = deriveForkSource([A("s1", "/acct/z")], null, "s1", "/p");
    expect(f.source.liveConfigDir).toBe("/acct/z");
    expect(f.source.sourceIsLive, "pidfile 说它活着").toBe(true);
    // tmux 清单是 null（本机根本不查）⇒ 名字为 null。本机那条路会把 tmux 这一格摘掉，
    // 所以这里只钉「没有凭空多出一个 tmux 名」。
    expect(f.sourceTmuxName).toBeNull();
  });

  it("★★ 平台答不出（Windows：`available:false`）→ 落「不知道」，**不是**「账号 0」", () => {
    // 本机那条路在 available:false 时喂空表 —— 与「查了但这个 sid 不在表里」同形，
    // 结论都是 unknown。**关键是不能变成 `null`（= 确认账号 0）**。
    const f = deriveForkSource([], null, "s1", "/p");
    expect(f.source.sourceIsLive).toBe(false);
    expect(f.source.liveConfigDir, "落 null 就是宣称「确认账号 0」").toBeUndefined();
  });

  it("★ 本机会话确认是账号 0（pidfile 里 configDir 缺席）→ 这次才是真的 null", () => {
    const f = deriveForkSource([A("s1", null)], null, "s1", "/p");
    expect(f.source.sourceIsLive).toBe(true);
    expect(f.source.liveConfigDir).toBeNull();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// `K-R46`：**分叉本机起会话也要把 tmux 名铸出来传下去**（行为，不是文本）
// ═════════════════════════════════════════════════════════════════════════════
//
// 病（09-10 现打）：后端**故意**拒绝自己铸名（`history.rs` 的 `NO_TMUX_NAME`）⇒
// 前端不传 = 会话不进具名 tmux 容器。三条 `resume_history_session` 的调用点里，
// 本条与 `views/history.ts` 那条先前都没传（只有 `tabs.ts` 传了）。
//
// 🔴 **这条路上这一格先前尤其贵**：POSIX 后端当时只有「显式账号 0」那一态渲染得出容器
// （今天是本机后端 `local.rs::render_ccm_with`：具名账号说不出 `--account <名字>` ⇒ §35 降级；
//  「没表态」⇒ 直接拒），而**分叉是全仓唯一说得出 `{ kind: "base" }` 的生产路**
// （`localLaunchAccountSync` 只回 `named` / `undefined`，回不出 `base`）。
// ⇒ 名字没传的时候，这里是本来最有机会建出容器、却建不成的那一条。
//
// 🔴 `K-R53` 09-11 **订正这一段的现在时**：上面那句「只有账号 0 那一态渲染得出容器」
// **今天已经不成立** —— 具名账号带上名字之后也渲染得出来
// （`LaunchAccount::Named::name`；逐格表住 `history.rs::tests::
//  every_local_account_shape_gets_a_named_verdict_from_the_backend_path`）。
// ⇒ 「分叉是唯一说得出 base 的生产路」仍然是真的，**「唯一进得了容器的路」不再是**。
// 下面那条断 `{ kind: "base" }` 的判据**照旧有效**（它断的是这条路说得出账号 0），
// 只是它不再顺带证明「别处都进不去」。
//
// ⚠ **本组买不到什么**：同 `views/history-actions.vitest.ts` 那组 —— 止于
//   「monitor 发出去的载荷里有这个名字」。后端真的建了容器、以及「`↗ 调出终端` 能用」
//   都在射程之外（后者本机走的是 Win32 HWND 缓存，与这个名字无关）。
describe("K-R46：分叉本机起会话的 tmux 名（行为）", () => {
  const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
  const askMock = askForkLaunch as unknown as ReturnType<typeof vi.fn>;
  const SRC = "cafe0000-1111-4222-8333-444455556666";
  const NEW = "deadbeef-2222-4333-8444-555566667777";

  /** 源会话活着、账号确认是「账号 0」（⇒ 三格全 known ⇒ 一次都不用问）。 */
  /** 那台（本机）后端铸名那一问被问到的入参（名字问后端，判据只核问了什么、用了什么）。 */
  const mintAsks: { origin: string; args: { cwd?: string; forkOf?: string } }[] = [];

  /** 源会话活着、账号确认是「账号 0」（⇒ 三格全 known ⇒ 一次都不用问）。`minted`：本机后端铸回的名字；`null` = 问不到。 */
  function serveLocal(minted: string | null): void {
    mintAsks.length = 0;
    invokeMock.mockImplementation(launchRenderShim((cmd: string, args: unknown) => {
      // 本机「会话 ↔ 账号」经通道问本机后端（原先是 E79 那条已退役的本机 Tauri 命令）。
      if (isChanCall(cmd, args, "accounts-sessions")) {
        return Promise.resolve(
          linesReply([
            { pid: 1, sessionId: SRC, cwd: "/p", configDir: null, account: null, bare: true, alive: true },
          ]),
        );
      }
      if (cmd === "tmux_name_mint") {
        const { origin, ...rest } = args as Record<string, unknown>;
        mintAsks.push({ origin: String(origin), args: rest });
        return Promise.resolve(minted ?? undefined);
      }
      return Promise.resolve(undefined);
    }));
  }

  function resumePayload(): Record<string, unknown> {
    const call = localLaunchCalls(invokeMock.mock.calls, "resume_history_session")[0];
    expect(call, "一次 `resume_history_session` 都没发出去 —— 分叉那条本机路没走到").toBeTruthy();
    return call!;
  }

  async function fork(): Promise<string> {
    const outcome = await runForkFlow({
      origin: LOCAL_ORIGIN,
      newSessionId: NEW,
      sourceSessionId: SRC,
      cwd: "/p",
    });
    await new Promise((r) => setTimeout(r, 0));
    return outcome;
  }

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    askMock.mockReset();
    askMock.mockResolvedValue(null);
  });

  it("★★ 载荷里的 `tmuxName` 是本机后端铸回来的那个（问的是新会话的 cwd，不是拼出来的）", async () => {
    // 派生（`<项目名>-cc`）＋ 避让（`-2`）在后端：这里替身写死它铸了 `p-cc-2`。
    // 基名从 **cwd**（`/p`）派生，不再是 `<sid8>-cc`（用户 `R55`：「要是可读的名字 / 不要id」）。
    serveLocal("p-cc-2");
    expect(await fork()).toBe("started");
    // 反空真：三格全 known ⇒ **一次追问小窗都不该弹**（弹了说明事实喂错了，下面在测别的东西）。
    expect(askMock, "不该弹追问小窗 —— 源会话事实三格全 known").not.toHaveBeenCalled();
    expect(mintAsks, "铸名要问本机后端、交新会话的 cwd").toEqual([{ origin: LOCAL_ORIGIN, args: { cwd: "/p" } }]);
    expect(
      resumePayload().tmuxName,
      "分叉本机起的载荷里没有后端铸的 tmux 名 —— 要么名字没传（后端 `NO_TMUX_NAME` 早退\n" +
        "⇒ 会话不进具名容器），要么没问后端 `tmux-name-mint`（全仓唯一铸造口）。",
    ).toBe("p-cc-2");
    // `K-R96`：名字里**一个 sid 片段都没有** —— 新老 sid 都不许出现。
    expect(String(resumePayload().tmuxName).includes(NEW.slice(0, 8))).toBe(false);
    expect(String(resumePayload().tmuxName).includes(SRC.slice(0, 8))).toBe(false);
    // 名字铸的是**新会话自己**的，不是源会话的 —— 换了个 sid 就该换个名字。
    expect(resumePayload().sessionId).toBe(NEW);
    // 这条路说得出「账号 0」，而那是 POSIX 后端唯一渲染得出容器的一态。
    expect(resumePayload().account).toEqual({ kind: "base" });
  });

  /** 主会话 09-28 裁 FIX4 ④：「分叉也走真成功：调用方等到才说『已分叉』」。 */
  it("FIX4 ④：发出去了但没看到分叉出来的会话起来 ⇒ 不是 started、不说「已分叉」", async () => {
    serveLocal("p-cc");
    vi.mocked(awaitArrival).mockResolvedValueOnce(false);
    const toasts = vi.mocked(showActionFailureToast);
    toasts.mockClear();
    expect(await fork()).toBe("failed");
    // 没看到那一句由主窗口的等待方说（这里被替身顶掉）；分叉这一侧一个字都不再说（尤其不说「✓ 已从这一轮分叉并起新会话」）。
    expect(toasts.mock.calls.map((c) => c[0])).toEqual([]);
    expect(vi.mocked(awaitArrival)).toHaveBeenCalledWith(expect.objectContaining({ match: { sid: NEW }, arrived: null }));
  });

  it("★★ 本机后端问不到（**不知道**占了哪些名字）⇒ `tmuxName` 传 `null`，**绝不硬铸**", async () => {
    serveLocal(null);
    expect(await fork()).toBe("started");
    expect(
      resumePayload().tmuxName,
      "不知道本机占了哪些名字时铸了一个 —— 那是把「不知道」当成了「空集」（issue #76 的形状）",
    ).toBeNull();
  });
});
