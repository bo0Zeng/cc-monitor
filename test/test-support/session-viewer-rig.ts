/**
 * K-R45 判据共用的**台子**（第二轮建，还的是第一轮申报的第 1 笔债；第三轮**扩了它**）。
 *
 * # 🔴 第三轮：住户从两个变成三个，名字没跟着改 —— 这一句是说明，不是借口
 *
 * 新住户是 `views/live-user-inputs.vitest.ts`（实时窗口那条路，被测对象是
 * `TabManager` 不是 `SessionViewer`）。它用的是本文件里**与 viewer 无关**的那几样：
 * 补 jsdom 缺的四样（`installViewerRig`）＋ 造记录行（`line` / `userLine` /
 * `assistantLine` / `withSession`）。真正 viewer-only 的只有 `tauriCoreMock`
 * （灌 `Channel`）与 `expectLoaded`（核 viewer 的状态栏），实时那侧一个都不用。
 * ⇒ **没有开第二份台子**（第二轮刚把两份并成一份，当轮长回两份是老债复发）。
 * 文件名没改是因为改名要动三个套件的 import 与两处文档住址，而收益只有「名字更准」；
 * **登记在这里，别当没看见**。
 *
 * # 为什么要有这一份
 *
 * `session-viewer-scroll.vitest.ts`（`KR45D0`）与 `session-viewer-user-inputs.vitest.ts`
 * （`KR45D1`）各带过一套**逐字重复**的桩：IPC（`@tauri-apps/api/core` 的 `invoke` /
 * `Channel`）· `ResizeObserver` · `CSS.escape` · `requestAnimationFrame` · `scrollIntoView`。
 * 上一轮 `src/test-support/` 在写区外，只能各写一份并把它**申报成债**。
 *
 * 债的形状不是「重复代码难看」，是：**两份一旦漂了，两个套件量的就不是同一个台子** ——
 * 而两边都以「我测的是真渲染管线」自居，谁漂了从输出面上看不出来。
 *
 * # 台子的保真边界（搬家不改这一段，只把它挪到一个家）
 *
 * - 渲染管线是**真的**：真 `cards/renderMessage` → 真 `render-stream-record` → 真
 *   `markCardUuid` 写 `data-uuid`。**没有** mock 掉「卡上有没有 `data-uuid`」这件事。
 * - **只有 IPC 那一层是假的**：`invoke` 换成「把 `viewerRig.chunk` 从 `Channel` 灌回去」。
 * - jsdom 没有 `ResizeObserver` / `scrollIntoView` / **`CSS`** ⇒ 这里补桩。
 *   `scrollIntoView` 的桩正是观测口：jsdom 无布局，「滚没滚到」量不了，
 *   量得了的只有**「请求发给了谁」**。
 * - 🔴 `SessionViewer.load()` 整段包在 `try/catch` 里 ⇒ 定位段抛任何错都会被**吞成状态栏
 *   一句「加载失败」**，判据只会看见「什么都没发生」。⇒ 每个用例挂载完先 `expectLoaded()`。
 *
 * # 刻意**没有**搬进来的
 *
 * `mount()`（两边给 `load()` 的参数不同）· `vi.mock()` 调用本身（它按调用文件的相对路径
 * 解析，且会被提升到文件顶）· 只有一边用的 DOM 查询助手。搬「其实不一样的东西」
 * 会逼出一个参数越加越多的壳，那是另一种漂。
 */
import { expect, vi } from "vitest";

/** 灌给 `invoke` 的那一整块 chunk。每个用例在 `mount()` 里塞，`installViewerRig()` 清空。 */
export const viewerRig: { chunk: unknown[] } = { chunk: [] };

/**
 * `@tauri-apps/api/core` 的替身。
 *
 * ⚠ 调用方必须走**异步动态 import** 的工厂：
 * ```ts
 * vi.mock("@tauri-apps/api/core", async () => (await import("./session-viewer-rig")).tauriCoreMock());
 * ```
 * `vi.mock` 会被提升到所有 import 之上，直接引用一个静态 import 进来的名字会踩 TDZ；
 * 异步工厂里的动态 import 在「被 mock 的模块第一次被 import」那一刻才求值，绕开这件事，
 * 而且拿到的是**同一个模块实例** ⇒ 两个套件共用同一个 `viewerRig.chunk`。
 */
