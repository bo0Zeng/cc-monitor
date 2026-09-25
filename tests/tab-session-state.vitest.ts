// @vitest-environment node
/**
 * 〔U4 · `设计/01 §6.2` · `设计/30 §3.5.6`〕会话状态的两个轴（`src/tab-session-state.ts`）的判据。
 *
 * 设计与判据编号见 `调研/第四波记录/U4.md §2`。本文件管：
 * - S1 转移表：3 态 × 5 事件 = 15 格，逐格相等。**期望手写自设计表**（`U4.md §1.1`），不从实现生成。
 * - S2 呈现表：3 态 → {类开关 · 状态名 · 提示句}。期望串是本文件里的字面量（抄自 `设计/30 §3.5.2` 那张表），
 *   被测串从文案表 `src/shared/copy/table.json` 取 —— 两份语料。
 */
import { describe, expect, it } from "vitest";

import {
  ENDED,
  LIVE,
  RECONNECTABLE,
  hasTerminal,
  isLive,
  isResumeOnly,
  nextState,
  stateView,
  type SessionState,
  type StateEvent,
} from "../src/tab-session-state";

const NAME = new Map<SessionState, string>([
  [LIVE, "活"],
  [RECONNECTABLE, "可重连"],
  [ENDED, "已结束"],
]);
const nameOf = (s: SessionState): string => NAME.get(s) ?? `〈不是三个常量之一：${JSON.stringify(s)}〉`;

describe("S1 转移表（3 态 × 5 事件，逐格 == `U4.md §1.1`）", () => {
  // 行 = 事件；列 = 此刻（活 · 可重连 · 已结束）。「=」表示不变（且必须是同一个对象：调用方靠它判「变没变」）。
  const TABLE: Record<StateEvent, [string, string, string]> = {
    ended: ["已结束", "已结束", "="],
    idle: ["可重连", "=", "="],
    started: ["=", "活", "活"],
    "remote-line": ["=", "活", "活"],
    activity: ["=", "活", "="],
  };
  const FROM: SessionState[] = [LIVE, RECONNECTABLE, ENDED];

  it("★ 15 格两向相等（事件集 == 表的行集；每格结果 == 表）", () => {
    const events = Object.keys(TABLE) as StateEvent[];
    expect(events.sort()).toEqual(["activity", "ended", "idle", "remote-line", "started"]);
    const got: Record<string, string[]> = {};
    for (const ev of events) {
      got[ev] = FROM.map((s) => {
        const n = nextState(s, ev);
        return n === s ? "=" : nameOf(n);
      });
    }
    expect(got).toEqual(TABLE);
  });

  it("★ 三个常量的两轴取值 == 设计（活 ＋ 没报 · 死 ＋ 容器还在 · 死 ＋ 只能 resume）", () => {
    expect([LIVE, RECONNECTABLE, ENDED]).toEqual([
      { liveness: "live", recoverability: null },
      { liveness: "dead", recoverability: "attachable" },
      { liveness: "dead", recoverability: "resumable" },
    ]);
    // 冻结：谁也别就地改一个共享常量（改了就是把所有 tab 一起改了）。
    expect([LIVE, RECONNECTABLE, ENDED].every((s) => Object.isFrozen(s))).toBe(true);
  });

  it("★ 三个行为谓词在三个态上 == 设计（`U4.md §1.1` 谓词表）", () => {
    const row = (f: (s: SessionState) => boolean): boolean[] => FROM.map(f);
    expect({
      isResumeOnly: row(isResumeOnly),
      hasTerminal: row(hasTerminal),
      isLive: row(isLive),
    }).toEqual({
      //            活     可重连  已结束
      isResumeOnly: [false, false, true],
      hasTerminal: [true, true, false],
      isLive: [true, false, false],
    });
  });
});

describe("S2 呈现表（3 态 → 类 · 状态名 · 提示句 == `设计/30 §3.5.2`）", () => {
  it("★ 三个态逐格相等", () => {
    expect([LIVE, RECONNECTABLE, ENDED].map(stateView)).toEqual([
      { ended: false, reconnectable: false, name: null, tooltip: null },
      { ended: false, reconnectable: true, name: "可重连", tooltip: "程序退了，终端还在 —— 可以接回去" },
      { ended: true, reconnectable: false, name: "已结束", tooltip: "这个会话已结束" },
    ]);
  });

  it("★ 两个类互斥（`30 §3.5.1`：两套灰在 CSS 上互斥）且只在死了的时候出现", () => {
    for (const s of [LIVE, RECONNECTABLE, ENDED]) {
      const v = stateView(s);
      expect(v.ended && v.reconnectable, `${nameOf(s)}：两个类同时亮`).toBe(false);
      expect(v.ended || v.reconnectable, `${nameOf(s)}：类亮不亮 == 死没死`).toBe(s.liveness === "dead");
    }
  });
});
