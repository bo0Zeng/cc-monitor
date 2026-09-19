/**
 * 秤 6 ——「内存」（`设计/17 §6` 表第 6 行）。
 *
 * 设计逐字要三样：
 *   ①「`buildResultBody` 闭包持有的文本总字节」——「`cards/index.ts:570` 累加 `text.length`」
 *   ②「`branchFolder.records` / `tab.userInputs` / `TailWindow.pending` 三个账本大小」
 *      ——「三个账本进 `debugSnapshot`」
 *   ③「验 `§2.8`、`§5.5`，**并核对 `tabs.ts` 那句「文本前端零处留存」**」
 *
 * 第 ③ 件才是这杆秤真正值钱的地方：那是一句**声称**，本文件要给出它成不成立的**读数**。
 * 判词与全部读数在 `tests/evidence/S6-memory-ledger.md`，**结论是「不成立」**，
 * 下面「留存活体」那两格就是它的活体。
 *
 * # 🔴 它量不到什么（这一段不完整本身就是缺陷）
 *
 * 1. **这不是进程 RSS。** 它是「**我们自己的账本/闭包里经手了多少**」。
 *    DOM 节点本身、marked/katex 的中间产物、V8 的字符串去重与 rope 表示、
 *    GC 的实际时机 —— 一个都不在里面。
 * 2. **`resultTextLedger` 是累计流量，不是瞬时驻留。** 同一条 result 重渲一次就再记一次，
 *    而旧闭包此刻已可回收 ⇒ 它是**驻留上界**。要瞬时值得 heap snapshot（`§5.5` 的另一半，
 *    本轮**没做** —— jsdom 里没有 retainer 图）。
 * 3. **单位是 UTF-16 码元不是字节。** V8 里非 Latin-1 字符串每码元 2 字节 ⇒
 *    `§2.8` 那句「7 MB ⇒ UTF-16 下 ~14 MB」就是这么换的。下面读数两个单位都给。
 * 4. **三个账本量的是条数，不是字节。** 一条 `BranchRecord` 是三个短字符串、
 *    一条 `UserInputEntry` 是截断到 80 字的摘要、而 `TailWindow.pending` 一条是
 *    **整条 payload**（这一份才是真的文本驻留）。⇒ **三个数不可加**，加起来没有意义。
 * 5. **语料是 69 条的定长夹具，不是真机会话。** 会话越长 `produced` 越大，
 *    本文件给的是「这份语料上的读数 + 换算方法」，不是「一次会话的总量」。
 *
 * # 语料
 *
 * `tests/__fixtures__/scale2-height-records.jsonl` —— **结构采自真机、正文全部合成**
 * （`设计/17 §6` 数据源纪律 2026-09-18 改判，用户逐字「这是测试啊 / 不应该进」）。
 * 本文件**不读** `~/.claude/projects`，也不新造含真实会话正文的夹具。
 *
 * 复算：`npx vitest run tests/scale6-memory-ledger.vitest.ts`
 * 死值验：`bash tests/evidence/S6-mutations.sh`
 * 读数：  `tests/evidence/S6-memory-ledger.md`
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// --- 只 mock「会真的去碰机器」的那几样；渲染管线保持真身（同 live-user-inputs.vitest.ts）---
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../src/tasks-panel", () => ({
  fetchSessionTasks: vi.fn().mockResolvedValue([]),
}));
vi.mock("../src/turn-notify", () => ({
  turnEndNotifier: { observe: vi.fn() },
}));
vi.mock("../src/behavior", () => ({
  getBehavior: vi
    .fn()
    .mockResolvedValue({ resumeCommandLocal: "", resumeCommandRemote: "cct" }),
}));
vi.mock("../src/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runLocalResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../src/account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000,
}));
vi.mock("../src/fork-flow", () => ({
  runForkFlow: vi.fn().mockResolvedValue(undefined),
}));

import {
  renderMessage,
  buildToolGroup,
  addToToolGroup,
  resultTextLedger,
  resetResultTextLedger,
  type JsonlRecord,
  type RenderContext,
} from "../src/cards/index";
import { buildCorpus } from "./scale2-height-corpus";
import {
  installViewerRig,
  userLine,
  assistantLine,
  type RigPayload,
} from "./test-support/session-viewer-rig";
import { TabManager, type Tab } from "../src/tabs";

const FIXTURE = resolve(__dirname, "__fixtures__/scale2-height-records.jsonl");

function freshCtx(): RenderContext {
  return {
    parentPath: "/tmp/scale6/session.jsonl",
    origin: null,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  };
}

/** 渲染一条记录并把产物挂进 `document.body`（tool-group 那一支照生产包外壳）。 */
function renderInto(rec: JsonlRecord, ctx: RenderContext): HTMLElement | null {
  const res = renderMessage(rec, ctx);
  if (res.kind === "skip") return null;
  if (res.kind === "card") {
    document.body.appendChild(res.element);
    return res.element;
  }
  const group = buildToolGroup(res.timestamp);
  addToToolGroup(group, res.units);
  document.body.appendChild(group.root);
  return group.root;
}

