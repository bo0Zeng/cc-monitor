/**
 * K-R45 · `KR45D0`：给 `SessionViewer.scrollToMessage` 立哨。
 *
 * ## 为什么这个文件今天才有
 *
 * `scrollToMessage`（`session-viewer.ts`，issue #6 起就在）与它的高亮样式 `search-hit-flash`
 * 在**全部 119 个 `*.vitest.ts` 里零命中**（09-10 现打，分母 = `find src -name '*.vitest.ts'`
 * 的 119 份；命中数 = `grep -l` 的 0 份）。而 `K-R45` 要把它的调用方**从 1 个变成 2 个**
 * （今天唯一调用方是搜索命中：`history.ts` 传 `scrollToUuid: hit.uuid`）。
 * ⇒ **先立哨，再加调用方** —— 反过来做的话，哨立起来时已经罩着新代码，
 * 它证不了「原来那条路今天还好使」，只证得了「我刚写的东西自洽」。
 *
 * ## 判据故意不改被测函数的形状
 *
 * `scrollToMessage` 是 `private`，本文件**不把它改成 public 来迁就测试**。
 * 走的是本仓既有的 cast 惯例（`history-search-resume.vitest.ts` 拿 `buildSearchSession`
 * 就是这么拿的）。⇒ 测的是**今天在跑的那一版**，不是一个为了好测而新捏的形状。
 *
 * ## 台子的保真边界（写出来，别让读的人以为它比实际更强）
 *
 * - 渲染管线是**真的**：真 `cards/renderMessage` → 真 `render-stream-record` → 真
 *   `markCardUuid` 写 `data-uuid`。**没有** mock 掉「卡上有没有 `data-uuid`」这件事
 *   —— 那正是 `tabs.vitest.ts` 头注记的那族病（「量具把被量的东西 mock 没了」）。
 *   下面 `it("台子自检…")` 那一条就是钉这件事的：mock 哪天空心化，它先红。
 * - **只有 IPC 那一层是假的**：`invoke` 被换成「把夹具 payload 从 Channel 灌回去」。
 * - jsdom 没有 `ResizeObserver` / `scrollIntoView` / **`CSS`** ⇒ 本文件补桩。
 *   `scrollIntoView` 的桩正是判据的观测口：**「请求滚动到它」= 在那张卡上调了
 *   `scrollIntoView`**。jsdom 里没有真实布局，「滚没滚到」量不了，量得了的只有
 *   「请求发给了谁」。
 * - 🔴 **`load()` 整段包在 try/catch 里**（`session-viewer.ts` 的 `catch (e)` → 状态栏写
 *   「加载失败」）⇒ 定位段抛任何错都会被**吞成一句状态文**，判据只会看见「什么都没发生」。
 *   本文件第一版就栽在这上面：`CSS` 未定义 ⇒ 三条判据一起报「调了 0 次」，
 *   读起来像「函数没被调」，真相是「函数炸了」。⇒ 每个用例挂载完先 `expectLoaded()`。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

/** 夹具 payload 由每个用例塞进来，`invoke` 的桩从这里取。 */
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
// 分叉那条路与跳转无关，且它会拉起 IPC / 弹窗链路 —— 挡在门外。
vi.mock("../fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { MessageStream } from "../stream";
import { SessionViewer } from "./session-viewer";

type Payload = { session_id: string; cwd: null; path: string; seq: number; message: unknown };

function userLine(seq: number, uuid: string, text: string): Payload {
  return {
    session_id: "s1",
    cwd: null,
    path: "/p/s1.jsonl",
    seq,
    message: {
      type: "user",
      uuid,
      timestamp: `2026-09-10T00:00:${String(seq).padStart(2, "0")}.000Z`,
      message: { role: "user", content: text },
      cwd: null,
      sessionId: "s1",
      isSidechain: false,
      isMeta: false,
      parentUuid: seq > 1 ? `u${seq - 1}` : null,
      forkedFrom: null,
    },
  };
}

/** 取 viewer 的私有 `scrollToMessage`（不改被测函数的可见性，见头注）。 */
function jump(v: SessionViewer, uuid: string): void {
  (v as unknown as { scrollToMessage(u: string): void }).scrollToMessage(uuid);
}

let scrollIntoView: ReturnType<typeof vi.fn>;
let toBottom: ReturnType<typeof vi.spyOn>;
let rafQueue: FrameRequestCallback[];

/** 手动跑 rAF 队列：`scrollToMessage` 的双 rAF 幂等重发要靠它才走得到。 */
function flushRaf(rounds: number): void {
  for (let i = 0; i < rounds; i++) {
    const due = rafQueue;
    rafQueue = [];
    for (const cb of due) cb(0);
  }
}

/**
 * 🔴 `load()` 会把定位段抛出的异常吞成状态栏一句「加载失败：…」。
 * 不核这一句，一切「调了 0 次」的读数都分不清是「没调」还是「炸了」。
 */
function expectLoaded(v: SessionViewer): void {
  const status = v.element.querySelector(".history-status")!.textContent ?? "";
  expect(status).not.toContain("加载失败");
}

async function mount(lines: Payload[], scrollToUuid?: string): Promise<SessionViewer> {
  rig.chunk = lines;
  const v = new SessionViewer(() => {});
  document.body.appendChild(v.element);
  await v.load({
    jsonlPath: "/p/s1.jsonl",
    displayTitle: "T",
    suppressBranch: true, // 分支按钮不在本件射程里
    ...(scrollToUuid === undefined ? {} : { scrollToUuid }),
  });
  expectLoaded(v);
  return v;
}

beforeEach(() => {
  document.body.replaceChildren();
  rafQueue = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    rafQueue.push(cb);
    return rafQueue.length;
  });
  // jsdom 没有全局 `CSS`（真浏览器里有）⇒ 补一份**只做转义、不做别的**的桩。
  // 不补的话 `scrollToMessage` 第一行选择器就抛，而 `load()` 会把它吞掉。
  vi.stubGlobal("CSS", { escape: (s: string) => s.replace(/["\\]/g, "\\$&") });
  scrollIntoView = vi.fn();
  Element.prototype.scrollIntoView = scrollIntoView as unknown as Element["scrollIntoView"];
  toBottom = vi.spyOn(MessageStream.prototype, "scrollToBottom");
});

afterEach(() => {
  vi.unstubAllGlobals();
  toBottom.mockRestore();
});

describe("KR45D0 SessionViewer.scrollToMessage —— 今天零判据的那个函数", () => {
  // ★ 台子自检：先证明「卡上真有 data-uuid」，再谈下面三条钉什么。
  //   渲染管线哪天被 mock 空心化（`tabs.vitest.ts` 头注那族病），这一条先红，
  //   而不是让下面三条静默地「查不到卡 ⇒ 走 fallback ⇒ 照样绿」。
  it("台子自检：真渲染管线给每条 user 记录写了 data-uuid（不是 mock 出来的）", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), userLine(2, "u2", "第二句")]);
    const cards = v.element.querySelectorAll("[data-uuid]");
    expect(cards.length).toBe(2);
    expect([...cards].map((c) => c.getAttribute("data-uuid"))).toEqual(["u1", "u2"]);
    // 卡是真 user 气泡（renderMessage 的 buildUserCard），不是随便一个 div
    expect(cards[0].classList.contains("card-user")).toBe(true);
    expect(cards[0].textContent).toContain("第一句");
  });

  it("给一个存在的 uuid ⇒ 找到那张卡 + 请求滚动到它 + 挂上 search-hit-flash", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), userLine(2, "u2", "第二句")], "u1");
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u1"]')!;
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    // 点名「滚给了谁」——不是「滚了几次」。this 就是那张卡。
    expect(scrollIntoView.mock.instances[0]).toBe(target);
    expect(scrollIntoView).toHaveBeenCalledWith({ block: "center" });
    expect(target.classList.contains("search-hit-flash")).toBe(true);
    // 走了定位这条路 ⇒ 不许再贴底（贴底会把用户从命中处弹走）
    expect(toBottom).not.toHaveBeenCalled();
  });

  it("双 rAF 之后幂等重发一次 scrollIntoView（content-visibility 估值几何那条修复）", async () => {
    const v = await mount([userLine(1, "u1", "第一句")], "u1");
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u1"]')!;
    expect(scrollIntoView).toHaveBeenCalledTimes(1); // 首发
    flushRaf(2);
    expect(scrollIntoView).toHaveBeenCalledTimes(2); // 双 rAF 后精确落点
    expect(scrollIntoView.mock.instances[1]).toBe(target);
  });

  it("给一个不存在的 uuid ⇒ 退到底部（不抛错、不静默什么都不做）", async () => {
    await mount([userLine(1, "u1", "第一句")], "没有这个 uuid");
    expect(scrollIntoView).not.toHaveBeenCalled();
    expect(toBottom).toHaveBeenCalledTimes(1); // 退到底部，正好一次
  });

  it("卡落在 <details> 里 ⇒ 祖先被展开", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), userLine(2, "u2", "第二句")]);
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u2"]')!;
    const det = document.createElement("details");
    target.parentElement!.insertBefore(det, target);
    det.appendChild(target);
    expect(det.open).toBe(false);

    toBottom.mockClear();
    jump(v, "u2");

    expect(det.open).toBe(true);
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(scrollIntoView.mock.instances[0]).toBe(target);
    expect(toBottom).not.toHaveBeenCalled();
  });

  // ★ 这一条钉的是一个**真修过的 bug**（session-viewer.ts 注释逐字：「此前只开 details，
  //   命中折叠段内的卡会被 0fr 裁剪、flash 不可见」）。ESC 回退段不是 `<details>`，
  //   是 `div.branch-fold-wrap` ⇒ 只钉 details 那一条会让这个 bug 悄悄回归。
  it("卡落在 ESC 回退段 div.branch-fold-wrap 里 ⇒ 也要被展开（不只 details）", async () => {
    const v = await mount([userLine(1, "u1", "第一句"), userLine(2, "u2", "第二句")]);
    const target = v.element.querySelector<HTMLElement>('[data-uuid="u2"]')!;
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    const header = document.createElement("div");
    header.className = "branch-fold-header";
    header.setAttribute("aria-expanded", "false");
    wrap.appendChild(header);
    target.parentElement!.insertBefore(wrap, target);
    wrap.appendChild(target);

    jump(v, "u2");

    expect(wrap.classList.contains("expanded")).toBe(true);
    expect(header.getAttribute("aria-expanded")).toBe("true");
    expect(scrollIntoView.mock.instances[0]).toBe(target);
  });
});
