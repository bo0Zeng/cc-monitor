/**
 * K-R45 · `KR45D1`（甲 · 历史查看器）：列出你说过的每一句，点一下跳过去。
 *
 * 〔用 09-10 逐字〕「**要的就是跳过去就行**」。
 *
 * ## 🔴 口径不在这里量了（SE1）
 *
 * 「一条用户输入」的口径**只有一个住址**：后端 `observe/user_inputs.rs`（四条：`type:"user"` ＋
 * 非 `isMeta` ＋ **非 `isSidechain`** ＋ 有 uuid ＋ 纯文本非空），判据在 Rust 那侧
 * （`user_inputs_tests.rs`，每条口径两向）。查看器从 SE1 起**问后端要**清单（`list_user_inputs`），
 * 本文件量的是：后端给什么就列什么、按它的顺序 · 问的是这一份会话 · 点一下跳过去。
 * 后端那份清单由台子里的 `outlineBackend` 替身**逐条写明**（它不判 —— 不拿前端算的去对前端）。
 *
 * ## 台子住哪儿
 *
 * IPC / `ResizeObserver` / `CSS` / `scrollIntoView` / rAF 那一套桩与
 * `session-viewer-scroll.vitest.ts` **共用一份**，住 `session-viewer-rig.ts`
 * （保真边界写在那份的头注里：真渲染管线，只有 IPC 那层是假的）。
 * 本套件不叫 `flushRaf` ⇒ 双 rAF 那一格归 `KR45D0`，这里不重复量。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";

// ⚠ 必须是**异步动态 import** 的工厂（理由见 rig 里 `tauriCoreMock` 的头注：`vi.mock` 提升 + TDZ）。
vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  return rig.tauriCoreMock();
});
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));
// 「这一条显示不了」那一格要一条渲染时真抛的记录：uuid 叫 `boom` 的那一条在真渲染器之前抛（别的照真渲染器走）。
vi.mock("../../../../src/frontend/ui/render-stream-record", async (orig) => {
  const real = await orig<typeof import("../../../../src/frontend/ui/render-stream-record")>();
  return {
    ...real,
    renderStreamRecord: (p: { message: { uuid?: string } }, ...rest: unknown[]) => {
      if (p.message.uuid === "boom") throw new Error("这一条坏了");
      return (real.renderStreamRecord as (...a: unknown[]) => void)(p, ...rest);
    },
  };
});

import {
  installViewerRig,
  expectLoaded,
  assistantLine,
  userLine,
  viewerRig,
  outlineBackend,
  outlineEntry,
  settleOutline,
  type RigPayload,
  type ViewerRigHandles,
} from "../../../test-support/session-viewer-rig";
import { REPO_ROOT } from "../../../test-support/repo-root";
import { SessionViewer } from "../../../../src/frontend/ui/views/session-viewer";
import { invoke } from "@tauri-apps/api/core";
import { chanArgsJson, sessionReadCalls, type ChanCallArgs } from "../../../test-support/chan-fake";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

let rig: ViewerRigHandles;

/** ESC 中断标记那一条：monitor 按 `agents/claudecode/text.rs::user_text` 判出来的成品（夹具显式写，前端不判）。 */
const INTERRUPT = { clean: "", interrupt: true };

/**
 * 挂一份会话。`outline` = 后端这一刻说清单里有哪几条（缺省：`lines` 里 user 行的 uuid **按给的顺序**，
 * 摘要取正文 —— 这只是夹具的省事写法，**不是**判定：要排掉谁的用例一律显式传）。
 */
async function mount(lines: RigPayload[], outline?: string[]): Promise<SessionViewer> {
  viewerRig.chunk = lines;
  const text = (p: RigPayload): string => {
    const c = (p.message as { message?: { content?: unknown } }).message?.content;
    return typeof c === "string" ? c : "";
  };
  outlineBackend.entries = (outline ?? lines
    .filter((p) => (p.message as { type?: string }).type === "user")
    .map((p) => (p.message as { uuid: string }).uuid))
    .map((u) => outlineEntry(u, text(lines.find((p) => (p.message as { uuid?: string }).uuid === u)!)));
  const v = new SessionViewer();
  document.body.appendChild(v.element);
  await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true });
  await settleOutline();
  expectLoaded(v.element);
  return v;
}

