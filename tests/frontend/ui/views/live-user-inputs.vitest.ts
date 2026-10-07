/**
 * K-R45 · `KR45D2`（乙 · 实时窗口）的大纲 ——**清单改问后端要之后**的判据。
 *
 * # 这一格从前量什么、现在量什么
 *
 * 从前实时窗口在 `onLine` 旁路里一条一条攒清单（`Tab.userInputs`）—— 到达序不是对话序，
 * monitor 起得晚就不全。那本旁路账本删了：清单问后端要
 * （`list_user_inputs` ⇒ `--list-user-inputs`），判定只住后端（Rust 那侧自己的判据量四条口径）。
 * ⇒ 本文件**不再量口径**，量的是：
 * - **顺序 = 后端给的顺序**（文件序），与到达序无关 —— 旁路账本最大的那个病；
 * - **前端不攒**：流上来一条用户输入，清单里有没有它只看后端说没说（两向）；
 * - 什么时候要、要哪一截（批结束 · 真用户输入上屏 · 切进来有新行）· 在途合并 ·
 *   增量要不到 / 被重写 ⇒ 从 0 重要 · 老后端 ⇒ 灰掉说原因、此后不再要 · 关 tab 之后迟到的不回写；
 * - 跳：已建卡的滚过去 · 没建卡的标出来 · 🔴 上翻补批之后再点 ⇒ 撤标记（`不可跳 → 可跳` 的活体）。
 *
 * # 台子（🔴 渲染管线是**真的**，这是本文件与 `tabs.vitest.ts` 的关键差别）
 *
 * 只 mock 掉 IPC 与几个纯外部动作；`list_user_inputs` 由 `session-viewer-rig.ts` 的
 * `outlineBackend` 替身回答 —— 它**不判**，每条清单项由用例逐条写明（异源：不是拿前端算出来的去对前端）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";

/**
 * 骨架索引的替身：`null` = 回 `undefined`（== SE1 那几格的形状：索引要不到）；
 * 否则回一份索引，`rows[k]` 就是 seq k（大纲那几个键由用例逐行写明 —— 异源：不拿前端算的去对前端）。
 */
const indexStub = vi.hoisted(() => ({ rows: null as null | Array<Record<string, unknown>> }));

