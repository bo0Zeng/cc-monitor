/**
 * 批量菜单：每一项「动作（能做的个数）」· 只把能做的交出去（个数按单个菜单亮那一项的同一个谓词数）· 杀之前确认、确认框列出这一批 ·
 * 做完一条提示（做了 · 跳过、各为什么 · 失败、各为什么）· 固定 / 集合一次改完。后端那两件与宿主都是替身。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), recentToasts: vi.fn(() => [{ title: "最新一条" }]) }));
vi.mock("../../../src/frontend/ui/status-messages", () => ({ showMessage: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/dialog", () => ({ confirmDialog: vi.fn(), askText: vi.fn() }));

import { openBatchMenu, type TabBatchHost, type TabBatchRun } from "../../../src/frontend/ui/tab-batch-menu";
import { toast as showActionFailureToast } from "../../../src/frontend/ui/kit/toast";
import { askText } from "../../../src/frontend/ui/kit/dialog";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { showMessage } from "../../../src/frontend/ui/status-messages";
import { LIVE, ENDED, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import { LOCAL_ORIGIN, originFromWire } from "../../../src/frontend/ui/ipc/origin";
import type { Tab } from "../../../src/frontend/ui/tab-model";

const tab = (sid: string, live: boolean, extra: Partial<Tab> = {}): Tab =>
  ({ sessionId: sid, origin: LOCAL_ORIGIN, title: `T-${sid}`, aiTitle: `T-${sid}`, projectDir: null, kind: null, bgName: null, forkedFromSessionId: null, state: live ? LIVE : ENDED, pinned: false, group: null, ...extra }) as Tab;

let tabs: Tab[];
let host: TabBatchHost;
const calls = (f: unknown): unknown[][] => vi.mocked(f as () => void).mock.calls;
let run: TabBatchRun & { stop: ReturnType<typeof vi.fn>; start: ReturnType<typeof vi.fn>; confirm: ReturnType<typeof vi.fn> };

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const buttons = (): HTMLButtonElement[] => [...document.body.querySelectorAll<HTMLButtonElement>("[role^=menuitem]")];
const labelOf = (b: Element): string => b.querySelector("[data-part=label]")?.textContent ?? "";
const press = async (label: string): Promise<void> => {
  const b = buttons().find((x) => labelOf(x) === label);
  expect(b, `菜单里没有「${label}」：${buttons().map(labelOf).join(" | ")}`).toBeDefined();
  b!.click();
  await flush();
  await flush();
};
const open = (): void => openBatchMenu(new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }), tabs.map((t) => t.sessionId), host, run);
/** 逐条原因：不上 toast 正文，只进记录（`more`），在「消息」里展开看。 */
const toastBody = (): string => {
  const c = vi.mocked(showActionFailureToast).mock.calls.at(-1);
  expect(c?.[1], "toast 正文只留汇总一句").toBe("");
  return ((c?.[2] as { more?: string[] } | undefined)?.more ?? []).join("\n");
};
const toastHead = (): string => (vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[0] ?? "") as string;

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
    expect(document.body.querySelector("[role=menu] [role=presentation]")?.textContent, "头一行「已选 N 个」").toBe(copyText("tabBatch.menu.head", { n: 3 }));
    const want = [
      copyText("tabBatch.menu.stop", { n: 2 }),
      copyText("tabBatch.menu.startTmux", { n: 1 }),
      copyText("tabBatch.menu.startWindow", { n: 1 }),
      copyText("tabBatch.menu.pin", { n: 2 }),
      copyText("tabBatch.menu.unpin", { n: 1 }),
      copyText("tabBatch.menu.leave", { n: 1 }),
      copyText("tabBatch.menu.close", { n: 1 }),
    ];
    const got = buttons().map(labelOf);
    for (const w of want) expect(got).toContain(w);
    tabs = [tab("a", true), tab("c", true)];
    open();
    const close = buttons().find((b) => labelOf(b) === copyText("tabBatch.menu.close", { n: 0 }));
    expect(close?.disabled, "一个都关不了 ⇒ 置灰").toBe(true);
    expect(close?.querySelector("[data-part=why]")?.textContent, "灰着的第二行说为什么").toBe(copyText("tabBatch.why.live"));
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
    const spec = run.confirm.mock.calls[0][0] as { list: string[]; note?: string; danger?: boolean; action: string };
    expect(spec.list, "清单只列这一批里还在跑的（本机的不写机器）").toEqual(["T-a", "T-c"]);
    expect(spec.note).toBe(copyText("sessionState.batch.skipped", { n: 1 }));
    expect(spec.danger, "批量杀是撤不回的：确认框默认焦点要在「取消」").toBe(true);
    expect(spec.action, "批量确认的数量写在按钮上").toBe(copyText("tabBatch.stop.action", { n: 2 }));
    expect(run.stop.mock.calls[0][0].map((t: Tab) => t.sessionId)).toEqual(["a", "c"]);
    expect(toastHead()).toBe(copyText("sessionState.batch.someFailed", { done: 1, failed: 1 }));
    expect(toastBody().split("\n")).toEqual([
      copyText("tabBatch.result.failedLine", { title: "T-c", why: "门拦下了" }),
      copyText("tabBatch.result.skippedLine", { title: "T-b", why: copyText("tabBatch.why.ended") }),
    ]);
    const opts = vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[2] as { action?: { label: string; run: () => void } };
    expect(opts.action?.label, "有逐条原因 ⇒ 带［查看］").toBe(copyText("tabBatch.result.view"));
    opts.action!.run();
    expect(showMessage, "［查看］打开「消息」并展开刚才那一条").toHaveBeenCalledWith({ title: "最新一条" });
    run.confirm.mockResolvedValueOnce(false);
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    expect(run.stop, "确认框点了取消 ⇒ 一个都不杀").toHaveBeenCalledTimes(1);
  });

  it("全都做成了 ⇒ 一句汇总，不带［查看］", async () => {
    run.stop.mockResolvedValue([
      { sid: "a", outcome: "done", why: "" },
      { sid: "c", outcome: "done", why: "" },
    ]);
    tabs = [tab("a", true), tab("c", true)];
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    expect(toastBody()).toBe("");
    expect((vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[2] as { action?: unknown }).action).toBeUndefined();
  });

  it("远端那几行机器只出一次：标题不带 `[机器]` 前缀，机器写在括号里", async () => {
    const devbox = originFromWire("devbox");
    tabs = [
      tab("r", true, { origin: devbox, title: "[devbox] [billing] 账单导出改成流式", projectDir: "/w/billing", aiTitle: "账单导出改成流式" }),
      tab("l", true, { title: "[web] 首页", projectDir: "/w/web", aiTitle: "首页" }),
    ];
    run.stop.mockResolvedValue([
      { sid: "r", outcome: "failed", why: "门拦下了" },
      { sid: "l", outcome: "done", why: "" },
    ]);
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    const spec = run.confirm.mock.calls[0][0] as { list: string[] };
    expect(spec.list).toEqual([copyText("tabBatch.stop.line", { title: "billing 账单导出改成流式", machine: "devbox" }), "web 首页"]);
    expect(toastBody().split("\n")).toEqual([copyText("tabBatch.result.failedLine", { title: "billing 账单导出改成流式", why: "门拦下了" })]);
  });

  it("B3 · 固定 / 集合：一次交出能做的那几个；新建集合整批进同一个新组", async () => {
    open();
    await press(copyText("tabBatch.menu.pin", { n: 2 }));
    expect(calls(host.setPinned)).toEqual([[["a", "c"], true]]);
    expect(toastHead()).toBe(copyText("tabBatch.result.done", { action: copyText("tabBatch.action.pin"), done: 2 }));
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