const rowsOf = (v: SessionViewer): HTMLButtonElement[] => [
  ...v.element.querySelectorAll<HTMLButtonElement>(".user-input-row"),
];
/** 工具行「你说过的话 · N ▾」那颗按钮（设计稿「文件与历史」乙4-④：大纲只这一处入口）。 */
const toggleOf = (v: SessionViewer): HTMLButtonElement =>
  v.element.querySelector<HTMLButtonElement>('[data-role="said"]')!;
/** 按钮上写的那一句（宽档那一形；中档、窄档另有一份只写数字的，靠 CSS 换）。 */
const saidOf = (v: SessionViewer): string => toggleOf(v).querySelector('[data-part="full"]')!.textContent ?? "";
/** 清单面板：收着时住查看器里，开着时搬进 kit 浮层（挂在 body 上）⇒ 按文档找。 */
const panelOf = (_v: SessionViewer): HTMLElement =>
  document.querySelector<HTMLElement>(".user-inputs")!;
/**
 * 🔴 **找卡一律限定在消息流容器里**。`[data-uuid]` 在本仓只有一个意思（渲染出来的消息卡），
 * 而清单行是另一种东西 —— 早期一版把行也写成 `data-uuid`，这个判据当场把两者混在一起
 * 数成 **350**（150 张卡 + 200 行）。行的属性因此改名 `data-input-uuid`；
 * 这两个 helper 让「卡」与「行」在判据里也分得开，别再退回 `v.element.querySelector`。
 */
const streamOf = (v: SessionViewer): HTMLElement =>
  v.element.querySelector<HTMLElement>(".session-viewer-stream")!;
const cardOf = (v: SessionViewer, uuid: string): HTMLElement | null =>
  streamOf(v).querySelector<HTMLElement>(`[data-uuid="${uuid}"]`);

beforeEach(() => {
  rig = installViewerRig();
});

afterEach(() => vi.unstubAllGlobals());

describe("SE1 清单挂进查看器：后端给什么就列什么（查看器不判）", () => {
  it("问的是这一份会话（本机 origin 逐字 `<local>`、从 0 起），列出来的 == 后端给的，顺序不动", async () => {
    const v = await mount(
      [
        userLine(1, "u1", "第一句"),
        assistantLine(2, "a1", "回复"),
        userLine(3, "u3", "skill 展开的 prompt", { isMeta: true }),
        userLine(7, "u7", "第二句"),
      ],
      ["u1", "u7"], // 后端说：主线用户输入是这两条（isMeta 那条它排掉了）
    );
    const calls = sessionReadCalls(vi.mocked(invoke).mock.calls, "list_user_inputs");
    expect(calls).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 0 }]);
    const rows = rowsOf(v);
    expect(rows.map((r) => r.dataset.inputUuid)).toEqual(["u1", "u7"]);
    expect(rows[0].textContent).toBe("1. 第一句");
    expect(saidOf(v)).toBe("你说过的话 · 2");
  });

  it("🔴 查看器不自己判：后端没列的 user 行不出现（反向：旧的 TS 判定若还在，u3 会冒出来）", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), userLine(3, "u3", "也像一句用户输入")], ["u1"]);
    expect(rowsOf(v).map((r) => r.dataset.inputUuid)).toEqual(["u1"]);
  });

  it("后端要不到（老后端 / 本机后端不在）⇒ 灰掉、原因挂在开关提示上", async () => {
    viewerRig.chunk = [userLine(1, "u1", "第一句")];
    const v = new SessionViewer();
    document.body.appendChild(v.element);
    outlineBackend.available = false;
    outlineBackend.reason = "本机后端不在";
    await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true });
    await settleOutline();
    expect(rowsOf(v).length).toBe(0);
    expect(toggleOf(v).disabled).toBe(true);
    // 「老后端」那一档今天是通道的「对端事前说不认」（`peer/unsupported`）—— 它不带原因，
    //   那句话由 `session-reads.ts` 说（原因文字只在 `refused` 那一档原样带过来，下一格量它）。
    expect(toggleOf(v).title).toContain("后端版本旧");
    // 瞬时那一档（对端说「不行」）：原因原样带上。
    const w = new SessionViewer();
    document.body.appendChild(w.element);
    outlineBackend.failure = "transport";
    await w.load({ jsonlPath: "/p/s2.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true });
    await settleOutline();
    expect(toggleOf(w).title).toContain("本机后端不在");
  });

  it("面板默认收着，点开关才展开（默认收着 ⇒ 对既有布局零影响）；再点一次收回查看器里", async () => {
    const v = await mount([userLine(1, "u1", "第一句")]);
    expect(panelOf(v).hidden).toBe(true);
    expect(v.element.contains(panelOf(v))).toBe(true);
    expect(toggleOf(v).getAttribute("aria-expanded")).toBe("false");
    toggleOf(v).click();
    expect(panelOf(v).hidden).toBe(false);
    expect(v.element.contains(panelOf(v)), "开着时在 kit 浮层里，不在查看器里").toBe(false);
    expect(toggleOf(v).getAttribute("aria-expanded")).toBe("true");
    toggleOf(v).click();
    expect(panelOf(v).hidden).toBe(true);
    expect(v.element.contains(panelOf(v))).toBe(true);
    expect(toggleOf(v).getAttribute("aria-expanded")).toBe("false");
  });

  it("一条用户输入都没有 ⇒ 开关禁用、清单为空（不给一个点了没反应的入口）", async () => {
    const v = await mount([assistantLine(1, "a1", "只有回复")]);
    expect(rowsOf(v).length).toBe(0);
    expect(toggleOf(v).disabled).toBe(true);
    expect(saidOf(v)).toBe("你说过的话"); // 0 条不挂计数
  });
});