// --- 只 mock「会真的去碰机器」的那几样；渲染管线保持真身 ---
vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../../../test-support/chan-fake");
  return {
    // 会话读面三问改走通道（`withSessionReads` 译 `chan_call` ⇄ 旧名字 ＋ 旧回包）。
    invoke: vi.fn(withSessionReads(async (cmd: string, raw: Record<string, unknown>) => {
      const args = raw as { fromOffset: number };
      if (cmd === "list_user_inputs") return rig.answerListUserInputs(args);
      if (cmd === "read_session_index" && indexStub.rows && args.fromOffset === 0) {
        const rows = indexStub.rows;
        return { available: true, from: 0, end: rows.length, rows };
      }
      return undefined;
    })),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../../src/frontend/ui/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../../src/frontend/ui/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runLocalResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { invoke } from "@tauri-apps/api/core";
import { sessionReadCalls } from "../../../test-support/chan-fake";
import {
  installViewerRig,
  assistantLine,
  userLine,
  withSession,
  outlineBackend,
  outlineEntry,
  settleOutline,
  type RigPayload,
  type ViewerRigHandles,
} from "../../../test-support/session-viewer-rig";
import { REPO_ROOT } from "../../../test-support/repo-root";
import { TabManager, type Tab } from "../../../../src/frontend/ui/tabs";
import { MAX_TRANSIENT_FAILURES } from "../../../../src/frontend/ui/views/outline-source";

/** 这一趟里发了哪些 `list_user_inputs`（只看参数）。 */
const outlineCalls = (): unknown[] =>
  sessionReadCalls(vi.mocked(invoke).mock.calls, "list_user_inputs");

let rig: ViewerRigHandles;
let tm: TabManager;
let streamRootEl: HTMLElement;

/**
 * `onLine` 要的是生成物类型 `JsonlLinePayload`，而台子的 `message` 刻意是 `unknown`
 * （判据要造畸形记录）。**强转只此一处**，别再散开写（先例：`tabs.vitest.ts` 的 `as never`）。
 */
const feed = (p: RigPayload): void => tm.onLine(p as never);

const peek = (sid: string): Tab =>
  (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;

const overlayOf = (sid = "s1"): HTMLElement =>
  peek(sid).inputsEl;
const rowsOf = (sid = "s1"): HTMLButtonElement[] => [
  ...overlayOf(sid).querySelectorAll<HTMLButtonElement>(".user-input-row"),
];
/** 大纲入口那颗钮（不摆进 DOM —— 入口是查找面板的「大纲」页签；它仍是清单可用与否、几条的那一格状态）。 */
const toggleOf = (sid = "s1"): HTMLButtonElement => peek(sid).inputsPanel.toggle;
/**
 * 🔴 找卡**只在那个 tab 自己的流里找**。`[data-uuid]` 在本仓只有一个意思
 * （渲染出来的消息卡）；清单行用的是 `data-input-uuid`，两者不许混着数。
 */
const cardOf = (uuid: string, sid = "s1"): HTMLElement | null =>
  peek(sid).streamEl.querySelector<HTMLElement>(`[data-uuid="${uuid}"]`);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  rig = installViewerRig();
  const barEl = document.createElement("div");
  streamRootEl = document.createElement("div");
  document.body.append(barEl, streamRootEl);
  tm = new TabManager(barEl, streamRootEl);
});

afterEach(() => {
  vi.unstubAllGlobals();
  indexStub.rows = null;
});