function toolUseRec(id: string, name: string, uuid: string): JsonlRecord {
  return {
    type: "assistant",
    uuid,
    parentUuid: null,
    timestamp: "2026-09-18T12:00:00.000Z",
    message: {
      role: "assistant",
      content: [{ type: "tool_use", id, name, input: { command: "echo hi" } }],
    },
  } as unknown as JsonlRecord;
}

function toolResultRec(
  toolUseId: string,
  text: string,
  uuid: string,
): JsonlRecord {
  return {
    type: "user",
    uuid,
    parentUuid: null,
    timestamp: "2026-09-18T12:00:01.000Z",
    message: {
      role: "user",
      content: [{ type: "tool_result", tool_use_id: toolUseId, content: text }],
    },
  } as unknown as JsonlRecord;
}

/**
 * 一段「首行短、深处带记号」的合成正文。
 * 记号刻意埋在**末尾**：`firstLinePreview(text, 60)` 会把首行放进 summary，
 * 若记号在首行，「渲染前 DOM 里找不到它」那半就是假的。
 */
const DEEP_MARK = "DEEP-MARK-6f3a91";
const LONG_TEXT = `第一行很短。\n${"填充正文一二三四五六七八九十。".repeat(600)}\n${DEEP_MARK}`;

/** UTF-16 码元 → UTF-8 字节（只给报告换算用，不参与判据）。 */
function utf8Bytes(s: string): number {
  return new TextEncoder().encode(s).length;
}

// ─────────────────────────────────────────────────────────────────────────────
// 甲：`buildResultBody` 闭包持有的文本总量 —— `设计/17 §5.5` 点名的那个累加
// ─────────────────────────────────────────────────────────────────────────────

