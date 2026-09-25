// @vitest-environment node
/**
 * 〔U4 · `设计/01 §6.2` · `设计/30 §3.5.6`〕会话状态的两个轴（`src/tab-session-state.ts`）的判据。
 *
 * 设计与判据编号见 `调研/第四波记录/U4.md §2`。本文件管：
 * - S1 转移表：3 态 × 5 事件 = 15 格，逐格相等。**期望手写自设计表**（`U4.md §1.1`），不从实现生成。
 * - S2 呈现表：3 态 → {类开关 · 状态名 · 提示句}。期望串是本文件里的字面量（抄自 `设计/30 §3.5.2` 那张表），
 *   被测串从文案表 `src/shared/copy/table.json` 取 —— 两份语料。
 * - S4 两轴是唯一读法：tab 层 ＋ 两个快照消费者里，旧的一轴半写法（`tmuxIdle` · `"archived"` 状态字面量 ·
 *   `status === "live"` · `TabStatus`）零处。带同一谓词的正控。
 * - S5 措辞一处定：全 `src/**.ts` 剥注释后「已结束」「可重连」只在登记的豁免里（两向），
 *   「归档」「灰」零处；文案表里说这两个词的条目全在 `sessionState.*` 下。带同一谓词的正控。
 * （S6 探针 ↔ `graylight-suite.sh` 要 `TabManager` 真跑，住 `tabs.vitest.ts`。）
 */
import { describe, expect, it } from "vitest";

import { productionTsFiles } from "./test-support/production-sources.ts";
import { stripComments } from "./test-support/strip-comments.ts";
import { loadTable } from "./copy/copy-support.ts";

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

/** 生产 `.ts`（剥过注释）。一次读完，下面几条共用。 */
const PROD = productionTsFiles("src").map((f) => ({ file: f.file, code: stripComments(f.text, "ts") }));
const count = (code: string, re: RegExp): number => code.match(re)?.length ?? 0;

describe("S4 两轴是唯一读法（零命中 ＋ 同一谓词的正控）", () => {
  /** 旧的读法：两个轴挤在 `status` ＋ `tmuxIdle` 里时的四种写法。 */
  const OLD = /\btmuxIdle\b|["'](?:archived|tmux-idle)["']|\.status\s*[!=]==?\s*["'](?:live|archived)["']|\bTabStatus\b/g;
  /** 人群：tab 层（`tab-*.ts` ＋ 组装根）＋ 会话快照的两个消费者。 */
  const inScope = (f: string): boolean =>
    /^src\/tab-[a-z-]+\.ts$/.test(f) ||
    ["src/tabs.ts", "src/session-status.ts", "src/views/grid-monitor.ts"].includes(f);

  it("★ 正控：四种旧写法各命中一次（谓词没拼错）", () => {
    const sample = [
      "tab.tmuxIdle = true;",
      'refs.root.classList.toggle("archived", archived);',
      'if (s.status === "live") n += 1;',
      "status: TabStatus,",
    ].join("\n");
    expect(count(sample, OLD)).toBe(4);
    // 剥注释之后才判：注释里说到旧名字不算（这些文件的订正注释逐字引用旧写法）。
    expect(count(stripComments('// tab.tmuxIdle 已退役\nconst x = 1;', "ts"), OLD)).toBe(0);
  });

  it("★ 人群 == 盘上的 tab 层 ＋ 两个消费者（反空真），且每一份里旧写法零处", () => {
    const scope = PROD.filter((p) => inScope(p.file));
    expect(scope.map((p) => p.file).filter((f) => !f.startsWith("src/tab-")).sort()).toEqual([
      "src/session-status.ts",
      "src/tabs.ts",
      "src/views/grid-monitor.ts",
    ]);
    expect(scope.map((p) => p.file), "新份自己要在人群里").toContain("src/tab-session-state.ts");
    const hits = Object.fromEntries(
      scope.map((p) => [p.file, count(p.code, OLD)] as const).filter(([, n]) => n > 0),
    );
    expect(hits, "两轴之后别再按一轴半读：改读 `tab-session-state.ts` 的谓词 / `stateView`").toEqual({});
  });
});

describe("S5 说到会话状态的字只在文案表 `sessionState.*`（零命中 ＋ 登记豁免两向 ＋ 正控）", () => {
  /**
   * 状态名出现在源码字面量里的**豁免**（文件 → 处数）。两向：登记的今天必须真命中这么多处，没登记的零处。
   * 这两处不经 `stateView`，理由各一句：
   */
  const EXEMPT: Record<string, number> = {
    // 换号重启回执里的「旧会话已结束」说的是那一次重启里被杀掉的旧进程，不是某个 tab 此刻的状态。
    "src/account-restart.ts": 1,
    // 快捷键名「关闭已结束的 Tab」：快捷键表是静态数据，不经文案表（`keybindings/actions.ts` 另有自己的判据族）。
    "src/keybindings/actions.ts": 1,
  };
  const NAMES = /已结束|可重连/g;
  const RETIRED = /归档|灰(?![色度阶])/g;

  it("★ 正控：字面量里的词命中、注释里的不算", () => {
    const code = stripComments('const a = "这个会话已结束"; // 可重连\nconst b = `变灰 · 归档`;', "ts");
    expect([count(code, NAMES), count(code, RETIRED)]).toEqual([1, 2]);
  });

  it("★ 「已结束」「可重连」：源码字面量里 == 登记的豁免（两向）", () => {
    const hits = Object.fromEntries(
      PROD.map((p) => [p.file, count(p.code, NAMES)] as const).filter(([, n]) => n > 0),
    );
    expect(hits).toEqual(EXEMPT);
  });

  it("★ 「归档」「灰」：源码字面量里零处（`设计/91 §4` R1：只许说已结束 / 可重连）", () => {
    const hits = Object.fromEntries(
      PROD.map((p) => [p.file, count(p.code, RETIRED)] as const).filter(([, n]) => n > 0),
    );
    expect(hits).toEqual({});
  });

  it("★ 文案表里说到这两个词的条目全在 `sessionState.*` 下，且两个状态名各自恰有一条", () => {
    const table = loadTable();
    const saying = Object.entries(table)
      .filter(([, e]) => count(e.zh, NAMES) > 0)
      .map(([k]) => k);
    expect(saying.filter((k) => !k.startsWith("sessionState."))).toEqual([]);
    expect(
      Object.entries(table)
        .filter(([, e]) => e.zh === "已结束" || e.zh === "可重连")
        .map(([k]) => k)
        .sort(),
    ).toEqual(["sessionState.ended.name", "sessionState.reconnectable.name"]);
  });
});