describe("SE1 清单问后端要：顺序是后端给的，前端不攒", () => {
  /**
   * 造「到达序 ≠ 文件序」的办法就是 live 真实的到达序：**重放尾块先到**。
   * 批里先来 seq=100 ⇒ 钉 floor=100 直渲；再来 seq=1 ⇒ 收纳。后端按文件序说 [u1, u100]。
   */
  function replayTailFirst(): void {
    outlineBackend.entries = [outlineEntry("u1", "很久以前那一句"), outlineEntry("u100", "最新那一句")];
    tm.onBatchStart();
    feed(userLine(100, "u100", "最新那一句"));
    feed(userLine(1, "u1", "很久以前那一句"));
    tm.onBatchEnd();
  }

  it("🔴 编号 = 对话顺序（后端的文件序），不是到达序 —— 旁路账本那个病的活体", async () => {
    replayTailFirst();
    await settleOutline();
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u100"]);
    expect(rowsOf().map((r) => r.textContent)).toEqual(["1. 很久以前那一句", "2. 最新那一句"]);
    expect(toggleOf().textContent).toBe("大纲 · 2");
    // 批期不要、批结束 active tab 要**一次**：本机 origin 逐字 `<local>`、从 0 起
    expect(outlineCalls()).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 0 }]);
  });

  it("🔴 前端不判：流上来的用户输入，后端没说就不列；后端说了、流上没来的也列（两向）", async () => {
    outlineBackend.entries = [outlineEntry("only-in-file")];
    feed(userLine(1, "u1", "流上来的一句"));
    await settleOutline();
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["only-in-file"]);
  });

  it("真用户输入上屏 ⇒ 从上次的 end 接着要一截、追加；已有的行一个不碰", async () => {
    // 第一条是「渲染时被剥空、没有卡」的那一形（后端列了它，流上没有它的卡）⇒ 点它必然标上
    outlineBackend.entries = [outlineEntry("gone", "剥空的那条"), outlineEntry("u100", "最新那一句")];
    feed(userLine(100, "u100", "最新那一句"));
    await settleOutline();
    const first = rowsOf()[0];
    first.click();
    await settleOutline();
    expect(first.dataset.unjumpable, "先得真的标上，不然「不碰」是空真").toBe("1");
    vi.mocked(invoke).mockClear();

    outlineBackend.entries.push(outlineEntry("u101", "刚说的"));
    feed(userLine(101, "u101", "刚说的"));
    await settleOutline();

    expect(outlineCalls()).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 2 }]);
    expect(rowsOf().map((r) => r.textContent)).toEqual([
      "1. 剥空的那条",
      "2. 最新那一句",
      "3. 刚说的",
    ]);
    expect(rowsOf()[0], "整表重建过 ⇒ 挂着的标记被抹掉").toBe(first);
    expect(first.dataset.unjumpable).toBe("1");
  });

  it("在途时又来两句 ⇒ 合并成**补一趟**（不是每句一趟）", async () => {
    replayTailFirst();
    await settleOutline();
    vi.mocked(invoke).mockClear();
    outlineBackend.entries.push(outlineEntry("u101"), outlineEntry("u102"), outlineEntry("u103"));
    feed(userLine(101, "u101", "一"));
    feed(userLine(102, "u102", "二"));
    feed(userLine(103, "u103", "三"));
    await settleOutline();
    expect(outlineCalls().length).toBe(2);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u100", "u101", "u102", "u103"]);
  });

  it("增量要不到（文件被截断/重写）⇒ 从 0 重要一份、整表换", async () => {
    replayTailFirst();
    await settleOutline();
    outlineBackend.entries = [outlineEntry("r1")]; // 文件变短：上次的 end=2 越过了 EOF
    vi.mocked(invoke).mockClear();
    feed(userLine(101, "r1", "重写之后"));
    await settleOutline();
    expect(outlineCalls().map((a) => (a as { fromOffset: number }).fromOffset)).toEqual([2, 0]);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["r1"]);
  });

  it("增量里冒出已有的 uuid（被重写但更长）⇒ 从 0 重要一份", async () => {
    replayTailFirst();
    await settleOutline();
    // 文件被重写、而且更长：上次的 end=2 之后那一截里冒出了已经列过的 u100
    outlineBackend.entries = [outlineEntry("x"), outlineEntry("u1"), outlineEntry("u100"), outlineEntry("y")];
    vi.mocked(invoke).mockClear();
    feed(userLine(101, "y", "重写之后"));
    await settleOutline();
    expect(outlineCalls().map((a) => (a as { fromOffset: number }).fromOffset)).toEqual([2, 0]);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["x", "u1", "u100", "y"]);
  });

  it("🔴 结构性（oldBackend）⇒ 灰掉、原因挂提示上；**只要一次**，此后触发也不再要（不白起进程）", async () => {
    outlineBackend.available = false;
    outlineBackend.reason = "这台机器上的后端版本旧";
    replayTailFirst();
    await settleOutline();
    expect(rowsOf().length).toBe(0);
    expect(toggleOf().disabled).toBe(true);
    expect(toggleOf().title).toContain("本机的后端版本旧");
    expect(outlineCalls().length, "结构性失败只该要一次").toBe(1);
    vi.mocked(invoke).mockClear();
    feed(userLine(101, "u101", "又说一句"));
    tm.onBatchStart();
    tm.onBatchEnd();
    await settleOutline();
    expect(outlineCalls()).toEqual([]);
  });

  it("🔴 瞬时（transport）⇒ 灰着但**下一次触发再要**；要到了就恢复、计数清零", async () => {
    outlineBackend.failNext = 1; // ssh 抖一下
    replayTailFirst();
    await settleOutline();
    expect(outlineCalls().length).toBe(1);
    expect(rowsOf().length).toBe(0);
    expect(toggleOf().disabled).toBe(true);
    expect(toggleOf().title).toContain("连不上");
    // 下一次触发（真用户输入上屏）⇒ 再要一次，这次通了
    outlineBackend.entries.push(outlineEntry("u101", "刚说的"));
    feed(userLine(101, "u101", "刚说的"));
    await settleOutline();
    expect(outlineCalls().length, "瞬时失败之后下一次触发没再要").toBe(2);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u100", "u101"]);
    expect(toggleOf().title).toBe("按你的输入跳转");
  });

  it("瞬时失败时手上已有清单 ⇒ 不动它（行与续点都不动），下一次从原续点接着要", async () => {
    replayTailFirst();
    await settleOutline();
    const first = rowsOf()[0];
    vi.mocked(invoke).mockClear();
    outlineBackend.failNext = 2; // 增量那一趟 ＋ 从 0 重要那一趟都失败
    feed(userLine(101, "u101", "刚说的"));
    await settleOutline();
    expect(rowsOf()[0], "瞬时失败把已有清单抹了").toBe(first);
    expect(rowsOf().length).toBe(2);
    expect(toggleOf().disabled).toBe(false);
    vi.mocked(invoke).mockClear();
    outlineBackend.entries.push(outlineEntry("u102"));
    feed(userLine(102, "u102", "又一句"));
    await settleOutline();
    expect(outlineCalls().map((a) => (a as { fromOffset: number }).fromOffset)).toEqual([2]);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u100", "u102"]);
  });

  it("数的是**连续**：中间成功一次就清零（失败累计过上限但不连续 ⇒ 照样再要）", async () => {
    outlineBackend.failNext = MAX_TRANSIENT_FAILURES - 1;
    replayTailFirst(); // 失败
    await settleOutline();
    for (let i = 2; i < MAX_TRANSIENT_FAILURES; i++) {
      feed(userLine(100 + i, `x${i}`, "又一句")); // 失败……
      await settleOutline();
    }
    feed(userLine(150, "ok", "这次通了")); // 成功 ⇒ 清零
    await settleOutline();
    expect(rowsOf().length).toBeGreaterThan(0);
    outlineBackend.failNext = MAX_TRANSIENT_FAILURES - 1;
    for (let i = 0; i < MAX_TRANSIENT_FAILURES - 1; i++) {
      feed(userLine(160 + i, `y${i}`, "又一句"));
      await settleOutline();
    }
    vi.mocked(invoke).mockClear();
    outlineBackend.entries.push(outlineEntry("z"));
    feed(userLine(170, "z", "还在要吗"));
    await settleOutline();
    expect(outlineCalls().length, "累计而非连续地数 ⇒ 这里已经停了").toBeGreaterThan(0);
  });

  it(`🔴 连续瞬时失败到上限（${MAX_TRANSIENT_FAILURES} 次）⇒ 按结构性处理：此后不再要`, async () => {
    outlineBackend.failNext = 99;
    replayTailFirst(); // 第 1 次
    await settleOutline();
    for (let i = 2; i <= MAX_TRANSIENT_FAILURES + 2; i++) {
      feed(userLine(100 + i, `x${i}`, "又一句")); // 每句都是一次触发
      await settleOutline();
    }
    expect(outlineCalls().length, "到了上限还在要 / 没到上限就停了").toBe(MAX_TRANSIENT_FAILURES);
    // 切走再切回（有新行）也不要
    feed(withSession(userLine(1, "w1", "别的会话"), "s2"));
    await settleOutline();
    vi.mocked(invoke).mockClear();
    tm.switchTo("s1");
    await settleOutline();
    expect(outlineCalls()).toEqual([]);
  });

  it("后端说一条都没有 ⇒ 开关禁用（不给一个点了没反应的入口）", async () => {
    feed(assistantLine(1, "a1", "只有回复"));
    await settleOutline();
    expect(outlineCalls().length).toBeGreaterThan(0); // 真的问过了，不是没问
    expect(rowsOf().length).toBe(0);
    expect(toggleOf().disabled).toBe(true);
    expect(toggleOf().textContent).toBe("大纲");
  });
});

