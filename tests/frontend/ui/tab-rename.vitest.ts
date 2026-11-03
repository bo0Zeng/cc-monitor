/**
 * 标签页改名：名字住会话那台的注解（`history-annotate {customTitle}`，历史页「改标题」同一格），tab 上的名字等那台推帧来了才换。
 *
 * - 标题双击 · 右键「改标题」⇒ 标题那一格上浮出输入框（初值是此刻的名）：Enter 定 · Esc 取消 · 失焦定；
 * - 改成空 ⇒ 交空串（回到原标题）；与此刻的名相同 ⇒ 不交；
 * - 交成 ⇒ 可撤销的提示，撤销照回包的 `prev_title` 交回去（没有 ⇒ 空串）；
 * - 没有标签页栏的窗口 ⇒ 对话框。
 *
 * 夹具只造结构（sid · 占位名），不采会话正文。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import faces from "../../__fixtures__/session-face.golden.json";
import type { SessionFace } from "../../../src/frontend/ui/generated/SessionFace";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => {}), listen: vi.fn(async () => () => {}) }));
vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));
vi.mock("../../../src/frontend/ui/history-reads", () => ({ annotate: vi.fn(async () => ({ entry: {}, prev: null, prev_title: null })) }));
vi.mock("../../../src/frontend/ui/kit/dialog", async (orig) => ({ ...(await orig<object>()), askText: vi.fn(async () => null) }));
vi.mock("../../../src/frontend/ui/kit/toast", async (orig) => ({ ...(await orig<object>()), undoToast: vi.fn(() => () => {}), failToast: vi.fn(() => () => {}) }));

import { TabStore } from "../../../src/frontend/ui/tab-store";
import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { annotate } from "../../../src/frontend/ui/history-reads";
import { askText } from "../../../src/frontend/ui/kit/dialog";
import { undoToast } from "../../../src/frontend/ui/kit/toast";
import { renameTab } from "../../../src/frontend/ui/tab-rename";
import { TabMenu, type TabMenuHost } from "../../../src/frontend/ui/tab-menu";
import type { TabSessionActions } from "../../../src/frontend/ui/tab-session-actions";
import { NO_LINEAGE } from "../../../src/frontend/ui/session-hidden";

const LIVE_FACE = faces["live-idle"] as SessionFace;

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid,
    title: sid,
    background: false,
    bgName: null,
    label: `名-${sid}`,
    labelProject: null,
    projectDir: `/w/${sid}`,
    face: LIVE_FACE,
    line: null,
    pinned: false,
    group: null,
    hidden: null,
    lineage: NO_LINEAGE,
    origin: LOCAL,
    forkedFromSessionId: null,
    writers: [],
    unread: 0,
    needs: null,
    autoAnswer: null,
    autoAnswerChoices: [],
    autoAnswered: null,
    backgroundWork: null,
    pending: [],
    lastSay: null,
    touchedFiles: new Set(),
    usage: null,
    retries: new Map(),
    ...over,
  } as unknown as Tab;
}

function bar(tabs: Tab[]) {
  const store = new TabStore();
  for (const t of tabs) {
    store.tabs.set(t.sessionId, t);
    store.orderedIds.push(t.sessionId);
  }
  store.activeId = tabs[0]?.sessionId ?? null;
  const host = {
    refreshTabBar: vi.fn(),
    closeEndedIn: vi.fn(),
    openTabCwd: vi.fn().mockResolvedValue(undefined),
    bringTerminalToFront: vi.fn().mockResolvedValue(undefined),
    closeTab: vi.fn(),
    switchTo: vi.fn(),
    answerAt: vi.fn(),
    pick: vi.fn(),
    isSelected: vi.fn().mockReturnValue(false),
    beginDrag: vi.fn(),
    takeSuppressedClick: vi.fn().mockReturnValue(false),
    beginGroupDrag: vi.fn(),
    takeSuppressedHeadClick: vi.fn().mockReturnValue(false),
    openMenu: vi.fn(),
    toggleSelect: vi.fn(),
    openMenuAt: vi.fn(),
    rereadAll: vi.fn().mockResolvedValue(undefined),
    fixMachine: vi.fn(),
  } satisfies TabBarViewHost;
  const el = document.createElement("div");
  document.body.appendChild(el);
  const prefs = { collections: [], hiddenOpen: false, setHiddenOpen: vi.fn() } as unknown as TabBarPrefs;
  const view = new TabBarView(store, prefs, el, host);
  view.refresh();
  return { view, store, el };
}

const titleOf = (r: ReturnType<typeof bar>, sid: string): HTMLElement => r.view.tabButtons.get(sid)!.label;
const editor = (): HTMLInputElement | null => document.querySelector<HTMLInputElement>("body > input");
const key = (el: HTMLElement, k: string): void => void el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
const settle = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("标签页改名：写那台注解里的 customTitle", () => {
  it("★ 双击标题 ⇒ 输入框初值是此刻的名；Enter ⇒ 交会话那台 `customTitle`，给可撤销的提示，撤销照 prev_title 交回", async () => {
    const r = bar([tab("a"), tab("b", { origin: "box-1" as Tab["origin"] })]);
    // 名字那一格在标题格里靠右（前面是目录名）：框只盖名字那一格。
    titleOf(r, "b").getBoundingClientRect = () => ({ left: 48, top: 6, width: 90, height: 18, right: 138, bottom: 24, x: 48, y: 6, toJSON: () => ({}) });
    titleOf(r, "b").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    const input = editor();
    expect(input, "双击标题该浮出输入框").not.toBeNull();
    expect(input!.value).toBe("名-b");
    expect(input!.style.left, "框的左缘对齐名字那一格，不盖前面的目录名").toBe("48px");
    expect(input!.placeholder, "留空回到原标题，摆在占位里").toBe(copyText("history.rename.prompt"));
    vi.mocked(annotate).mockResolvedValueOnce({ entry: {}, prev: null, prev_title: "旧名" } as never);
    input!.value = "修登录";
    key(input!, "Enter");
    await settle();
    expect(annotate).toHaveBeenCalledWith("box-1", "b", { customTitle: "修登录" });
    expect(editor(), "定了就收起").toBeNull();
    expect(r.store.tabs.get("b")!.label, "名字等那台推帧来换，界面不先改").toBe("名-b");
    expect(undoToast).toHaveBeenCalledWith(copyText("history.rename.done"), expect.any(Function), expect.any(Function));
    vi.mocked(undoToast).mock.calls[0][1]();
    expect(annotate).toHaveBeenLastCalledWith("box-1", "b", { customTitle: "旧名" });
  });

  it("改成空 ⇒ 交空串（回到原标题）；撤销时改之前没有改过的名 ⇒ 交空串", async () => {
    const r = bar([tab("a")]);
    titleOf(r, "a").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    editor()!.value = "  ";
    key(editor()!, "Enter");
    await settle();
    expect(annotate).toHaveBeenCalledWith(LOCAL, "a", { customTitle: "" });
    vi.mocked(undoToast).mock.calls[0][1]();
    expect(annotate).toHaveBeenLastCalledWith(LOCAL, "a", { customTitle: "" });
  });

  it("Esc ⇒ 取消、不交；失焦 ⇒ 定；与此刻的名相同 ⇒ 不交", async () => {
    const r = bar([tab("a")]);
    titleOf(r, "a").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    editor()!.value = "不要了";
    key(editor()!, "Escape");
    await settle();
    expect(editor()).toBeNull();
    expect(annotate).not.toHaveBeenCalled();
    titleOf(r, "a").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    key(editor()!, "Enter");
    await settle();
    expect(annotate, "没改 ⇒ 不交").not.toHaveBeenCalled();
    titleOf(r, "a").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    editor()!.value = "失焦定";
    editor()!.dispatchEvent(new FocusEvent("blur"));
    await settle();
    expect(annotate).toHaveBeenCalledWith(LOCAL, "a", { customTitle: "失焦定" });
  });

  it("右键菜单「改标题」走同一个口（栏上就地改）", () => {
    const r = bar([tab("a")]);
    const host = {
      tab: (s: string) => r.store.tabs.get(s),
      collectionsLoaded: () => false,
      collections: () => [],
      groupAt: () => null,
      joinGroup: vi.fn(),
      foundGroup: vi.fn(),
      leaveGroup: vi.fn(),
      pinnedLoaded: () => false,
      togglePin: vi.fn(),
      close: vi.fn(),
      autoAnswer: vi.fn(),
      hide: vi.fn(),
      unhide: vi.fn(),
      openCwd: vi.fn(),
      front: vi.fn(),
      viewTerminal: vi.fn(),
    } satisfies TabMenuHost;
    new TabMenu(host, {} as TabSessionActions).open(new MouseEvent("contextmenu"), "a");
    const item = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((e) => e.textContent?.includes(copyText("history.menu.rename")));
    expect(item, "菜单里有「改标题」").toBeDefined();
    expect(item!.textContent, "同菜单别的项一样写键").toContain("F2");
    item!.click();
    expect(editor()?.value).toBe("名-a");
  });

  it("没有标签页栏的窗口 ⇒ 会话头标题上就地改（初值是此刻的名、不开对话框），交法相同", async () => {
    const head = document.createElement("span");
    head.dataset.headTitle = "";
    document.body.append(head);
    renameTab(tab("z"));
    const ed = editor()!;
    expect(ed.value).toBe("名-z");
    ed.value = "修登录";
    ed.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await settle();
    expect(askText).not.toHaveBeenCalled();
    expect(annotate).toHaveBeenCalledWith(LOCAL, "z", { customTitle: "修登录" });
    head.remove();
  });
});
