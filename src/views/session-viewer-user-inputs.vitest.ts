/**
 * K-R45 · `KR45D1`（甲 · 历史查看器）：列出你说过的每一句，点一下跳过去。
 *
 * 〔用 09-10 逐字〕「**要的就是跳过去就行**」。
 *
 * ## 🔴 分母写在判据的名字里
 *
 * 「一条用户输入」的口径**只有一个住址**：`user-input-index.ts::collectUserInputs`
 * 的头注（`type:"user"` ＋ 非 `isMeta` ＋ **非 `isSidechain`** ＋ 有 uuid ＋ 纯文本非空）。
 * 「**sidechain / 子 agent 里的用户消息算不算**」这一问，本件选的是**不算** ——
 * 这份清单回答的是「**我**在这个会话里说过什么」。这个选择写进了下面那条判据的名字。
 *
 * ## 台子的保真边界（同 `session-viewer-scroll.vitest.ts`，那边头注写得更细）
 *
 * 真渲染管线 + 真 `collectUserInputs`，只有 IPC 那一层是假的。
 * ⚠ **本文件的 mock 骨架与 `session-viewer-scroll.vitest.ts` 是两份**：
 *   写区（`src/views`）里没有放共用测试夹具的地方（`src/test-support/` 本轮在写区外）。
 *   ⇒ 这是一笔**申报过的债**，两份骨架漂了就会变成「两个套件量的不是同一个台子」。
 *   PM 若同意开 `src/test-support/session-viewer-rig.ts`，两边都该搬过去。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const rig = vi.hoisted(() => ({ chunk: [] as unknown[] }));

vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "stream_read_session_jsonl") {
      const ch = args.onChunk as { onmessage?: ((v: unknown) => void) | null };
      ch.onmessage?.(rig.chunk);
      return rig.chunk.length;
    }
    return undefined;
  }),
}));
vi.mock("../fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { SessionViewer } from "./session-viewer";
import { collectUserInputs } from "./user-input-index";

type Payload = { session_id: string; cwd: null; path: string; seq: number; message: unknown };

function line(seq: number, message: Record<string, unknown>): Payload {
  return { session_id: "s1", cwd: null, path: "/p/s1.jsonl", seq, message };
}

function user(
  seq: number,
  uuid: string | null,
  content: unknown,
  over: Record<string, unknown> = {},
): Payload {
  return line(seq, {
    type: "user",
    uuid,
    timestamp: `2026-09-10T00:00:${String(seq % 60).padStart(2, "0")}.000Z`,
    message: { role: "user", content },
    cwd: null,
    sessionId: "s1",
    isSidechain: false,
    isMeta: false,
    parentUuid: null,
    forkedFrom: null,
    ...over,
  });
}

function assistant(seq: number, uuid: string, text: string): Payload {
  return line(seq, {
    type: "assistant",
    uuid,
    timestamp: `2026-09-10T00:00:${String(seq % 60).padStart(2, "0")}.000Z`,
    message: { role: "assistant", content: [{ type: "text", text }] },
    sessionId: "s1",
    isSidechain: false,
    requestId: null,
    parentUuid: null,
    forkedFrom: null,
    isApiErrorMessage: false,
    error: null,
    apiErrorStatus: null,
  });
}

let scrollIntoView: ReturnType<typeof vi.fn>;

function expectLoaded(v: SessionViewer): void {
  const status = v.element.querySelector(".history-status")!.textContent ?? "";
  expect(status).not.toContain("加载失败");
}

async function mount(lines: Payload[]): Promise<SessionViewer> {
  rig.chunk = lines;
  const v = new SessionViewer(() => {});
  document.body.appendChild(v.element);
  await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", suppressBranch: true });
  expectLoaded(v);
  return v;
}

const rowsOf = (v: SessionViewer): HTMLButtonElement[] => [
  ...v.element.querySelectorAll<HTMLButtonElement>(".session-viewer-input-row"),
];
const toggleOf = (v: SessionViewer): HTMLButtonElement =>
  v.element.querySelector<HTMLButtonElement>(".session-viewer-inputs-toggle")!;
const panelOf = (v: SessionViewer): HTMLElement =>
  v.element.querySelector<HTMLElement>(".session-viewer-inputs")!;
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
  document.body.replaceChildren();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
  vi.stubGlobal("requestAnimationFrame", () => 0); // 本套件不量双 rAF 那一格（它归 KR45D0）
  vi.stubGlobal("CSS", { escape: (s: string) => s.replace(/["\\]/g, "\\$&") });
  scrollIntoView = vi.fn();
  Element.prototype.scrollIntoView = scrollIntoView as unknown as Element["scrollIntoView"];
});

afterEach(() => vi.unstubAllGlobals());

describe("KR45D1 挑句子这一半（collectUserInputs，纯函数）", () => {
  it("口径就是那四条：type/isMeta/isSidechain/有 uuid 且纯文本非空", () => {
    const got = collectUserInputs([
      { type: "user", uuid: "a", timestamp: "t", message: { content: "第一句" } },
      { type: "assistant", uuid: "b", message: { content: [{ type: "text", text: "回复" }] } },
      { type: "user", uuid: "c", isMeta: true, message: { content: "注入的 prompt" } },
      { type: "user", uuid: "d", isSidechain: true, message: { content: "子 agent 的活" } },
      { type: "user", uuid: "e", message: { content: [{ type: "tool_result", content: "x" }] } },
      { type: "user", uuid: null, message: { content: "没有 uuid 就跳不过去" } },
      { type: "user", uuid: "g", message: { content: "   " } },
      { type: "user", uuid: "h", timestamp: "t2", message: { content: [{ type: "text", text: "第二句" }] } },
    ]);
    expect(got.map((e) => e.uuid)).toEqual(["a", "h"]);
    expect(got.map((e) => e.payloadIndex)).toEqual([0, 7]); // 下标是原数组里的，不是过滤后的
    expect(got[1].excerpt).toBe("第二句");
  });

  it("摘要压成一行并截断（清单一行一条，别把布局撑爆）", () => {
    const long = "甲".repeat(200);
    const [e] = collectUserInputs([
      { type: "user", uuid: "a", message: { content: `多\n行\n  文  本` } },
    ]);
    expect(e.excerpt).toBe("多 行 文 本");
    const [f] = collectUserInputs([{ type: "user", uuid: "b", message: { content: long } }]);
    expect(f.excerpt.length).toBe(81); // 80 字 + 省略号
    expect(f.excerpt.endsWith("…")).toBe(true);
  });
});

describe("KR45D1 清单挂进查看器：条数 = 主线用户输入条数（子 agent/sidechain 的不算）", () => {
  it("不多不少 —— 混进 assistant / isMeta / sidechain / tool_result / 无 uuid 也只列主线那些", async () => {
    const v = await mount([
      user(1, "u1", "第一句"),
      assistant(2, "a1", "回复"),
      user(3, "u3", "skill 展开的 prompt", { isMeta: true }),
      user(4, "u4", "子 agent 里说的", { isSidechain: true }),
      user(5, "u5", [{ type: "tool_result", tool_use_id: "t1", content: "结果" }]),
      user(6, null, "没有 uuid"),
      user(7, "u7", "第二句"),
    ]);
    const rows = rowsOf(v);
    expect(rows.length).toBe(2); // 分母 = 7 条记录里的 2 条主线用户输入
    expect(rows.map((r) => r.dataset.inputUuid)).toEqual(["u1", "u7"]);
    expect(rows[0].textContent).toBe("1. 第一句");
    expect(toggleOf(v).textContent).toBe("我说过的 2 句");
  });

  it("面板默认收着，点开关才展开（默认收着 ⇒ 对既有布局零影响）", async () => {
    const v = await mount([user(1, "u1", "第一句")]);
    expect(panelOf(v).hidden).toBe(true);
    expect(toggleOf(v).getAttribute("aria-expanded")).toBe("false");
    toggleOf(v).click();
    expect(panelOf(v).hidden).toBe(false);
    expect(toggleOf(v).getAttribute("aria-expanded")).toBe("true");
  });

  it("一条用户输入都没有 ⇒ 开关禁用、清单为空（不给一个点了没反应的入口）", async () => {
    const v = await mount([assistant(1, "a1", "只有回复")]);
    expect(rowsOf(v).length).toBe(0);
    expect(toggleOf(v).disabled).toBe(true);
    expect(toggleOf(v).textContent).toBe("我说过的 0 句");
  });
});

describe("KR45D1 点一下跳过去", () => {
  it("点某一行 ⇒ 滚到那条对应的卡上（走的就是搜索命中今天在用的那条路）", async () => {
    const v = await mount([user(1, "u1", "第一句"), user(2, "u2", "第二句")]);
    const target = cardOf(v, "u2")!;
    rowsOf(v)[1].click();
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(scrollIntoView.mock.instances[0]).toBe(target); // 点名滚给了谁
    expect(target.classList.contains("search-hit-flash")).toBe(true);
    expect(rowsOf(v)[1].dataset.unjumpable).toBeUndefined();
  });

  // ★★ 这一条是整件活的**理由**：长会话首屏只渲染末尾 150 条，
  //    而「找不回自己刚才说过的那句话」说的恰恰是**已经滚没了**的那些。
  //    清单若跟着 DOM 走，最需要它的那一段正好一条都列不出来。
  it("长会话：清单是全量的（DOM 只渲染了尾段），点没渲染的那条也跳得过去", async () => {
    const lines = Array.from({ length: 200 }, (_, i) => user(i + 1, `u${i + 1}`, `第 ${i + 1} 句`));
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
    expect(scrollIntoView.mock.instances[0]).toBe(target);
    expect(rowsOf(v)[0].dataset.unjumpable).toBeUndefined();
  });

  // ★ 活体夹具，钉的是 `user-input-index.ts` 头注里**自陈的那条不等价**：
  //   渲染那边还会剥一层 `stripInternalNoise`，`[Request interrupted by user]`
  //   会被整条剥空 ⇒ 不建卡 ⇒ 清单里这一条落不到卡上。
  //   `KR45D3` / `§0c` 的红线是「**不许静默产出那一形**」——这里断的就是「它没静默」。
  it("跳不过去的那一条不许静默：标出来（这条不等价是自陈的，这里给它一个活体）", async () => {
    const v = await mount([
      user(1, "u1", "第一句"),
      user(2, "u2", "[Request interrupted by user]"),
    ]);
    // 先证明夹具真的落在那一形上：清单有它，DOM 没有它的卡
    expect(rowsOf(v).map((r) => r.dataset.inputUuid)).toEqual(["u1", "u2"]);
    expect(cardOf(v, "u2")).toBeNull();

    rowsOf(v)[1].click();

    expect(rowsOf(v)[1].dataset.unjumpable).toBe("1"); // 看得出来
    expect(rowsOf(v)[1].title).toContain("跳不过去");
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("换一个会话 ⇒ 清单跟着换（旧会话的句子不许挂在新会话上）", async () => {
    const v = await mount([user(1, "u1", "旧会话第一句"), user(2, "u2", "旧会话第二句")]);
    expect(rowsOf(v).length).toBe(2);

    rig.chunk = [user(1, "n1", "新会话唯一一句")];
    await v.load({ jsonlPath: "/p/s2.jsonl", displayTitle: "T2", suppressBranch: true });
    expectLoaded(v);

    expect(rowsOf(v).map((r) => r.dataset.inputUuid)).toEqual(["n1"]);
    expect(toggleOf(v).textContent).toBe("我说过的 1 句");
  });
});
