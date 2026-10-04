// @vitest-environment node
/**
 * 会话状态的两个轴（`src/frontend/ui/tab-session-state.ts`）的判据。
 *
 * 设计与判据编号。本文件管：
 * - S1 转移表：3 态 × 5 事件 = 15 格，逐格相等。**期望手写自设计表**（`U4.md §1.1`），不从实现生成。
 * 扩成 7 态 × 10 事件 = 70 格（`U4b.md §1.3`，T1）；呈现表 7 态（T2）；T4「说不清不许说成已结束」。
 * 加一行事件 `unseen`（那台机器看不见了）⇒ 7 × 11 = 77 格。
 * 容器开放联合：加一态「活·形式未知」与一行事件 `container-other` ⇒ 8 × 12 = 96 格。
 * - S2 呈现表：3 态 → {类开关 · 状态名 · 提示句}。期望串是本文件里的字面量（抄自那张表），
 *   被测串从文案表 `src/shared/copy/table.json` 取 —— 两份语料。
 * - S4 两轴是唯一读法：tab 层 ＋ 两个快照消费者里，旧的一轴半写法（`tmuxIdle` · `"archived"` 状态字面量 ·
 *   `status === "live"` · `TabStatus`）零处。带同一谓词的正控。
 * - S5 措辞一处定：全 `src/**.ts` 剥注释后「已结束」「可重连」只在登记的豁免里（两向），
 *   「归档」「灰」零处；文案表里说这两个词的条目全在 `sessionState.*` 下。带同一谓词的正控。
 * （S6 探针 ↔ `graylight-suite.sh` 要 `TabManager` 真跑，住 `tabs.vitest.ts`。）
 */
import { describe, expect, it } from "vitest";

import { productionTsFiles } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";
import { loadTable } from "../../copy/copy-support.ts";

import {
  ENDED,
  GONE,
  LIVE,
  LIVE_ATTACHABLE,
  LIVE_RESUMABLE,
  LIVE_UNKNOWN_HOST,
  RECONNECTABLE,
  UNSEEN,
  canResume,
  closesWithoutMenu,
  hasTerminal,
  isLive,
  isResumeOnly,
  nextState,
  stateView,
  type SessionState,
  type StateEvent,
} from "../../../src/frontend/ui/tab-session-state";

/** 八个态（列序 == `U4b.md §1.3` 那张转移表的列序）。 */
const FROM: SessionState[] = [LIVE, LIVE_ATTACHABLE, LIVE_RESUMABLE, LIVE_UNKNOWN_HOST, RECONNECTABLE, ENDED, GONE, UNSEEN];
const NAME = new Map<SessionState, string>([
  [LIVE, "活"],
  [LIVE_ATTACHABLE, "活·可接回"],
  [LIVE_RESUMABLE, "活·只能重开"],
  [LIVE_UNKNOWN_HOST, "活·形式未知"],
  [RECONNECTABLE, "可重连"],
  [ENDED, "已结束"],
  [GONE, "记录没了"],
  [UNSEEN, "说不清"],
]);
const nameOf = (s: SessionState): string => NAME.get(s) ?? `〈不是八个常量之一：${JSON.stringify(s)}〉`;

