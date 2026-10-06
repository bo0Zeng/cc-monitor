/**
 * 🔴 （第一刀 · 步 1）：**固定高度的加载态**。
 *
 * # 它钉的是什么
 *
 * 那两张相隔 3 秒的真机截图给出的病是**整页重排**：每一块在数据回来前是一行字
 *（「扫描中…」「加载中…」），回来变成一整张表 ⇒ 块长高 ⇒ 下面的东西全部往下掉，
 * 那 3 秒里点任何东西都会点在错的地方。
 *
 * ⇒ 修法是让**加载态与渲染态占同样高**，而这要求两处用**同一个数**：
 * 骨架自己的高度、与数据回来之后容器上钉着的那个高度下限。
 * 分开写两份，下一次改其中一处，重排就悄悄回来了 —— **而它不报错**。
 * ⇒ 住址只有 `src/frontend/ui/settings/skeleton.ts` 的 `SKELETON_PX` 一处，本条对着它做**两向集合相等**。
 *
 * # 反空真
 *
 * ① 登记表非空；② 屏幕上真扫到的那一批非空；③ **两边集合相等**（任一侧被清空、
 * 或有人新加一块骨架不进登记表，当场分叉）；④ 每一处的 `min-height` 与登记表**逐字相等**。
 *
 * 🔴 **〔射程 · 说清它盖不到什么〕**
 * - **不量真实排版**：jsdom 没有排版引擎 ⇒ 判据 #1 那条「加载期高度变化 = 0」
 *   要真机挂 `ResizeObserver` 才量得到，本条**不声称**它绿了。
 * - `min-height` 是**下限**不是等高：内容比它高时仍会长高。本条买的是
 *   「不会从一行字跳成一整屏」，不是像素级零位移。
 * - 那几块 per-machine 分节（账号 / MCP / 工具 …）本条没盖 —— 它们不在
 * 第一刀那三块的射程里。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    { get: (_t, name: string) => () => Promise.reject(new Error(`[stub] ${name}`)) },
  ),
}));
vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    headActions = (): HTMLElement[] => [];
    pageIdOfMachine = (): string | null => null;
    menuFor = (): unknown[] => [];
    metaOfPage = (): string | null => null;
    setConnected = (): void => {};
    originOfPage = (): string | null => null;
    isUnconfiguredPage = (): boolean => false;
    element = document.createElement("div");
    refresh = vi.fn().mockResolvedValue(undefined);
    constructor(opts?: {
      pages?: { addMachinePage: (id: string, t: string, el: HTMLElement) => void };
    }) {
      setTimeout(() => {
        opts?.pages?.addMachinePage(
          "machine:（本机）",
          "本机",
          document.createElement("div"),
        );
      }, 0);
    }
  },
}));
vi.mock("../../../../src/frontend/ui/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); },
}));
// `cc_integration.ts` 并进了 `machine-aliases.ts`（终端集成成了「别名」那一块 PowerShell 那一侧），它的替身随之删掉。
vi.mock("../../../../src/frontend/ui/keybindings/editor", () => ({
  KeybindingsEditor: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: vi.fn(),
    popOverlay: vi.fn(),
    startRecording: vi.fn(),
    cancelRecording: vi.fn(),
    exportOverrides: vi.fn().mockReturnValue({}),
    applyOverrides: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";
import {
  SKELETON_PX,
  makeSkeleton,
  holdSkeletonHeight,
  skeletonHeight,
  type SkeletonKey,
} from "../../../../src/frontend/ui/settings/skeleton";

const tick = () => new Promise((r) => setTimeout(r, 0));
const sorted = (xs: Iterable<string>) => [...new Set(xs)].sort();

/** 把面板整个走一遍（两个顶层页 + 本机子页；顶层「改动足迹」已删），让每一块都至少渲染过一次骨架。 */
async function openAllPages(): Promise<HTMLElement> {
  const p = new SettingsPanel({ windowMode: true });
  await p.open();
  await tick();
  for (const id of ["app", "app-appearance", "app-logs", "app-data", "machine:（本机）", "machines"]) {
    document.querySelector<HTMLButtonElement>(`[id="settings-tab-${id}"]`)?.click();
    await tick();
  }
  return document.querySelector<HTMLElement>(".settings-panel")!;
}

describe("：骨架与它替代的那块内容**同一个高度**（第一刀 · 步 1）", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostOsForTests("windows");
  });

  it("🔴 量具自检：登记表非空，且屏幕上真扫得到骨架", async () => {
    expect(Object.keys(SKELETON_PX).length, "登记表空了 ⇒ 下面的集合相等恒真").toBeGreaterThan(0);
    const root = await openAllPages();
    const seen = root.querySelectorAll("[data-skeleton], [data-skeleton-hold]");
    expect(seen.length, "一块骨架都没扫到 ⇒ 本文件全部断言都是空转").toBeGreaterThan(0);
  });

  it("🔴 登记表 == 屏幕上真出现的那一批（两向集合相等）", async () => {
    const root = await openAllPages();
    const onScreen = sorted([
      ...[...root.querySelectorAll<HTMLElement>("[data-skeleton]")].map(
        (e) => e.dataset.skeleton!,
      ),
      ...[...root.querySelectorAll<HTMLElement>("[data-skeleton-hold]")].map(
        (e) => e.dataset.skeletonHold!,
      ),
    ]);
    expect(
      onScreen,
      "骨架登记表（`skeleton.ts::SKELETON_PX`）与屏幕上真出现的那一批分叉了。\n" +
        "  多出来的：新加了一块骨架却没进登记表 ⇒ 它的高度没有第二处对得上；\n" +
        "  少了的：某一块的骨架被摘了（或那一页压根没渲染）—— 重排就从那儿回来。",
    ).toEqual(sorted(Object.keys(SKELETON_PX)));
  });

  it("🔴 每一处的 `min-height` 与登记表**逐字相等**（不是「差不多」）", async () => {
    const root = await openAllPages();
    const all = [
      ...root.querySelectorAll<HTMLElement>("[data-skeleton]"),
      ...root.querySelectorAll<HTMLElement>("[data-skeleton-hold]"),
    ];
    expect(all.length).toBeGreaterThan(0);
    for (const el of all) {
      const key = (el.dataset.skeleton ?? el.dataset.skeletonHold) as SkeletonKey;
      expect(el.style.minHeight, `${key} 这一处的高度与登记表对不上`).toBe(
        skeletonHeight(key),
      );
    }
  });

  it("🔴 骨架与它替代的容器是**同一个数**（这才是「不重排」的全部内容）", () => {
    // 不经过面板，直接把两条路各走一遍 —— 断的是「两处取的是同一个住址」。
    // 有人把其中一处改成字面量，这一条当场红。
    for (const key of Object.keys(SKELETON_PX) as SkeletonKey[]) {
      const sk = makeSkeleton(key, "载入中");
      const holder = document.createElement("div");
      holdSkeletonHeight(holder, key);
      expect(holder.style.minHeight, `${key}：容器与骨架不同高`).toBe(sk.style.minHeight);
    }
  });

  it("骨架不是空的，也不冒充兜底态（「没有第三态」那一格）", () => {
    const sk = makeSkeleton("footprint", "正在扫这台机器上的足迹…");
    expect(sk.textContent, "空骨架与「加载失败」在屏幕上分不开").not.toBe("");
    expect(sk.getAttribute("aria-busy"), "读屏器那一侧也要知道它在忙").toBe("true");
  });
});