describe("SE2 首屏：索引顺带出大纲 ⇒ 同一份文件只读一遍", () => {
  /** 造一份 `[0, n)` 的索引：每行一个 uuid `u<k>`；`inputs` 里的那几行带大纲的键（`x` 摘要 / `ts`）。 */
  function indexWith(n: number, inputs: Record<number, string>): Array<Record<string, unknown>> {
    return Array.from({ length: n }, (_, k) => ({
      o: k,
      n: 1,
      t: "user",
      u: `u${k}`,
      ...(inputs[k] !== undefined ? { x: inputs[k], ts: `t${k}` } : {}),
    }));
  }
  const indexCalls = (): unknown[] =>
    sessionReadCalls(vi.mocked(invoke).mock.calls, "read_session_index");

  function replay(): void {
    tm.onBatchStart();
    feed(userLine(100, "u100", "最新那一句"));
    feed(userLine(1, "u1", "很久以前那一句"));
    tm.onBatchEnd();
  }

  it("🔴 索引里带了大纲 ⇒ `list_user_inputs` 一次都不发（从 0 那趟省掉），面板 == 索引给的", async () => {
    indexStub.rows = indexWith(101, { 1: "很久以前那一句", 100: "最新那一句" });
    // 反证台子：清单那条若被问到，会回一份**不同的**清单 —— 面板若长成它，就是没用上索引
    outlineBackend.entries = [outlineEntry("list-only", "只有清单那条才会给")];
    replay();
    await settleOutline();
    expect(indexCalls(), "索引真的发了（从 0）").toEqual([
      { origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 0 },
    ]);
    expect(outlineCalls()).toEqual([]);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u100"]);
    expect(rowsOf().map((r) => r.textContent)).toEqual(["1. 很久以前那一句", "2. 最新那一句"]);
    expect(toggleOf().textContent).toBe("大纲 · 2");
  });

  it("种上之后，真用户输入上屏 ⇒ 从索引的 end 接着要增量（不是从 0）", async () => {
    indexStub.rows = indexWith(101, { 1: "a", 100: "b" });
    replay();
    await settleOutline();
    expect(outlineCalls()).toEqual([]);
    outlineBackend.entries = Array.from({ length: 102 }, (_, k) => outlineEntry(`z${k}`));
    feed(userLine(101, "u101", "刚说的"));
    await settleOutline();
    expect(outlineCalls()).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 101 }]);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u100", "z101"]);
  });

  it("🔴 索引里一个 `x` 都没有（老后端 / 真的零条，分不清）⇒ 照旧自己从 0 要**恰好一次**", async () => {
    indexStub.rows = indexWith(101, {});
    outlineBackend.entries = [outlineEntry("u1"), outlineEntry("u100")];
    replay();
    await settleOutline();
    expect(indexCalls().length).toBe(1);
    expect(outlineCalls()).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 0 }]);
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u100"]);
  });

  it("索引要不到（回包不对 / 失败）⇒ 同上，从 0 要恰好一次", async () => {
    outlineBackend.entries = [outlineEntry("u1")];
    replay();
    await settleOutline();
    expect(indexCalls().length, "索引那一趟照发").toBe(1);
    expect(outlineCalls()).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 0 }]);
  });
});