describe("S1 / T1 转移表（〔U4b〕7 态 × 10 事件 ＋〔GP1〕`unseen` 一行，逐格 == `U4b.md §1.3` ＋ `GP1.md §1`）", () => {
  // 行 = 事件；列 = 此刻（活·没报 · 活·可接回 · 活·只能重开 · 活·形式未知 · 可重连 · 已结束 · 记录没了 · 说不清）。
  // 「=」表示不变（且必须是同一个对象：调用方靠它判「变没变」）。**手写自设计表，不从实现生成。**
  type Row = [string, string, string, string, string, string, string, string];
  const TABLE: Record<StateEvent, Row> = {
    ended: ["已结束", "已结束", "已结束", "已结束", "已结束", "=", "=", "已结束"],
    idle: ["可重连", "可重连", "可重连", "可重连", "=", "=", "=", "可重连"],
    started: ["=", "=", "=", "=", "活", "活", "活", "活"],
    "remote-line": ["=", "=", "=", "=", "活", "活", "活", "活"],
    activity: ["=", "=", "=", "=", "活", "=", "=", "="],
    "container-hosted": ["活·可接回", "=", "活·可接回", "活·可接回", "=", "=", "=", "="],
    "container-none": ["活·只能重开", "活·只能重开", "=", "活·只能重开", "=", "=", "=", "="],
    // 不认识的宿主：活着的都落「形式未知」（不当可接回）；死了的不动。
    "container-other": ["活·形式未知", "活·形式未知", "活·形式未知", "=", "=", "=", "=", "="],
    "record-gone": ["=", "=", "=", "=", "=", "记录没了", "=", "="],
    "record-present": ["=", "=", "=", "=", "=", "=", "已结束", "="],
    "seen-absent": ["=", "=", "=", "=", "=", "=", "=", "已结束"],
    // 那台机器看不见了：还有终端可去的两态（活 · 可重连）⇒ 说不清；死透了的不动（看不见推翻不了它们的死）。
    unseen: ["说不清", "说不清", "说不清", "说不清", "说不清", "=", "=", "="],
  };

  it("★ 96 格两向相等（事件集 == 表的行集；每格结果 == 表）", () => {
    const events = Object.keys(TABLE) as StateEvent[];
    expect([...events].sort()).toEqual([
      "activity",
      "container-hosted",
      "container-none",
      "container-other",
      "ended",
      "idle",
      "record-gone",
      "record-present",
      "remote-line",
      "seen-absent",
      "started",
      "unseen",
    ]);
    const got: Record<string, string[]> = {};
    for (const ev of events) {
      got[ev] = FROM.map((s) => {
        const n = nextState(s, ev);
        return n === s ? "=" : nameOf(n);
      });
    }
    expect(got).toEqual(TABLE);
  });

  it("★ 八个常量的两轴取值 == 设计，且都冻结", () => {
    expect(FROM).toEqual([
      { liveness: "live", recoverability: null },
      { liveness: "live", recoverability: "attachable" },
      { liveness: "live", recoverability: "resumable" },
      { liveness: "live", recoverability: "unknown-host" },
      { liveness: "dead", recoverability: "attachable" },
      { liveness: "dead", recoverability: "resumable" },
      { liveness: "dead", recoverability: "gone" },
      { liveness: "unseen", recoverability: null },
    ]);
    // 冻结：谁也别就地改一个共享常量（改了就是把所有 tab 一起改了）。
    expect(FROM.every((s) => Object.isFrozen(s))).toBe(true);
  });

  it("★ 行为谓词在八个态上 == 设计（`U4b.md §1.3` 谓词；前三列是 U4b 新添的活态，行为必须 == 旧「活」；说不清不给恢复、只在右键菜单里关）", () => {
    const row = (f: (s: SessionState) => boolean): boolean[] => FROM.map(f);
    expect({
      isResumeOnly: row(isResumeOnly),
      canResume: row(canResume),
      closesWithoutMenu: row(closesWithoutMenu),
      hasTerminal: row(hasTerminal),
      isLive: row(isLive),
    }).toEqual({
      //                 活     活·接回 活·重开 活·未知 可重连  已结束 记录没了 说不清
      isResumeOnly: [false, false, false, false, false, true, true, true],
      canResume: [false, false, false, false, false, true, true, false],
      closesWithoutMenu: [false, false, false, false, false, true, true, false],
      hasTerminal: [true, true, true, true, true, false, false, false],
      isLive: [true, true, true, true, false, false, false, false],
    });
  });
});

