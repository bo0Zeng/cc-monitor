/**
 * 〔U3b〕查看器接骨架的判据（真 `SessionViewer` ＋ 真渲染管线，只有 IPC 是假的，台子见 rig 头注）。
 *
 * 夹具形状：1000 个可计行，偶数行是 user 记录（uuid `u{i}`、seq `2i`），奇数行是不可显示的
 * `permission-mode`（占号、不出 payload）—— 正是子步 1 那次改编号要对上的形状。
 *
 * 买到：索引与正文**并行**要（本机 origin 逐字 `<local>`）· 尾巴之上画出占位、深链岛与尾巴之间的缝也画
 * （洞 → seq 区间的换算含夹在中间的不可显示行）· seq 对不上 / 索引不可用 ⇒ 不接、行为不变 ·
 * 跳到占位里的一条 ⇒ 经骨架物化（占位切开）而不是凭空插一段。
 * **买不到**：几何（jsdom 没布局 ⇒ `fillVisible` 在这里恒不动；那一半住 `tests/skeleton-view.vitest.ts`）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../test-support/session-viewer-rig");
  return rig.tauriCoreMock();
});
vi.mock("../../src/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { invoke } from "@tauri-apps/api/core";
import {
  installViewerRig,
  expectLoaded,
  userLine,
  viewerRig,
  type RigPayload,
} from "../test-support/session-viewer-rig";
import { SessionViewer } from "../../src/views/session-viewer";
import { SKELETON_GAP_CLASS } from "../../src/skeleton-view";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";

const ROWS = 1000;
/** payload：偶数 seq 的 user 记录（`u{i}` 在第 i 条可显示记录上） */
const payloads = (): RigPayload[] =>
  Array.from({ length: ROWS / 2 }, (_, i) =>
    userLine(2 * i, `u${i}`, `第 ${i} 句`, { parentUuid: i > 0 ? `u${i - 1}` : null }),
  );
/** 索引：`shift` ≠ 0 模拟「seq 空间对不上」 */
const index = (shift = 0) => ({
  available: true,
  from: 0,
  end: ROWS * 10,
  rows: Array.from({ length: ROWS }, (_, s) =>
    s % 2 === 0
      ? { o: s * 10, n: 10, t: "user", u: `u${s / 2 + shift}`, ch: 10, pl: 1 }
      : { o: s * 10, n: 10, t: "permission-mode" },
  ),
});

const settle = () => new Promise((r) => setTimeout(r, 0));
const gaps = (root: HTMLElement) =>
  Array.from(root.querySelectorAll<HTMLElement>(`.${SKELETON_GAP_CLASS}`)).map((g) => [
    Number(g.dataset.skeletonLo),
    Number(g.dataset.skeletonHi),
  ]);

async function mount(scrollToUuid?: string): Promise<SessionViewer> {
  viewerRig.chunk = payloads();
  const v = new SessionViewer(() => {});
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

describe("〔U3b〕查看器接骨架", () => {
  it("索引与正文并行要；尾巴 150 条之上画一块占位 [0, 尾巴第一条的 seq)", async () => {
    viewerRig.index = index();
    const v = await mount();
    const calls = vi.mocked(invoke).mock.calls.map((c) => c[0]);
    expect(calls).toContain("read_session_index");
    expect(vi.mocked(invoke).mock.calls.find((c) => c[0] === "read_session_index")?.[1]).toEqual({
      origin: "<local>",
      jsonlPath: "/p/s1.jsonl",
      fromOffset: 0,
    });
    // 尾巴 = 第 350..499 条可显示记录 = seq 700..998
    expect(gaps(v.element)).toEqual([[0, 700]]);
  });

  it("深链岛：顶上没洞（岛从 0 起），岛与尾巴之间那条缝按 seq 画 —— 含夹在中间的不可显示行", async () => {
    viewerRig.index = index();
    const v = await mount("u100"); // 岛 = 第 0..199 条 = seq 0..398；尾巴从 seq 700
    expect(gaps(v.element)).toEqual([[399, 700]]);
  });

  it("🔴 seq 空间对不上 ⇒ 不接（不许硬对）", async () => {
    viewerRig.index = index(3);
    const v = await mount();
    expect(gaps(v.element)).toEqual([]);
  });

  it("索引不可用（本机后端不在 / 老后端）⇒ 不接，行为与之前一样", async () => {
    viewerRig.index = { available: false, reason: "老后端", from: 0, end: 0, rows: [] };
    const v = await mount();
    expect(gaps(v.element)).toEqual([]);
    expect(v.element.querySelector('[data-uuid="u10"]')).toBeNull();
  });

  it("跳到占位里的一条 ⇒ 经骨架物化：卡出来了、占位在它两边切开", async () => {
    viewerRig.index = index();
    const v = await mount();
    (v as unknown as { scrollToMessage(u: string): void }).scrollToMessage("u150"); // seq 300
    expect(v.element.querySelector('[data-uuid="u150"]')).not.toBeNull();
    const g = gaps(v.element);
    expect(g.length).toBe(2);
    expect(g[0][0]).toBe(0);
    expect(g[0][1]).toBeLessThanOrEqual(300);
    expect(g[1][0]).toBeGreaterThan(300);
    expect(g[1][1]).toBe(700);
  });
});