describe("KR45D1 点一下跳过去", () => {
  it("点某一行 ⇒ 滚到那条对应的卡上（走的就是搜索命中今天在用的那条路）", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), userLine(2, "u2", "第二句")]);
    const target = cardOf(v, "u2")!;
    rowsOf(v)[1].click();
    expect(rig.scrollIntoView).toHaveBeenCalledTimes(1);
    expect(rig.scrollIntoView.mock.instances[0]).toBe(target); // 点名滚给了谁
    expect(target.classList.contains("search-hit-flash")).toBe(true);
    expect(rowsOf(v)[1].dataset.unjumpable).toBeUndefined();
  });

  // ★★ 这一条是整件活的**理由**：长会话首屏只渲染末尾 150 条，
  //    而「找不回自己刚才说过的那句话」说的恰恰是**已经滚没了**的那些。
  //    清单若跟着 DOM 走，最需要它的那一段正好一条都列不出来。
  it("长会话：清单是全量的（DOM 只渲染了尾段），点没渲染的那条也跳得过去", async () => {
    const lines = Array.from({ length: 200 }, (_, i) => userLine(i + 1, `u${i + 1}`, `第 ${i + 1} 句`));
    const v = await mount(lines);

    const inDom = streamOf(v).querySelectorAll("[data-uuid]").length;
    expect(inDom).toBe(150); // TAIL_INITIAL：首屏只渲染末尾 150 条
    expect(rowsOf(v).length).toBe(200); // 而清单是 200 条 —— 分母 = 全部 200 条用户输入
    // 分水岭：清单条数 > DOM 里的卡数 ⇒ 它确实不是扫 DOM 扫出来的
    expect(rowsOf(v).length).toBeGreaterThan(inDom);

    expect(cardOf(v, "u1")).toBeNull(); // 第 1 条还没渲染
    rowsOf(v)[0].click();
    const target = cardOf(v, "u1");
    expect(target).not.toBeNull(); // 点下去把目标岛渲染出来了
    expect(rig.scrollIntoView.mock.instances[0]).toBe(target);
    expect(rowsOf(v)[0].dataset.unjumpable).toBeUndefined();
  });

  // ★ 活体夹具：清单里有、渲染不建卡的一条（后端清单今天已按同一条规则排掉剥空的项，
  //   这一形只剩「清单与渲染之间别的原因」—— 夹具用替身清单造出它）。
  //   `KR45D3` / `§0c` 的红线是「**不许静默产出那一形**」——这里断的就是「它没静默」。
  it("跳不过去的那一条不许静默：标出来（这条不等价是自陈的，这里给它一个活体）", async () => {
    const v = await mount([
      userLine(1, "u1", "第一句"),
      userLine(2, "u2", "[Request interrupted by user]", { userText: INTERRUPT }),
    ]);
    // 先证明夹具真的落在那一形上：清单有它，DOM 没有它的卡
    expect(rowsOf(v).map((r) => r.dataset.inputUuid)).toEqual(["u1", "u2"]);
    expect(cardOf(v, "u2")).toBeNull();

    rowsOf(v)[1].click();

    expect(rowsOf(v)[1].dataset.unjumpable).toBe("1"); // 看得出来
    expect(rowsOf(v)[1].title).toContain("无法定位");
    expect(rig.scrollIntoView).not.toHaveBeenCalled();
  });

  // ★★ 顶了 PM 一句话，这一格就是那句话的读数（件 `§5.5`）。
  //    PM 给「跳成功时标记要删掉」的可达性理由是「**查看器**分批渲染，T 没渲染、T+1 渲染出来」。
  //    实测**对查看器不成立**：`scrollToMessage` 跳之前会先把目标那一段渲出来，
  //    所以查看器里一行被标上 `unjumpable`，当且仅当那条记录**渲染出来就是空的** ——
  //    而那是永久的。下面这一格把「永久」钉住：整个会话一条不剩地渲染完了，它照样没有卡，
  //    再点一次还是标着。⇒「不可跳 → 可跳」这条转移的**真住址是实时窗口**
  //    （`live-user-inputs.vitest.ts` 那条「上翻补批之后再点」是它的活体）。
  it("查看器这一侧：跳空是**永久**的（全渲染完照样不建卡）⇒ 转移的真住址不在这里", async () => {
    const v = await mount([
      userLine(1, "u1", "第一句"),
      userLine(2, "u2", "[Request interrupted by user]", { userText: INTERRUPT }),
    ]);
    // 先证明「没有任何未渲染的段留着」——否则下面那句「永久」是空真
    const status = v.element.querySelector('[data-role="status"]')!.textContent ?? "";
    expect(status, "还有未渲染的段 ⇒ 这一格证不了「永久」").toBe("2 条");

    rowsOf(v)[1].click();
    expect(rowsOf(v)[1].dataset.unjumpable).toBe("1");
    rowsOf(v)[1].click(); // 再点一次：查看器里没有任何东西能让这张卡长出来
    expect(cardOf(v, "u2")).toBeNull();
    expect(rowsOf(v)[1].dataset.unjumpable).toBe("1");
  });

  it("换一个会话 ⇒ 清单跟着换（旧会话的句子不许挂在新会话上）", async () => {
    const v = await mount([userLine(1, "u1", "旧会话第一句"), userLine(2, "u2", "旧会话第二句")]);
    expect(rowsOf(v).length).toBe(2);

    viewerRig.chunk = [userLine(1, "n1", "新会话唯一一句")];
    outlineBackend.entries = [outlineEntry("n1", "新会话唯一一句")];
    await v.load({ jsonlPath: "/p/s2.jsonl", displayTitle: "T2", origin: LOCAL_ORIGIN, suppressBranch: true });
    await settleOutline();
    expectLoaded(v.element);

    expect(rowsOf(v).map((r) => r.dataset.inputUuid)).toEqual(["n1"]);
    expect(saidOf(v)).toBe("你说过的话 · 1");
  });
});

