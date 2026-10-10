/**
 * 查看器「骨架 ＋ 按视口取」的判据（真 `SessionViewer` ＋ 真渲染管线，只有 IPC 是假的，台子见 rig 头注）。
 *
 * 夹具形状：1000 个可计行，偶数行是 user 记录（uuid `u{i}`、seq `2i`），奇数行不进界面（索引里没有 `t`，只占号）。
 *
 * 买到：正文只经骨架按偏移取（不整份读）· 首屏只取尾巴那一段（深链再取岛那一段）· 尾巴之上画出占位、深链岛与尾巴之间的缝也画
 * （洞 → seq 区间的换算含夹在中间的不可显示行）· 索引要不到 ⇒ 读不出（不退回整份读）·
 * 跳到占位里的一条 ⇒ 经骨架物化、取回那一段 · 取不到 ⇒ 占位放回去。
 * **买不到**：几何（jsdom 没布局 ⇒ `fillVisible` 在这里恒不动；那一半住 `tests/frontend/ui/skeleton-view.vitest.ts`）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  return rig.tauriCoreMock();
});
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { invoke } from "@tauri-apps/api/core";
import { recordReadCalls, sessionReadCalls } from "../../../test-support/chan-fake";
import {
  installViewerRig,
  expectLoaded,
  userLine,
  viewerRig,
  type RigPayload,
} from "../../../test-support/session-viewer-rig";
import { SessionViewer } from "../../../../src/frontend/ui/views/session-viewer";
import { SKELETON_GAP_CLASS, SkeletonView } from "../../../../src/frontend/ui/skeleton-view";
import { SkeletonLedger } from "../../../../src/frontend/ui/live-window";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

const ROWS = 1000;
/** payload：偶数 seq 的 user 记录（`u{i}` 在第 i 条可显示记录上） */
const payloads = (): RigPayload[] =>
  Array.from({ length: ROWS / 2 }, (_, i) =>
    userLine(2 * i, `u${i}`, `第 ${i} 句`),
  );
/** 索引：偶数行人说的（带正文字数，骨架判它建卡）、奇数行不进界面（没有 `t`，只占号）。 */
const index = () => ({
  available: true,
  from: 0,
  end: ROWS * 10,
  rows: Array.from({ length: ROWS }, (_, s) =>
    s % 2 === 0 ? { o: s * 10, n: 10, t: "said", u: `u${s / 2}`, ch: 10, pl: 1 } : { o: s * 10, n: 10 },
  ),
});
const ranges = () => recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_range");
/** 不带右端的 `history-page`（从某处读到末尾 ＝ 整份读那一形）。 */
const wholeReads = () => ranges().filter((r) => r.until === undefined);

const settle = () => new Promise((r) => setTimeout(r, 0));
const gaps = (root: HTMLElement) =>
  Array.from(root.querySelectorAll<HTMLElement>(`.${SKELETON_GAP_CLASS}`)).map((g) => [
    Number(g.dataset.skeletonLo),
    Number(g.dataset.skeletonHi),
  ]);

async function mount(scrollToUuid?: string): Promise<SessionViewer> {
  viewerRig.chunk = payloads();
  const v = new SessionViewer();
  document.body.appendChild(v.element);
  await v.load({
    jsonlPath: "/p/s1.jsonl",
    displayTitle: "T",
    origin: LOCAL_ORIGIN,
    suppressBranch: true,
    ...(scrollToUuid === undefined ? {} : { scrollToUuid }),
  });
  expectLoaded(v.element);
  await settle();
  return v;
}

beforeEach(() => {
  installViewerRig();
  vi.mocked(invoke).mockClear();
});
afterEach(() => vi.unstubAllGlobals());