describe("KR45D2 跳：后端给的清单，点到哪一条", () => {
  async function tailFirst(): Promise<void> {
    outlineBackend.entries = [outlineEntry("u1", "很久以前那一句"), outlineEntry("u100", "最新那一句")];
    feed(userLine(100, "u100", "最新那一句"));
    feed(userLine(1, "u1", "很久以前那一句"));
    await settleOutline();
  }

  it("点已经建了卡的那一条 ⇒ 滚给了那张卡（跳转走的是与查看器同一份 revealCard）", async () => {
    await tailFirst();
    expect(peek("s1").window.pendingCount).toBe(1); // 先证明 u1 真的被收纳了
    const target = cardOf("u100")!;
    rowsOf()[1].click();
    expect(rig.scrollIntoView).toHaveBeenCalledTimes(1);
    expect(rig.scrollIntoView.mock.instances[0]).toBe(target);
    expect(target.classList.contains("search-hit-flash")).toBe(true);
    expect(rowsOf()[1].dataset.unjumpable).toBeUndefined();
  });

  it("🔴 点还收在尾部窗口里、还没建卡的那一条 ⇒ 从它往下整段建出卡再跳过去（不叫人往上翻）", async () => {
    await tailFirst();
    expect(peek("s1").window.pendingCount, "起点：u1 真的还收着").toBe(1);
    rowsOf()[0].click();
    expect(peek("s1").window.pendingCount).toBe(0);
    expect(rowsOf()[0].dataset.unjumpable).toBeUndefined();
    expect(rig.scrollIntoView.mock.instances[0]).toBe(cardOf("u1"));
  });

  // ★★★ `不可跳 → 可跳` 这条转移的**活体**：把 `delete row.dataset.unjumpable;` 整句删掉在这一格上当场红。
  it("🔴 前端哪里都没有的那一条 ⇒ 标出来；它后来到了再点 ⇒ 跳得过去，标记与提示都跟着撤掉", async () => {
    outlineBackend.entries = [outlineEntry("late", "后来才到的那句"), outlineEntry("u100", "最新那一句")];
    feed(userLine(100, "u100", "最新那一句"));
    await settleOutline();
    const row = rowsOf()[0];
    row.click();
    await settleOutline();
    expect(row.dataset.unjumpable, "先得真的标上，不然下面那半是空真").toBe("1");

    feed(userLine(101, "late", "后来才到的那句"));
    await settleOutline();
    expect(cardOf("late"), "夹具选错了记录：卡没建出来").not.toBeNull();
    rowsOf()[0].click();

    expect(rowsOf()[0], "清单被整表重建过 ⇒ 这一格量的不是同一行").toBe(row);
    expect(row.dataset.unjumpable, "跳得过去了还灰着").toBeUndefined();
    expect(row.title, "跳得过去了还挂着「跳不过去」那句").toBe("后来才到的那句");
  });
});

