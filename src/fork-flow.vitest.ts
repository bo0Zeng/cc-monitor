/**
 * G6：源会话事实的推断（`deriveForkSource`）。
 *
 * 这里守的是**两个信号别互相顶替**：tmux 清单答「活没活 / 在哪个 tmux」，
 * pidfile 答「哪个账号」。tmux 清单里**根本没有账号信息**，所以「在 tmux 里找到了」
 * 绝不能顺势推出「账号是 0」。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { deriveForkSource } from "./fork-flow";
import type { SessionAccount } from "./accounts";

type TmuxRow = {
  name: string;
  path: string;
  command: string;
  attached: boolean;
  windows: number;
  sid: string | null;
};
const T = (name: string, sid: string | null, command = "claude"): TmuxRow => ({
  name,
  path: "/p",
  command,
  attached: false,
  windows: 1,
  sid,
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
   * ★★ 本文件最要紧的一条。账号功能没启用 / cc-acct-iso 没部署时 `rows` 是空的，
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
    // 但它的名字仍要进「已占用」，否则新分支可能取到同名
    expect(f.takenTmuxNames).toEqual(["other-cc"]);
  });

  it("已占用的 tmux 名 = 整张清单（含非 claude 的），新名要避开它们", () => {
    const f = deriveForkSource([], [T("a", null, "zsh"), T("b", "s1")], "s1", "/p");
    expect(f.takenTmuxNames).toEqual(["a", "b"]);
  });

  it("两份快照都取不到（远端不可达）→ 全落「不知道」，不落具体值", () => {
    const f = deriveForkSource(null, null, "s1", "/p");
    expect(f.source.sourceIsLive).toBe(false);
    expect(f.source.liveConfigDir).toBeUndefined();
    expect(f.takenTmuxNames).toEqual([]);
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
  const read = (p: string) => readFileSync(resolve(__dirname, "..", p), "utf8");
  const CALL_SITES = ["src/tabs.ts", "src/views/session-viewer.ts"];

  it("★ 成功 toast 的文案只出现在 fork-flow.ts 里", () => {
    const marker = "已从这一轮分叉并起新会话";
    expect(read("src/fork-flow.ts"), "接线层自己得有它，否则这条守卫在守空气").toContain(marker);
    for (const f of CALL_SITES) {
      expect(read(f), `${f} 又自己拼了一遍成功 toast`).not.toContain(marker);
    }
  });

  it("★ 调用点不许自己调 collectForkSource（那是接线层的活）", () => {
    for (const f of CALL_SITES) {
      expect(read(f), `${f} 绕过接线层自己查事实了`).not.toContain("collectForkSource");
    }
  });

  it("★ 反向自检：读文件这条路真的通（否则上面两条恒绿）", () => {
    expect(read("src/fork-flow.ts").length).toBeGreaterThan(2000);
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