describe("秤 6 甲：计数器本身先得是准的（不然下面所有读数都是装饰）", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    resetResultTextLedger();
  });

  it("一条 tool_result ⇒ produced/captured 各涨 1，字数**相等**于那条文本的 length", () => {
    const ctx = freshCtx();
    renderInto(toolUseRec("t1", "Bash", "a1"), ctx);
    renderInto(toolResultRec("t1", LONG_TEXT, "u1"), ctx);

    // 相等断言，不是下界 —— 常数写错/漏加一处都在这里当场红
    expect(resultTextLedger.produced).toBe(1);
    expect(resultTextLedger.producedUnits).toBe(LONG_TEXT.length);
    expect(resultTextLedger.captured).toBe(1);
    expect(resultTextLedger.capturedUnits).toBe(LONG_TEXT.length);
    expect(resultTextLedger.maxUnits).toBe(LONG_TEXT.length);
  });

  it("maxUnits 记的是**单条最长**，不是总和（两条一长一短，顺序反过来也一样）", () => {
    const ctx = freshCtx();
    renderInto(toolUseRec("t1", "Bash", "a1"), ctx);
    renderInto(toolUseRec("t2", "Bash", "a2"), ctx);
    renderInto(toolResultRec("t1", LONG_TEXT, "u1"), ctx);
    renderInto(toolResultRec("t2", "短", "u2"), ctx);

    expect(resultTextLedger.produced).toBe(2);
    expect(resultTextLedger.producedUnits).toBe(LONG_TEXT.length + 1);
    expect(resultTextLedger.maxUnits).toBe(LONG_TEXT.length);
  });

  it("`produced` 与 `captured` 在 fallback 那一支**会岔开** —— 岔开本身就是读数", () => {
    const ctx = freshCtx();
    // tool_use 没来过 ⇒ 走 makeCollapsible 的 fallback：`text` 被工厂闭包捕获，
    // 但 `buildResultBody` 要等展开才跑 ⇒ 此刻 captured 还是 0。
    renderInto(toolResultRec("never-came", LONG_TEXT, "u1"), ctx);

    expect(resultTextLedger.produced).toBe(1);
    expect(resultTextLedger.producedUnits).toBe(LONG_TEXT.length);
    expect(resultTextLedger.captured, "fallback 未展开 ⇒ 闭包侧还没记").toBe(0);

    // 展开一次 ⇒ 工厂跑 ⇒ 闭包侧才追上
    const details = document.body.querySelector(
      "details.block-tool-result",
    ) as HTMLDetailsElement;
    details.open = true;
    details.dispatchEvent(new Event("toggle"));
    expect(resultTextLedger.captured).toBe(1);
    expect(resultTextLedger.capturedUnits).toBe(LONG_TEXT.length);
  });
});