/**
 * 上一轮申报的第 4 笔债：面板样式内联（`style.cssText`），因为 `src/frontend/ui/styles.css` 在写区外。
 * 已搬进 `.user-inputs` / `.user-input-row` / `.user-input-row[data-unjumpable]`
 * （第三轮把类名从 `.session-viewer-*` 改成中性名 —— 这块界面现在两条路共用，
 * 而一个会出现在实时 tab 里的元素叫「session-viewer-…」是名字在说谎）。
 * 下面**两格是一对**，各买各的：
 * 第一格量的是「JS 这边不再拼样式」（行为，jsdom 量得到）；
 * 第二格量的是「CSS 那边真有宿主」—— jsdom **不加载** `styles.css`，
 * 把那三条规则整段删掉，第一格**一条都不会红**。
 * （同形先例：`account-chip.vitest.ts` 的 `.account-picker-status.warn` 那一格。）
 */
describe("KR45 债二：清单的样式住 styles.css，不再内联", () => {
  it("面板与清单行都不带内联 style（含「跳不过去」那一行 —— 变灰也不许退回内联）", async () => {
    const v = await mount([
      userLine(1, "u1", "第一句"),
      userLine(2, "u2", "[Request interrupted by user]", { userText: INTERRUPT }),
    ]);
    expect(panelOf(v).getAttribute("style")).toBeNull();
    expect(rowsOf(v).map((r) => r.getAttribute("style"))).toEqual([null, null]);

    rowsOf(v)[1].click(); // 这一条落不到卡上 ⇒ 会被标出来

    expect(rowsOf(v)[1].dataset.unjumpable).toBe("1"); // 标记还在（呈现的钩子就是它）
    expect(rowsOf(v)[1].getAttribute("style")).toBeNull(); // 而灰**不是**拿内联 opacity 涂的
  });

  it("那三条规则在 styles.css 里真有宿主（jsdom 不加载 CSS ⇒ 上一格盖不住这一形）", () => {
    // ⚠ 匹配单位是**一整行选择器**，不是子串 —— 子串比事实小，
    //   有人写 `.user-input-row .x { … }` 也会命中，而那一行根本没上样式。
    const cssLines = readFileSync(`${REPO_ROOT}/src/frontend/ui/styles.css`, "utf8")
      .split("\n")
      .map((l) => l.trim());
    // 抽取器自检：先确认这把尺子够得着那个文件（不然下面几条是空真）。
    expect(cssLines.length, "读到的 styles.css 只有几行 —— 尺子坏了").toBeGreaterThan(1000);
    expect(cssLines, "读到的不是 styles.css —— 连基准那条规则都没有").toContain(
      ".session-viewer {",
    );

    expect(cssLines, "面板没有 CSS 宿主 ⇒ 它退回一块没有高度上限、不滚动、无底边的裸 div").toContain(
      ".user-inputs {",
    );
    expect(cssLines, "清单行没有 CSS 宿主 ⇒ 每一行退回浏览器默认按钮长相").toContain(
      ".user-input-row {",
    );
    expect(cssLines, "「跳不过去那一条变灰」没有宿主 ⇒ 标记还在、但用户看不出来").toContain(
      ".user-input-row[data-unjumpable] {",
    );
    // 开关按钮在这之前**全仓零条规则** ⇒ 暗色界面上一个白底默认按钮。
    // 那三条共用规则都被钉住了，唯独它没有 —— 这一格补上。
    expect(cssLines, "开关按钮没有 CSS 宿主 ⇒ 暗色主题上退回浏览器默认按钮（浅底黑字）").toContain(
      ".user-inputs-toggle {",
    );

    // 🔴 面板规则里**绝不许出现 `display`**：面板靠 `el.hidden` 收起，而 `hidden` 就是
    //    UA 样式表里的 `display:none`，作者样式里任何一条 `display` 都盖得掉它
    //    ⇒ 面板从此永远展开，而「面板默认收着」那一格断的是 `.hidden` 属性，照样绿。
    const open = cssLines.indexOf(".user-inputs {");
    const body = cssLines.slice(open + 1, cssLines.indexOf("}", open));
    expect(
      body.filter((l) => /^display\s*:/.test(l)),
      "`.user-inputs` 里出现了 display ⇒ 它会盖掉 hidden 的 display:none，面板再也收不起来",
    ).toEqual([]);
  });
});

