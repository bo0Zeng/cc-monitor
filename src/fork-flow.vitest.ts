/**
 * G6：源会话事实的推断（`deriveForkSource`）。
 *
 * 这里守的是**两个信号别互相顶替**：tmux 清单答「活没活 / 在哪个 tmux」，
 * pidfile 答「哪个账号」。tmux 清单里**根本没有账号信息**，所以「在 tmux 里找到了」
 * 绝不能顺势推出「账号是 0」。
 */
import { describe, it, expect } from "vitest";
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