describe("KR45D2 清单跟着 tab 走", () => {
  it("每个 tab 一份，问的是各自那份会话（可见的只有 active 那一份）", async () => {
    outlineBackend.entries = [outlineEntry("e1")];
    feed(userLine(1, "u1", "一号会话说的"));
    expect(overlayOf("s1").classList.contains("active")).toBe(true);
    // ⚠ 喂 s2 的这一条**会把 active 带走**（`onRealUserInput` → `userActive` 的 auto-follow）
    feed(withSession(userLine(2, "v1", "二号会话说的"), "s2"));
    await settleOutline();
    const paths = new Set(outlineCalls().map((a) => (a as { jsonlPath: string }).jsonlPath));
    expect(paths).toEqual(new Set(["/p/s1.jsonl", "/p/s2.jsonl"]));
    expect(overlayOf("s1").classList.contains("active")).toBe(false);
    expect(overlayOf("s2").classList.contains("active")).toBe(true);
    tm.switchTo("s1");
    expect(overlayOf("s1").classList.contains("active")).toBe(true);
    expect(overlayOf("s2").classList.contains("active")).toBe(false);
  });

  it("切进来的后台 tab：有新行才要，没新行不要（不为切 tab 白起进程）", async () => {
    outlineBackend.entries = [outlineEntry("e1")];
    feed(userLine(1, "u1", "一号"));
    feed(withSession(userLine(2, "v1", "二号"), "s2")); // active 被带到 s2
    await settleOutline();
    tm.switchTo("s1");
    await settleOutline();
    vi.mocked(invoke).mockClear();
    tm.switchTo("s2");
    tm.switchTo("s1");
    await settleOutline();
    expect(outlineCalls(), "两边都没有新行 ⇒ 一趟都不该要").toEqual([]);
    tm.switchTo("s2");
    feed(assistantLine(3, "a3", "s1 在后台又长了一行")); // assistant 行：不带走 active、不触发真用户输入
    await settleOutline();
    vi.mocked(invoke).mockClear();
    tm.switchTo("s1");
    await settleOutline();
    expect(outlineCalls().map((a) => (a as { jsonlPath: string }).jsonlPath)).toContain("/p/s1.jsonl");
  });

  it("关掉 tab ⇒ 清单清空、查找面板摘掉；在途那趟回来也不许回写", async () => {
    outlineBackend.entries = [outlineEntry("u1")];
    feed(userLine(1, "u1", "第一句"));
    const overlay = overlayOf("s1");
    expect(streamRootEl.contains(overlay)).toBe(true);
    tm.archiveTab("s1"); // 只有 archived 的 tab 关得掉
    tm.closeTab("s1"); // 此刻 list_user_inputs 还在途
    await settleOutline();
    expect(streamRootEl.querySelectorAll(".session-find").length).toBe(0);
    expect(overlay.querySelectorAll(".user-input-row").length, "关掉之后迟到的清单回写了").toBe(0);
  });
});

