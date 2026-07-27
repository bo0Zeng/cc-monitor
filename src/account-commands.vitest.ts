// account-ux U8：Ctrl+K 账号命令的构造规则（纯函数测试）。
//
// 这批测试的存在本身就是 D 审计的一条发现：原先这段逻辑长在 main.ts 的闭包里，
// "命令何时出现/消失"一行断言都没有——把可用性判定改成恒 true 也不会红。
import { describe, it, expect, vi } from "vitest";
import { buildAccountCommands, type AccountCommandsInput } from "./account-commands";
import type { Account } from "./accounts";

function acct(p: Partial<Account>): Account {
  return {
    name: "wei",
    email: "wei@x.edu",
    configDir: "/h/.claude-accts/wei",
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    ...p,
  };
}

function input(over: Partial<AccountCommandsInput> = {}): AccountCommandsInput {
  return {
    snapshot: { accounts: [acct({ name: "wei" }), acct({ name: "amy" })], defaultName: "wei" },
    alignableSids: [],
    activeSid: null,
    chordHint: () => undefined,
    setCurrent: vi.fn(),
    alignSession: vi.fn(),
    alignAll: vi.fn(),
    openSettings: vi.fn(),
    ...over,
  };
}
const ids = (i: AccountCommandsInput): string[] => buildAccountCommands(i).map((c) => c.id);

describe("account-ux U8：Ctrl+K 账号命令", () => {
  it("术语用「当前账号」，不再说「切默认为」", () => {
    const cmds = buildAccountCommands(input());
    const t = cmds.find((c) => c.id === "acct-default-amy")!.title;
    expect(t).toContain("当前账号");
    expect(t).not.toContain("切默认为");
  });

  it("keywords 仍保留「默认」做搜索别名（老习惯搜得到）", () => {
    const c = buildAccountCommands(input()).find((x) => x.id === "acct-default-amy")!;
    expect(c.keywords).toContain("默认");
  });

  it("已是当前账号的那条标注「已是当前」，且点它不动手", () => {
    const setCurrent = vi.fn();
    const cmds = buildAccountCommands(input({ setCurrent }));
    const cur = cmds.find((c) => c.id === "acct-default-wei")!;
    expect(cur.title).toContain("已是当前");
    cur.run();
    expect(setCurrent).not.toHaveBeenCalled();
  });

  it("不可选账号不出现在命令里", () => {
    const i = input({
      snapshot: {
        accounts: [acct({ name: "wei" }), acct({ name: "amy", loggedIn: false })],
        defaultName: "wei",
      },
    });
    expect(ids(i)).toContain("acct-default-wei");
    expect(ids(i)).not.toContain("acct-default-amy");
  });

  it("非 ready（未连远端/未启用/老 daemon）→ 只剩「管理…」", () => {
    expect(ids(input({ snapshot: null }))).toEqual(["acct-manage"]);
  });

  // ↓↓ 这两条就是"计划要求但原先不可能做到"的变异锚点
  it("当前会话不在可对齐列表里 → **不出**单会话对齐命令", () => {
    const i = input({ alignableSids: ["other"], activeSid: "s1" });
    expect(ids(i)).not.toContain("acct-align-active");
  });

  it("当前会话确实可对齐 → 出命令，且 run 会去对齐", () => {
    const alignSession = vi.fn();
    const i = input({ alignableSids: ["s1"], activeSid: "s1", alignSession });
    const c = buildAccountCommands(i).find((x) => x.id === "acct-align-active")!;
    expect(c.title).toContain("重启"); // 破坏性后果写在脸上
    c.run();
    expect(alignSession).toHaveBeenCalledTimes(1);
  });

  it("没有不一致会话 → **不出**批量对齐命令", () => {
    expect(ids(input({ alignableSids: [] }))).not.toContain("acct-align-all");
  });

  it("有 k 个不一致 → 批量命令标题带上 k", () => {
    const c = buildAccountCommands(input({ alignableSids: ["a", "b", "c"] })).find(
      (x) => x.id === "acct-align-all",
    )!;
    expect(c.title).toContain("（3）");
  });

  it("activeSid 为 null（无活跃 tab）→ 不出单会话对齐命令，但批量照出", () => {
    const i = input({ alignableSids: ["a"], activeSid: null });
    expect(ids(i)).not.toContain("acct-align-active");
    expect(ids(i)).toContain("acct-align-all");
  });

  it("快捷键提示透传（教学式发现）", () => {
    const i = input({
      alignableSids: ["s1"],
      activeSid: "s1",
      chordHint: (id) => (id === "account.align-active" ? "Ctrl+Shift+A" : "Ctrl+Shift+K"),
    });
    const cmds = buildAccountCommands(i);
    expect(cmds.find((c) => c.id === "acct-align-active")!.hint).toBe("Ctrl+Shift+A");
    expect(cmds.find((c) => c.id === "acct-default-amy")!.hint).toBe("Ctrl+Shift+K");
  });
});