export function tauriCoreMock(): Record<string, unknown> {
  return {
    Channel: class {
      onmessage: ((v: unknown) => void) | null = null;
    },
    invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "stream_read_session_jsonl") {
        const ch = args.onChunk as { onmessage?: ((v: unknown) => void) | null };
        ch.onmessage?.(viewerRig.chunk);
        return viewerRig.chunk.length;
      }
      return undefined;
    }),
  };
}

/** 后端喂给 `SessionViewer` 的一行（`JsonlLinePayload` 的最小形状）。 */
export interface RigPayload {
  session_id: string;
  cwd: null;
  path: string;
  seq: number;
  message: unknown;
}

/** 裸一行：`message` 由调用方整块给（要造 assistant / 畸形记录时用）。 */
export function line(seq: number, message: Record<string, unknown>): RigPayload {
  return { session_id: "s1", cwd: null, path: "/p/s1.jsonl", seq, message };
}

/**
 * 一条 user 记录。`over` 覆盖任意字段（`isMeta` / `isSidechain` / `parentUuid` …）——
 * 判据要造的反例全靠它，别为每种反例再开一个构造器。
 */
export function userLine(
  seq: number,
  uuid: string | null,
  content: unknown,
  over: Record<string, unknown> = {},
): RigPayload {
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

/**
 * 把一行改挂到另一个会话上（多 tab 的判据要造两个 sid；`line()` 把 `s1` 写死了）。
 * `path` 跟着换 —— 两个 tab 的 `parentPath` 相同在真实里不会发生，别让夹具自带一处假。
 */
export function withSession(p: RigPayload, sessionId: string): RigPayload {
  return { ...p, session_id: sessionId, path: `/p/${sessionId}.jsonl` };
}

/** 一条 assistant 记录（清单口径里它必须被排掉，判据要拿它当反例）。 */
export function assistantLine(seq: number, uuid: string, text: string): RigPayload {
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

export interface ViewerRigHandles {
  /** `scrollIntoView` 的桩 —— 判据的观测口（「请求滚动到谁」）。 */
  scrollIntoView: ReturnType<typeof vi.fn>;
  /**
   * 手动跑 rAF 队列。不叫它，排进去的回调**永远不跑** ——
   * 「双 rAF 幂等重发」那一格靠它才走得到；不量那一格的套件不叫它即可。
   */
  flushRaf(rounds: number): void;
}

/**
 * 装台子：清 DOM、清夹具、补齐 jsdom 缺的四样。**每个用例 `beforeEach` 调一次**，
 * 收尾用 `vi.unstubAllGlobals()`。
 */
export function installViewerRig(): ViewerRigHandles {
  document.body.replaceChildren();
  viewerRig.chunk = [];
  let queue: FrameRequestCallback[] = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => queue.push(cb));
  // jsdom 没有全局 `CSS`（真浏览器里有）⇒ 补一份**只做转义、不做别的**的桩。
  // 不补的话 `scrollToMessage` 第一行选择器就抛，而 `load()` 会把它吞掉。
  vi.stubGlobal("CSS", { escape: (s: string) => s.replace(/["\\]/g, "\\$&") });
  const scrollIntoView = vi.fn();
  Element.prototype.scrollIntoView = scrollIntoView as unknown as Element["scrollIntoView"];
  return {
    scrollIntoView,
    flushRaf(rounds: number): void {
      for (let i = 0; i < rounds; i++) {
        const due = queue;
        queue = [];
        for (const cb of due) cb(0);
      }
    },
  };
}

/**
 * 🔴 `load()` 会把定位段抛出的异常吞成状态栏一句「加载失败：…」。
 * 不核这一句，一切「调了 0 次」的读数都分不清是「没调」还是「炸了」。
 */
export function expectLoaded(root: HTMLElement): void {
  const status = root.querySelector(".history-status")?.textContent ?? "";
  expect(status).not.toContain("加载失败");
}