/**
 * 与上面那几格**是一对，各买各的**：上面量的是「JS 这边把 `.active` 翻对了没有」，
 * 这一格量的是「CSS 那边真有宿主」—— jsdom **不加载** `styles.css`，
 * 把 `.session-find` 那几条规则整段删掉，上面**一格都不会红**，
 * 而真实后果是每个 tab 的清单**一起挂在屏幕上**（`.active` 翻得再对也没用）。
 * 同形先例：`session-viewer-user-inputs.vitest.ts` 里三条共用规则那一格。
 */
describe("KR45D2 · SE2 实时那块查找面板（大纲在里面）在 styles.css 里真有宿主", () => {
  it("两条规则都在，而且靠 visibility 收起（不是 display —— 切 tab 要 0 reflow）", () => {
    const cssLines = readFileSync(`${REPO_ROOT}/src/frontend/ui/styles.css`, "utf8")
      .split("\n")
      .map((l) => l.trim());
    // 抽取器自检：先确认这把尺子够得着那个文件（不然下面几条是空真）。
    expect(cssLines.length, "读到的 styles.css 只有几行 —— 尺子坏了").toBeGreaterThan(1000);
    expect(cssLines, "读到的不是 styles.css —— 连基准那条规则都没有").toContain(".stream {");

    expect(cssLines, "面板没有 CSS 宿主 ⇒ 每个 tab 的面板一起挂在屏幕上").toContain(".session-find {");
    expect(cssLines, "没有 .active 那条 ⇒ 切过去的那个 tab 的面板也显示不出来").toContain(
      ".session-find.active {",
    );
    // 旧的独立悬浮层整块删了：规则零命中（正控 = 上面那条 `.session-find {` 命中）
    expect(cssLines.filter((l) => l.startsWith(".live-user-inputs"))).toEqual([]);

    const open = cssLines.indexOf(".session-find {");
    const body = cssLines.slice(open + 1, cssLines.indexOf("}", open));
    expect(body, "默认不 hidden ⇒ 非 active 的 tab 的清单照样挂在屏幕上").toContain(
      "visibility: hidden;",
    );
    // 🔴 与 `.stream` 同一条纪律：切 active 不许走 display（整棵子树重建 layout tree）。
    expect(
      body.filter((l) => /^display\s*:/.test(l)),
      "`.session-find` 里出现了 display ⇒ 切 tab 从 0 reflow 退回整棵子树重建",
    ).toEqual([]);
  });
});