describe("秤 6 甲：69 条语料上的读数（`设计/17 §2.8` 那 7 MB 的现打版）", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    resetResultTextLedger();
  });

  /** 独立数一遍夹具里的 tool_result 块 —— 与计数器**两条路算同一个数**。 */
  function countToolResultBlocks(jsonl: string): number {
    let n = 0;
    for (const raw of jsonl.split("\n")) {
      if (!raw.trim()) continue;
      const rec = JSON.parse(raw) as { message?: { content?: unknown } };
      const content = rec.message?.content;
      if (!Array.isArray(content)) continue;
      for (const b of content) {
        if (
          b &&
          typeof b === "object" &&
          (b as { type?: unknown }).type === "tool_result"
        )
          n++;
      }
    }
    return n;
  }

  /**
   * 🔴 **这份语料上 12 条 tool_result 的 `tool_use` 一条都不在**（现打：语料里
   * 10 个 `tool_use.id`、12 个 `tool_result.tool_use_id`，**交集 0**）——
   * 因为夹具是按字节分位**挑出来的单条**，不是一段连续会话。
   * ⇒ 语料这一侧量到的全是 **fallback** 那条路，内联注入那条路（生产主路）
   *   由上面「计数器本身」那组的定向用例量。**这句必须写进报告，别让读数看起来更全。**
   */
  it("整份语料跑完 ⇒ 计数器数到的条数 == 夹具里独立数出来的 tool_result 块数", () => {
    const jsonl = readFileSync(FIXTURE, "utf8");
    const expected = countToolResultBlocks(jsonl);
    expect(
      expected,
      "夹具里一条 tool_result 都没有 ⇒ 下面全是空真",
    ).toBeGreaterThan(0);

    const items = buildCorpus(jsonl);

    expect(resultTextLedger.produced).toBe(expected);
    const unitsProduced = resultTextLedger.producedUnits;
    expect(unitsProduced).toBeGreaterThan(0);
    // 全是 fallback 且都没展开 ⇒ 闭包侧此刻是 0（见上面的头注）
    expect(
      resultTextLedger.captured,
      "语料里若出现了配得上的 tool_use，这一格该改口径",
    ).toBe(0);

    // 展开全部 fallback ⇒ 闭包侧追上，两侧**逐字节相等**
    let opened = 0;
    for (const it of items) {
      for (const d of it.element.querySelectorAll<HTMLDetailsElement>(
        "details.block-tool-result",
      )) {
        d.open = true;
        d.dispatchEvent(new Event("toggle"));
        opened++;
      }
    }
    expect(opened).toBe(expected);
    expect(resultTextLedger.captured).toBe(expected);
    expect(resultTextLedger.capturedUnits).toBe(unitsProduced);

    // 读数（进 `tests/evidence/S6-memory-ledger.md`）
    console.log(
      [
        "[S6-甲] 语料=tests/__fixtures__/scale2-height-records.jsonl（69 条，结构真/正文合成）",
        `[S6-甲] tool_result 块 ${expected} 条；配得上的 tool_use 0 个 ⇒ 全走 fallback`,
        `[S6-甲] 闭包持有文本 ${unitsProduced} UTF-16 码元 ≈ ${((unitsProduced * 2) / 1024).toFixed(1)} KiB（V8 双字节口径）`,
        `[S6-甲] 单条最长 ${resultTextLedger.maxUnits} 码元；均值 ${Math.round(unitsProduced / expected)} 码元`,
      ].join("\n"),
    );
  });

  it("语料全量的 UTF-8 字节数写进读数（`§2.8` 的 3192 B 均值是按字节算的，别混单位）", () => {
    const jsonl = readFileSync(FIXTURE, "utf8");
    buildCorpus(jsonl);
    expect(resultTextLedger.producedUnits).toBeGreaterThan(0);
    console.log(
      `[S6-甲] 整份夹具 ${jsonl.split("\n").filter((l) => l.trim()).length} 条 / ${utf8Bytes(jsonl)} UTF-8 字节`,
    );
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 乙：核对「文本前端零处留存」—— 这是设计点名要的那个判词
// ─────────────────────────────────────────────────────────────────────────────

describe("🔴 秤 6 乙：核对 `tabs.ts` 那句「一条记录的文本在前端零处留存」", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    resetResultTextLedger();
  });

  /**
   * 留存活体 ①：**DOM 里没有它，展开一下它又出来了** ⇒ 这期间它只能在 JS 闭包里。
   *
   * 这一格是整杆秤的判词所在。设计 `§2.8` 逐字：「这条要 heap snapshot 才能定论」——
   * 本格给的是**另一条不需要 heap snapshot 的定论路**：
   * 文本**曾经从 DOM 上完全消失**，之后**没有任何人再喂过它**，它却能再次出现。
   * ⇒ 中间那段时间它被前端某处留着。留在哪：`buildResultBody` 的闭包。
   */
  it("展开前整个 DOM 里找不到正文；不喂第二遍、只展开一下 ⇒ 正文一字不差地回来了", () => {
    const ctx = freshCtx();
    renderInto(toolUseRec("t1", "Bash", "a1"), ctx);
    renderInto(toolResultRec("t1", LONG_TEXT, "u1"), ctx);

    // 记录本体在这里就**彻底没人引用**了（上面两个 rec 都是临时对象，ctx 里不存 content）
    expect(ctx.pendingToolResults.size, "内联注入那条路不该进 pending 表").toBe(
      0,
    );

    const inline = document.body.querySelector(
      "details.block-tool-result-inline",
    ) as HTMLDetailsElement;
    expect(
      inline,
      "没注进 tool_use 折叠条 ⇒ 这一格量的不是那条路",
    ).not.toBeNull();

    // ① 展开前：整个 DOM 里搜不到深处那个记号
    expect(document.body.textContent ?? "").not.toContain(DEEP_MARK);
    expect(
      inline.querySelector(".block-body-result"),
      "懒建：此刻不该有 body",
    ).toBeNull();

    // ② 只做一件事：展开。没有任何人重新喂过文本。
    inline.open = true;
    inline.dispatchEvent(new Event("toggle"));

    // ③ 正文一字不差地回来了 ⇒ 它一直被留着
    const pre = inline.querySelector(".block-body-result") as HTMLElement;
    expect(pre).not.toBeNull();
    expect(pre.textContent).toBe(LONG_TEXT);
    expect(document.body.textContent ?? "").toContain(DEEP_MARK);
  });

  /**
   * 留存活体 ②：**第二处住址** —— `RenderContext.pendingToolResults`。
   *
   * tool_result 先于 tool_use 到达时，整条 `block`（含 `content` 原文）被存进这张表，
   * 等批末 `reconcilePendingToolResults` 重配。配不上的（tool_use 永远不会来：
   * 跨 session 引用 / 上游截断）**就一直留在表里**。
   * ⇒ 这不是闭包，是一张明摆着的 Map，与「零处留存」同样冲突。
   */
  it("第二处：tool_use 没来过 ⇒ 整条 block（含正文）留在 `ctx.pendingToolResults` 里", () => {
    const ctx = freshCtx();
    renderInto(toolResultRec("never-came", LONG_TEXT, "u1"), ctx);

    expect(ctx.pendingToolResults.size).toBe(1);
    const kept = ctx.pendingToolResults.get("never-came");
    expect(kept, "表里没有它 ⇒ 这一格量错了住址").toBeTruthy();
    // 相等断言：留的是**原文**，不是摘要
    expect((kept!.block as { content?: unknown }).content).toBe(LONG_TEXT);
  });

  /**
   * 对照组 —— **这一格是为了不把判词说过头**。
   *
   * 同一份文本走 `user` 纯文本那条路（不是 tool_result）：卡建完之后，
   * 正文**只在 DOM 里**，`ctx` 的四张表一个都没存它。
   * ⇒「零处留存」那句话**对普通消息卡是对的**，破的是 tool_result 那一支。
   * 少了这一格，判词就成了「前端到处留存」——那是另一种假话。
   */
  it("对照组：普通 user 文本卡渲完，`ctx` 四张表里一个字都没留", () => {
    const ctx = freshCtx();
    renderInto(
      {
        type: "user",
        uuid: "u1",
        parentUuid: null,
        timestamp: "2026-09-18T12:00:00.000Z",
        message: { role: "user", content: LONG_TEXT },
      } as unknown as JsonlRecord,
      ctx,
    );

    expect(ctx.pendingToolResults.size).toBe(0);
    expect(ctx.toolUseNames.size).toBe(0);
    expect(ctx.toolUseElements.size).toBe(0);
    expect(resultTextLedger.produced, "它根本不该经过 tool_result 那条路").toBe(
      0,
    );
    // 正文此刻**只**在 DOM 上（这一句同时说明：本秤不把 DOM 算进「留存」）
    expect(document.body.textContent ?? "").toContain(DEEP_MARK);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 丙：三个账本进 `debugSnapshot`
// ─────────────────────────────────────────────────────────────────────────────

describe("秤 6 丙：三个账本的大小进 `debugSnapshot`", () => {
  let tm: TabManager;

  const feed = (p: RigPayload): void => tm.onLine(p as never);
  const peek = (sid: string): Tab =>
    (tm as unknown as { tabs: Map<string, Tab> }).tabs.get(sid)!;
  /** 直读 `BranchFolder` 的私有账本 —— 判据侧的**第二条路**，用来跟快照对数。 */
  const realBranchRecords = (sid: string): number =>
    (peek(sid).branchFolder as unknown as { records: unknown[] }).records
      .length;
  const snap = (): Record<string, unknown> =>
    JSON.parse(tm.debugSnapshot()) as Record<string, unknown>;

  beforeEach(() => {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    tm = new TabManager(barEl, streamRootEl);
  });
  afterEach(() => vi.unstubAllGlobals());

  /**
   * 造一个**三个账本同时非空、而且三个数互不相同**的局面。
   *
   * 数互不相同是刻意的：若三个都是 2，把 `branchRecords` 错接成 `userInputs`
   * 这类「接错线」的错**照样会绿**。三个数分开 ⇒ 接错线当场红。
   *
   * - `branchRecords = 4`：4 条带 uuid 的记录（收纳的那两条也喂 —— `routeMetaAndBranch`
   *   是两条路径的单一来源）
   * - `userInputs   = 3`：3 条主线 user 文本（assistant 那条按口径排掉）
   * - `pending      = 2`：尾块先到钉住 floor=100 ⇒ seq 1/2 进收纳账本
   */
  function threeLedgersNonEmpty(): void {
    feed(userLine(100, "u100", "最新那一句")); // 渲染，钉 floor=100
    feed(assistantLine(101, "a101", "回复一句")); // 渲染；不进清单
    feed(userLine(1, "u1", "很久以前那一句")); // seq<floor ⇒ 收纳
    feed(userLine(2, "u2", "也是很久以前")); // seq<floor ⇒ 收纳
  }

  it("🔴 反空真：三个账本**同时**非空，且快照里三个数**逐个相等**于各自的真值", () => {
    threeLedgersNonEmpty();
    const s = snap();

    // ① 先证明三个账本真的都被喂到了（任一恒空，下面的相等断言就是装饰）
    expect(realBranchRecords("s1"), "branchFolder 恒空 ⇒ 这张表是空真").toBe(4);
    expect(peek("s1").userInputs.length, "userInputs 恒空 ⇒ 这张表是空真").toBe(
      3,
    );
    expect(peek("s1").window.pendingCount, "pending 恒空 ⇒ 这张表是空真").toBe(
      2,
    );

    // ② 快照读出来的必须**等于**它们 —— 相等断言，不是下界
    expect(s.branchRecords).toBe(4);
    expect(s.userInputs).toBe(3);
    expect(s.pending).toBe(2);

    console.log(`[S6-丙] debugSnapshot = ${tm.debugSnapshot()}`);
  });

  it("快照里的三个数与三个账本**同源**（不是各自写死的常数）", () => {
    threeLedgersNonEmpty();
    expect(snap().branchRecords).toBe(realBranchRecords("s1"));
    expect(snap().userInputs).toBe(peek("s1").userInputs.length);
    expect(snap().pending).toBe(peek("s1").window.pendingCount);

    // 再喂两条 ⇒ 三个数各自该怎么动就怎么动（写死的常数在这里当场红）
    feed(userLine(102, "u102", "再说一句")); // 渲染：branch+1、inputs+1、pending 不动
    feed(userLine(3, "u3", "又一条旧的")); // 收纳：branch+1、inputs+1、pending+1
    const s = snap();
    expect(s.branchRecords).toBe(6);
    expect(s.userInputs).toBe(5);
    expect(s.pending).toBe(3);
  });

  it("`branchRecords` 不许是 -1 —— -1 = 那根尺子断了（字段被改名读不到）", () => {
    threeLedgersNonEmpty();
    expect(snap().branchRecords).not.toBe(-1);
  });

  it("重投（换 seq 的同一条 uuid）不许让任何一个账本涨", () => {
    threeLedgersNonEmpty();
    const before = snap();
    feed({ ...userLine(100, "u100", "最新那一句"), seq: 999 });
    const after = snap();
    expect(after.branchRecords).toBe(before.branchRecords);
    expect(after.userInputs).toBe(before.userInputs);
    expect(after.pending).toBe(before.pending);
  });

  it("上翻补批把收纳的那些渲出来 ⇒ `pending` 归零，另外两个账本一条不少", () => {
    threeLedgersNonEmpty();
    peek("s1").streamEl.dispatchEvent(new Event("scroll"));

    const s = snap();
    expect(s.pending, "补批没真的跑 ⇒ 这一格是空真").toBe(0);
    expect(s.branchRecords, "账本不该随渲染缩水").toBe(4);
    expect(s.userInputs, "清单不该随渲染缩水").toBe(3);
  });
});
