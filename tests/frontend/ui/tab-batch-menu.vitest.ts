/**
 * 批量菜单：每一项「动作（能做的个数）」· 只把能做的交出去（个数按单个菜单亮那一项的同一个谓词数）· 杀之前确认、确认框列出这一批 ·
 * 做完一条提示（做了 · 跳过、各为什么 · 失败、各为什么）· 固定 / 集合一次改完。后端那两件与宿主都是替身。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/ask-dialog", () => ({ askConfirm: vi.fn(), askText: vi.fn() }));

import { openBatchMenu, type TabBatchHost, type TabBatchRun } from "../../../src/frontend/ui/tab-batch-menu";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { askText } from "../../../src/frontend/ui/ask-dialog";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { LIVE, ENDED, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import type { Tab } from "../../../src/frontend/ui/tab-model";

const tab = (sid: string, live: boolean, extra: Partial<Tab> = {}): Tab =>
  ({ sessionId: sid, origin: LOCAL_ORIGIN, title: `T-${sid}`, state: live ? LIVE : ENDED, pinned: false, group: null, ...extra }) as Tab;

let tabs: Tab[];
let host: TabBatchHost;
const calls = (f: unknown): unknown[][] => vi.mocked(f as () => void).mock.calls;
let run: TabBatchRun & { stop: ReturnType<typeof vi.fn>; start: ReturnType<typeof vi.fn>; confirm: ReturnType<typeof vi.fn> };

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const buttons = (): HTMLButtonElement[] => [...document.body.querySelectorAll<HTMLButtonElement>(".tab-context-menu-item")];
const press = async (label: string): Promise<void> => {
  const b = buttons().find((x) => x.textContent === label);
  expect(b, `菜单里没有「${label}」：${buttons().map((x) => x.textContent).join(" | ")}`).toBeDefined();
  b!.click();
  await flush();
  await flush();
};
const open = (): void => openBatchMenu(new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }), tabs.map((t) => t.sessionId), host, run);
const toastBody = (): string => (vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[1] ?? "") as string;

beforeEach(() => {
  vi.clearAllMocks();
  document.body.innerHTML = "";
  tabs = [tab("a", true), tab("b", false, { pinned: true }), tab("c", true, { group: "g" })];
  host = {
    tab: vi.fn((sid: string) => tabs.find((t) => t.sessionId === sid)),
    collectionsLoaded: vi.fn(() => true),
    collections: vi.fn(() => [{ id: "g", name: "白天" }]),
    pinnedLoaded: vi.fn(() => true),
    setPinned: vi.fn(),
    joinGroup: vi.fn(),
    foundGroup: vi.fn(() => null),
    leaveGroup: vi.fn(),
    closeTabs: vi.fn(),
  } as never;
  run = { stop: vi.fn(), start: vi.fn(), confirm: vi.fn(async () => true) } as never;
});

describe("批量菜单", () => {
  it("B1 · 每一项写能做的个数；为 0 的置灰", () => {
    open();
    const want = [
      copyText("tabBatch.menu.head", { n: 3 }),
      copyText("tabBatch.menu.stop", { n: 2 }),
      copyText("tabBatch.menu.startTmux", { n: 1 }),
      copyText("tabBatch.menu.startWindow", { n: 1 }),
      copyText("tabBatch.menu.pin", { n: 2 }),
      copyText("tabBatch.menu.unpin", { n: 1 }),
      copyText("tabBatch.menu.leave", { n: 1 }),
      copyText("tabBatch.menu.close", { n: 1 }),
    ];
    const got = buttons().map((b) => b.textContent);
    for (const w of want) expect(got).toContain(w);
    tabs = [tab("a", true), tab("c", true)];
    open();
    const close = buttons().find((b) => b.textContent === copyText("tabBatch.menu.close", { n: 0 }));
    expect(close?.disabled, "一个都关不了 ⇒ 置灰").toBe(true);
  });

  it("说不清（那台暂时看不见）的不算可起、也不说「已经结束了」：理由说看不见", async () => {
    tabs = [tab("a", false), tab("u", false, { state: UNSEEN })];
    run.start.mockResolvedValue([{ sid: "a", outcome: "done", why: "" }]);
    open();
    await press(copyText("tabBatch.menu.startTmux", { n: 1 }));
    expect(run.start.mock.calls[0][0].map((t: Tab) => t.sessionId)).toEqual(["a"]);
    const unseen = copyText("tabBatch.result.skippedLine", { title: "T-u", why: copyText("sessionState.unseen.tooltip") });
    expect(toastBody().split("\n")).toContain(unseen);
  });

  it("B2 · 杀：确认框列出这一批，只交还在跑的；结果一条：做了 · 跳过（为什么）· 失败（为什么）", async () => {
    run.stop.mockResolvedValue([
      { sid: "a", outcome: "done", why: "" },
      { sid: "c", outcome: "failed", why: "门拦下了" },
    ]);
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    const asked = run.confirm.mock.calls[0][0] as string;
    expect(asked).toContain(copyText("tabBatch.stop.line", { title: "T-a", machine: "本机" }));
    expect(asked).toContain(copyText("tabBatch.stop.line", { title: "T-c", machine: "本机" }));
    expect(asked).not.toContain("T-b");
    expect(run.confirm.mock.calls[0][1], "批量杀是撤不回的：确认框默认焦点要在「取消」").toEqual({ danger: true });
    expect(run.stop.mock.calls[0][0].map((t: Tab) => t.sessionId)).toEqual(["a", "c"]);
    expect(toastBody().split("\n")).toEqual([
      copyText("tabBatch.result.counts", { done: 1, skipped: 1, failed: 1 }),
      copyText("tabBatch.result.skippedLine", { title: "T-b", why: copyText("tabBatch.why.ended") }),
      copyText("tabBatch.result.failedLine", { title: "T-c", why: "门拦下了" }),
    ]);
    run.confirm.mockResolvedValueOnce(false);
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    expect(run.stop, "确认框点了取消 ⇒ 一个都不杀").toHaveBeenCalledTimes(1);
  });

  it("B3 · 固定 / 集合：一次交出能做的那几个；新建集合整批进同一个新组", async () => {
    open();
    await press(copyText("tabBatch.menu.pin", { n: 2 }));
    expect(calls(host.setPinned)).toEqual([[["a", "c"], true]]);
    expect(toastBody().split("\n")[0]).toBe(copyText("tabBatch.result.counts", { done: 2, skipped: 1, failed: 0 }));
    open();
    await press(copyText("tabBatch.menu.leave", { n: 1 }));
    expect(calls(host.leaveGroup)).toEqual([[["c"]]]);
    vi.mocked(askText).mockResolvedValue("新组");
    open();
    await press(copyText("tabBatch.menu.found", { n: 3 }));
    expect(calls(host.foundGroup).length).toBe(1);
    expect(calls(host.foundGroup)[0].slice(0, 2)).toEqual([["a", "b", "c"], "新组"]);
  });
});