describe("S2 / T2 呈现表（〔U4b〕7 态 → 类 · 状态名 · 提示句 == `U4b.md §1.3` ＋）", () => {
  it("★ 八个态逐格相等（期望串是本文件的字面量，被测串来自文案表）", () => {
    expect(FROM.map(stateView)).toEqual([
      { ended: false, unseen: false, reconnectable: false, name: null, tooltip: null },
      { ended: false, unseen: false, reconnectable: false, name: null, tooltip: "在 tmux 会话里运行：程序退了也能接回去" },
      { ended: false, unseen: false, reconnectable: false, name: null, tooltip: "不在 tmux 会话里：程序退了只能 resume" },
      { ended: false, unseen: false, reconnectable: false, name: null, tooltip: "终端形式未知" },
      { ended: false, unseen: false, reconnectable: true, name: "可重连", tooltip: "程序退了，终端还在 —— 可以接回去" },
      { ended: true, unseen: false, reconnectable: false, name: "已结束", tooltip: "这个会话已结束" },
      { ended: true, unseen: false, reconnectable: false, name: "记录已不在", tooltip: "这个会话已结束，它的记录也不在了，没法 resume" },
      { ended: false, unseen: true, reconnectable: false, name: "说不清", tooltip: "现在看不见那台机器，说不清这个会话还在不在" },
    ]);
  });

  it("★ 三个类互斥；已结束的外观 == 死了且没有终端可去；说不清单独一个类（不当已结束画）；活着不亮类", () => {
    for (const s of FROM) {
      const v = stateView(s);
      expect([v.ended, v.unseen, v.reconnectable].filter(Boolean).length <= 1, `${nameOf(s)}：两个类同时亮`).toBe(true);
      expect(v.ended, `${nameOf(s)}：.ended 外观 == 已结束 · 记录没了`).toBe(canResume(s));
      expect(v.unseen, `${nameOf(s)}：.unseen == 说不清`).toBe(s.liveness === "unseen");
      if (isLive(s)) expect(v.ended || v.unseen || v.reconnectable, `${nameOf(s)}：活着亮了类`).toBe(false);
    }
  });

  it("★ T4 说不清不许说成已结束：说不清那一格的状态名与提示句里零处「已结束」（正控：已结束那一格有）", () => {
    const u = stateView(UNSEEN);
    expect(`${u.name}${u.tooltip}`).not.toMatch(/已结束/);
    const e = stateView(ENDED);
    expect(`${e.name}${e.tooltip}`).toMatch(/已结束/);
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
    /^src\/frontend\/ui\/tab-[a-z-]+\.ts$/.test(f) ||
    ["src/frontend/ui/tabs.ts", "src/frontend/ui/session-status.ts", "src/frontend/ui/views/grid-monitor.ts"].includes(f);

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
    expect(scope.map((p) => p.file).filter((f) => !f.startsWith("src/frontend/ui/tab-")).sort()).toEqual([
      "src/frontend/ui/session-status.ts",
      "src/frontend/ui/tabs.ts",
      "src/frontend/ui/views/grid-monitor.ts",
    ]);
    expect(scope.map((p) => p.file), "新份自己要在人群里").toContain("src/frontend/ui/tab-session-state.ts");
    const hits = Object.fromEntries(
      scope.map((p) => [p.file, count(p.code, OLD)] as const).filter(([, n]) => n > 0),
    );
    expect(hits, "两轴之后别再按一轴半读：改读 `tab-session-state.ts` 的谓词 / `stateView`").toEqual({});
  });
});

describe("S5 说到会话状态的字只在文案表 `sessionState.*`（零命中 ＋ 登记豁免两向 ＋ 正控）", () => {
  /**
   * 状态名出现在源码字面量里的**豁免**（文件 → 处数）。两向：登记的今天必须真命中这么多处，没登记的零处。
   * 原先登记的两处都进了文案表，今天豁免表为空 ⇒ 这一条等于「源码字面量里零处」（正控在上一条）：
   */
  const EXEMPT: Record<string, number> = {
    // 换号重启回执那一句进了文案表，改说「旧会话已退出」（说的是那一次重启里被杀掉的旧进程，不是 tab 的状态）⇒ 这一条豁免也撤掉。
    // 快捷键名「关闭已结束的 Tab」进了文案表（`sessionState.closeEnded.shortcut`），不再是源码字面量 ⇒ 这一条豁免撤掉。
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

  it("★ 「归档」「灰」：源码字面量里零处（只许说已结束 / 可重连）", () => {
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