// 历史查看器的 Ctrl+F：工具行的查找框 ＋ 框下就地展开的命中清单（`find-strip.ts`，主窗口的会话内查找复用同一个），查的是后端 `history-find`。
describe("㊱③ 查看器的 Ctrl+F：工具行查找框 ＋ 问后端 `history-find`", () => {
  it("焦点进框 · 回车才问 · 问的是这一份会话 · 后端给什么命中就列什么 · 点命中跳到那张卡 · ✕ 收起", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), assistantLine(2, "a1", "回复里有 needle")]);
    viewerRig.find = {
      available: true,
      total: 1,
      hits: [{ uuid: "a1", kind: "assistant", before: "回复里有 ", matched: "needle", after: "" }],
    };
    const strip = v.element.querySelector<HTMLElement>('[data-role="find-strip"]')!;
    expect(strip, "查看器里没有命中清单那一条").toBeTruthy();
    expect(strip.hidden).toBe(true);
    v.openFind();
    const input = v.element.querySelector<HTMLInputElement>("[data-role=\"find-input\"]")!;
    expect(document.activeElement).toBe(input);
    expect(strip.hidden, "只给焦点不搜：回车才问那台").toBe(true);
    input.value = "needle";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settleOutline();
    expect(strip.hidden).toBe(false);
    expect(sessionReadCalls(vi.mocked(invoke).mock.calls, "find_in_session")).toEqual([
      { origin: "<local>", jsonlPath: "/p/s1.jsonl", query: "needle", includeTools: false },
    ]);
    expect(strip.querySelector('[data-role="find-head"]')!.textContent).toBe("1 处");
    const hits = [...strip.querySelectorAll<HTMLButtonElement>('[data-role="find-hit"]')];
    expect(hits.map((h) => h.dataset.hitUuid)).toEqual(["a1"]);
    hits[0].click();
    await settleOutline();
    expect(hits[0].dataset.unjumpable, "命中那一条跳空了 —— 查看器的「跳」没接到查找上").toBeUndefined();
    expect(cardOf(v, "a1")).toBeTruthy();
    strip.querySelector<HTMLButtonElement>('button[aria-label="收起查找结果"]')!.click();
    expect(strip.hidden).toBe(true);
  });

  it("命中片段去行内排版记号（`**` `__` 反引号 · 开头的 # / >），命中词照旧高亮", async () => {
    const v = await mount([assistantLine(1, "a1", "统一做**重试 + 退避**")]);
    viewerRig.find = {
      available: true,
      total: 2,
      hits: [
        { uuid: "a1", kind: "assistant", before: "统一做**", matched: "重试", after: " ＋ `退避` ＋ __整体超时__**；" },
        { uuid: "a1", kind: "assistant", before: "## > ", matched: "**重试**", after: "" },
      ],
    };
    v.openFind();
    const input = v.element.querySelector<HTMLInputElement>('[data-role="find-input"]')!;
    input.value = "重试";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settleOutline();
    const hits = [...v.element.querySelectorAll<HTMLElement>('[data-role="find-hit"]')];
    expect(hits.map((h) => h.textContent)).toEqual(["统一做重试 ＋ 退避 ＋ 整体超时；", "重试"]);
    expect(hits.map((h) => h.querySelector("mark")!.textContent)).toEqual(["重试", "重试"]);
  });

  it("勾「含工具输出与思考」⇒ 带着它再问一次；没命中写「无匹配「词」」", async () => {
    const v = await mount([userLine(1, "u1", "第一句")]);
    viewerRig.find = { available: true, total: 0, hits: [] };
    v.openFind();
    const input = v.element.querySelector<HTMLInputElement>("[data-role=\"find-input\"]")!;
    input.value = "nothing";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settleOutline();
    const strip = v.element.querySelector<HTMLElement>('[data-role="find-strip"]')!;
    expect(strip.querySelector('[data-role="find-head"]')!.textContent).toBe("无匹配「nothing」");
    const box = strip.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    box.click();
    await settleOutline();
    const calls = sessionReadCalls(vi.mocked(invoke).mock.calls, "find_in_session").map((c) => (c as { includeTools: boolean }).includeTools);
    expect(calls.slice(-2)).toEqual([false, true]);
  });
});

