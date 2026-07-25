// 快捷键 Action 清单的结构性守卫（account-ux U8 新增账号组时补上）。
//
// 这里锁的不是"某个键绑成什么"，而是几条**加 action 时最容易悄悄踩坏**的契约：
// 漏进显示顺序表 → 整组 action 在编辑器里静默消失，用户既看不到也绑不了，而 TS 不会报错。
import { describe, it, expect } from "vitest";
import { ACTIONS, CATEGORY_LABEL, CATEGORY_ORDER, type Category } from "./actions";

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
  it("默认 chord 不冲突（含未上线的：它们同样占用 chord 表）", () => {
    const byChord = new Map<string, string[]>();
    for (const a of ACTIONS) {
      if (!a.default) continue;
      byChord.set(a.default, [...(byChord.get(a.default) ?? []), a.id]);
    }
    const dup = [...byChord.entries()].filter(([, ids]) => ids.length > 1);
    expect(dup, `重复默认键位: ${JSON.stringify(dup)}`).toEqual([]);
  });

  it("未上线（available:false）的 action 不得占用默认 chord", () => {
    for (const a of ACTIONS) {
      if (!a.available) {
        expect(a.default, `${a.id} 未上线却占着 ${a.default}，会把同键的已上线动作打哑`).toBeNull();
      }
    }
  });
});

describe("account-ux U8：账号快捷键", () => {
  const acctActions = ACTIONS.filter((a) => a.category === "Acct");

  it("两条账号 action 都在清单里且已上线", () => {
    expect(acctActions.map((a) => a.id).sort()).toEqual([
      "account.align-active",
      "account.switch-default",
    ]);
    for (const a of acctActions) expect(a.available).toBe(true);
  });

  // 破坏性动作（重启会话、中断当前回合、丢进程内状态）不该有默认单键等着被误触；
  // 键位表也已经很满。用户想要自己去「设置 → 快捷键」绑。
  it("默认都**不绑**键位", () => {
    for (const a of acctActions) {
      expect(a.default, `${a.id} 不该有默认 chord`).toBeNull();
    }
  });

  it("破坏性那条的 label 要把后果写在脸上", () => {
    const align = ACTIONS.find((a) => a.id === "account.align-active")!;
    expect(align.label).toContain("重启");
  });
});