describe("查看器：骨架 ＋ 按视口取（不整份读）", () => {
  it("★ 打开：先要骨架索引，正文只按偏移取尾巴那一段（150 条），尾巴之上画一块占位", async () => {
    viewerRig.index = index();
    const v = await mount();
    const idx = sessionReadCalls(vi.mocked(invoke).mock.calls, "read_session_index");
    expect(idx).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 0 }]);
    // 尾巴 = 第 350..499 条可显示记录 = seq 700..998：一段取回（夹在中间不进界面的那几行不把它切碎）
    expect(ranges()).toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", offset: 7000, until: 9990, seqBase: 700 }]);
    expect(wholeReads()).toEqual([]);
    expect(gaps(v.element)).toEqual([[0, 700]]);
    expect(v.element.querySelector('[data-id="u499"]')).not.toBeNull();
    expect(v.element.querySelector('[data-id="u349"]')).toBeNull();
  });

  // 同主窗口接骨架那一刀（`tabs.vitest.ts`「接骨架：账本先拿到折叠」）：占位插进去的时候账本就该按折叠算好高，
  // 不是插完、DOM 变动回调排完版再改高再钉视口（那一帧多排两次版）。
  it("★ 接骨架：账本先拿到折叠、再插占位", async () => {
    viewerRig.index = index();
    const order: string[] = [];
    const folds = vi.spyOn(SkeletonLedger.prototype, "setFolds").mockImplementation(() => {
      order.push("折叠");
      return false;
    });
    const attach = vi.spyOn(SkeletonView.prototype, "attachGaps").mockImplementation(() => {
      order.push("插占位");
      return 0;
    });
    try {
      await mount();
    } finally {
      folds.mockRestore();
      attach.mockRestore();
    }
    expect(order).toContain("插占位");
    expect(order[0], `接骨架的顺序是 ${order.join(" → ")}：占位插进去的时候账本还没拿到折叠`).toBe("折叠");
  });

  it("深链岛：岛那一段与尾巴各取一段；顶上没洞（岛从 0 起），岛与尾巴之间那条缝按 seq 画 —— 含夹在中间的不可显示行", async () => {
    viewerRig.index = index();
    const v = await mount("u100"); // 岛 = 第 0..199 条 = seq 0..398；尾巴从 seq 700
    expect(gaps(v.element)).toEqual([[399, 700]]);
    expect(ranges().map((r) => [r.offset, r.until])).toEqual(
      expect.arrayContaining([
        [7000, 9990],
        [0, 3990],
      ]),
    );
    expect(ranges().length).toBe(2);
  });

  it("🔴 索引要不到 ⇒ 读不出那一条（错误条 ＋ 重试），不退回整份读", async () => {
    viewerRig.index = { available: false, reason: "老后端", from: 0, end: 0, rows: [] };
    viewerRig.chunk = payloads();
    const v = new SessionViewer();
    document.body.appendChild(v.element);
    await v.load({ jsonlPath: "/p/s1.jsonl", displayTitle: "T", origin: LOCAL_ORIGIN, suppressBranch: true });
    expect(gaps(v.element)).toEqual([]);
    expect(v.element.querySelector("[data-id]")).toBeNull();
    expect(ranges()).toEqual([]);
    expect(wholeReads()).toEqual([]);
    expect(v.element.textContent).toContain("老后端");
  });

  it("跳到占位里的一条 ⇒ 经骨架物化：按偏移取回那一段、卡出来了、占位在它两边切开", async () => {
    viewerRig.index = index();
    const v = await mount();
    vi.mocked(invoke).mockClear();
    const el = await (v as unknown as { scrollToMessage(u: string): Promise<HTMLElement | null> | HTMLElement | null }).scrollToMessage("u150"); // seq 300
    expect(el?.dataset.id).toBe("u150");
    expect(ranges().length).toBe(1);
    const g = gaps(v.element);
    expect(g.length).toBe(2);
    expect(g[0][0]).toBe(0);
    expect(g[0][1]).toBeLessThanOrEqual(300);
    expect(g[1][0]).toBeGreaterThan(300);
    expect(g[1][1]).toBe(700);
  });

  it("那一段取不到 ⇒ 跳说清没取到、占位放回去（下次滚到再取），不留一段空", async () => {
    viewerRig.index = index();
    const v = await mount();
    viewerRig.failPage = true;
    const got = (v as unknown as { scrollToMessage(u: string): Promise<HTMLElement | null> | HTMLElement | null }).scrollToMessage("u150");
    await expect(Promise.resolve(got)).rejects.toThrow("devbox 连不上");
    expect(gaps(v.element)).toEqual([[0, 700]]);
  });
});
