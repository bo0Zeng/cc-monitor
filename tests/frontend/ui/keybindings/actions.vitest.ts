// 快捷键 Action 清单的结构性守卫（account-ux U8 新增账号组时补上）。
//
// 这里锁的不是"某个键绑成什么"，而是几条**加 action 时最容易悄悄踩坏**的契约：
// 漏进显示顺序表 → 整组 action 在编辑器里静默消失，用户既看不到也绑不了，而 TS 不会报错。
import { describe, it, expect } from "vitest";
import { KeybindingDispatcher } from "../../../../src/frontend/ui/keybindings/registry";
import { ACTIONS, CATEGORY_LABEL, CATEGORY_ORDER, type Category } from "../../../../src/frontend/ui/keybindings/actions";

describe("ACTIONS 清单结构性守卫", () => {
  it("每个 Category 都在 CATEGORY_ORDER 里（漏加 = 整组在编辑器里静默消失）", () => {
    const used = new Set<Category>(ACTIONS.map((a) => a.category));
    for (const cat of used) {
      expect(CATEGORY_ORDER, `Category "${cat}" 不在 CATEGORY_ORDER 里`).toContain(cat);
    }
  });

  it("CATEGORY_ORDER 无重复、且每项都有显示标签", () => {
    expect(new Set(CATEGORY_ORDER).size).toBe(CATEGORY_ORDER.length);
    for (const cat of CATEGORY_ORDER) {
      expect(CATEGORY_LABEL[cat]).toBeTruthy();
    }
  });

  it("action id 全局唯一（id 是 config.json 的持久化 key）", () => {
    const ids = ACTIONS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  // **不能**跳过 available:false —— dispatcher 的 rebuildChordTable 不看 available，照样把它写进
  // chordToAction（"冲突时后定义的赢"），而派发时又 `if (!action?.available) return`：净效果是
  // 一条未上线 action 能把同 chord 的**已上线**快捷键彻底打哑，且按下去什么都不发生。
  it("默认 chord 不冲突（含未上线的：它们同样占用 chord 表）；同一个键分给作用范围不相交的几个不算", () => {
    const dup: string[] = [];
    for (const [i, a] of ACTIONS.entries())
      for (const b of ACTIONS.slice(i + 1))
        if (a.default && a.default === b.default && KeybindingDispatcher.scopesOverlap(a.scope, b.scope, a.default)) dup.push(`${a.default}: ${a.id} · ${b.id}`);
    expect(dup, "重复默认键位").toEqual([]);
    // 正控：同一档（主区）的两条压在同一个键上认得出；主区与栏不相交。
    expect(KeybindingDispatcher.scopesOverlap("main", "main", "Alt+ArrowUp")).toBe(true);
    expect(KeybindingDispatcher.scopesOverlap("main", "bar", "Alt+ArrowUp")).toBe(false);
    expect(KeybindingDispatcher.scopesOverlap("bare", "bar", "F2"), "单键规则含栏").toBe(true);
  });

  it("未上线（available:false）的 action 不得占用默认 chord", () => {
    for (const a of ACTIONS) {
      if (!a.available) {
        expect(a.default, `${a.id} 未上线却占着 ${a.default}，会把同键的已上线动作打哑`).toBeNull();
      }
    }
  });
});

describe("账号没有快捷键", () => {
  // 换这条会话的号经状态栏账号按钮开面板；「新会话默认」只在设置窗账号页 ⇒ 键位表里没有账号那一组。
  it("清单里没有 account.* 的 action", () => {
    expect(ACTIONS.filter((a) => a.id.startsWith("account."))).toEqual([]);
  });
});

describe("〔S4〕预留位不留", () => {
  const reserved = (xs: ReadonlyArray<{ id: string; available: boolean }>): string[] =>
    xs.filter((a) => !a.available).map((a) => a.id);

  it("清单里零条未上线（available:false）的 action（同一谓词的正控）", () => {
    expect(
      reserved([
        { id: "x.on", available: true },
        { id: "x.reserved", available: false },
      ]),
      "正控：谓词认得出一条预留位（否则下面的零命中不可信）",
    ).toEqual(["x.reserved"]);
    expect(reserved(ACTIONS)).toEqual([]);
  });
});

describe("新建会话快捷键（A2-17-8-1）", () => {
  it("Ctrl+N 新建会话、任何时候都按得到；单键 N 仍是在新窗口打开", () => {
    const a = ACTIONS.find((x) => x.id === "app.new-session");
    expect(a?.default).toBe("Ctrl+KeyN");
    expect(a?.scope).toBe("any");
    expect(a?.available).toBe(true);
    expect(ACTIONS.find((x) => x.id === "tab.pop-out")?.default).toBe("KeyN");
  });
});