// 设计稿「文件与历史」乙4-④：头两行 · 底一行只说条数 · 显示不了的那一条在卡位上说 · 读不出 ⇒ 错误条 ＋［重试］· 按轮折叠与主窗口同一个。
describe("乙4-④ 查看器的头 · 底一行 · 各态", () => {
  const status = (v: SessionViewer): string => v.element.querySelector('[data-role="status"]')!.textContent ?? "";

  it("底一行只说条数（没有「首屏 ms」「渲染失败」那类读数）；长会话没渲完 ⇒「{n} 条 · 上翻加载更早」", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), assistantLine(2, "a1", "回复")]);
    expect(status(v)).toBe("2 条");
    const many: RigPayload[] = [];
    for (let i = 1; i <= 200; i++) many.push(userLine(i, `m${i}`, `第 ${i} 句`));
    const w = await mount(many);
    expect(status(w)).toBe("200 条 · 上翻加载更早");
    expect(status(w)).not.toMatch(/ms|渲染/);
  });

  it("某一条显示不了 ⇒ 卡的位置上「这一条显示不了」［复制详情］（复制的是那一条原文 ＋ 原因），其余照画、状态行不报数", async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText: write } });
    const v = await mount([userLine(1, "u1", "第一句"), userLine(2, "boom", "坏的"), assistantLine(3, "a1", "回复")]);
    const kids = [...streamOf(v).querySelectorAll<HTMLElement>("[data-uuid], [data-role=\"broken\"]")];
    expect(kids.map((k) => k.dataset.uuid ?? `broken:${k.dataset.seq}`)).toEqual(["u1", "broken:2", "a1"]);
    const broken = streamOf(v).querySelector<HTMLElement>('[data-role="broken"]')!;
    expect(broken.textContent).toContain("这一条显示不了");
    [...broken.querySelectorAll("button")].find((b) => b.textContent === "复制详情")!.click();
    expect(write.mock.calls[0][0]).toContain("这一条坏了");
    expect(write.mock.calls[0][0]).toContain('"uuid": "boom"');
    expect(status(v)).toBe("3 条");
  });

  it("头：标题 ＋ 宿主给的徽标 · 按钮 · 第二行；读完在第二行后面接「· {n} 条」", async () => {
    viewerRig.chunk = [userLine(1, "u1", "第一句")];
    outlineBackend.entries = [];
    const v = new SessionViewer();
    document.body.appendChild(v.element);
    const badge = Object.assign(document.createElement("span"), { textContent: "Codex" });
    const act = Object.assign(document.createElement("button"), { textContent: "恢复" });
    const proj = Object.assign(document.createElement("span"), { textContent: "orders" });
    await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "支付回调验签", origin: LOCAL_ORIGIN, suppressBranch: true, head: { badges: [badge], actions: act, meta: [proj] } });
    await settleOutline();
    const title = v.element.querySelector('[data-role="title"]')!;
    expect(title.textContent).toBe("支付回调验签");
    expect(title.nextElementSibling?.textContent).toBe("Codex");
    expect(v.element.contains(act)).toBe(true);
    expect(v.element.querySelector('[data-role="meta"]')!.textContent).toBe("orders1 条");
  });

  it("读不出 ⇒ 头下一条错误条「读取会话失败 · …」［重试］，点了再读同一份", async () => {
    viewerRig.chunk = [userLine(1, "u1", "第一句")];
    viewerRig.failPage = true;
    const v = new SessionViewer();
    document.body.appendChild(v.element);
    await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true });
    await settleOutline();
    const bar = v.element.querySelector<HTMLElement>('[role="alert"]')!;
    expect(bar.textContent).toContain("读取会话失败");
    expect(status(v)).toBe("");
    viewerRig.failPage = false;
    [...bar.querySelectorAll("button")].find((b) => b.textContent === "重试")!.click();
    await settleOutline();
    expect(v.element.querySelector('[role="alert"]')).toBeNull();
    expect(status(v)).toBe("1 条");
  });

  it("按轮折叠与主窗口同一个：读完问那台这一份会话的 `history-turns`（从 0 起）", async () => {
    await mount([userLine(1, "u1", "第一句")]);
    const turns = vi.mocked(invoke).mock.calls
      .filter((c) => c[0] === "chan_call" && (c[1] as unknown as { op?: string }).op === "history-turns")
      .map((c) => chanArgsJson(c[1] as unknown as ChanCallArgs));
    expect(turns.at(-1)).toEqual({ path: "/p/s1.jsonl", from: 0 });
  });
});
