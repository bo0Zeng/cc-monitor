/**
 * 批量菜单：每一项「动作（能做的个数）」（个数按单个菜单亮那一项的同一个谓词数）· 杀之前确认、确认框列出这一批 ·
 * 结束 · 恢复 · 固定交本机后端一条、结局照回包摆（跳不跳过 · 那一句都是后端的）· 分组 · 关闭只经 monitor、不另出结果条。后端那几件与宿主都是替身。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), failToast: vi.fn(), recentToasts: vi.fn(() => [{ title: "最新一条" }]) }));
vi.mock("../../../src/frontend/ui/status-messages", () => ({ showMessage: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/dialog", () => ({ confirmDialog: vi.fn(), askText: vi.fn() }));

import { openBatchMenu, type TabBatchHost, type TabBatchRun } from "../../../src/frontend/ui/tab-batch-menu";
import { failToast, toast as showActionFailureToast } from "../../../src/frontend/ui/kit/toast";
import { askText } from "../../../src/frontend/ui/kit/dialog";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { endConfirm } from "../../../src/frontend/ui/end-confirm";
import { showMessage } from "../../../src/frontend/ui/status-messages";
import { goldenFace, liveFace } from "../../test-support/session-faces";
import { LOCAL, type Machine } from "../../../src/frontend/ui/ipc/origin";
import type { Tab } from "../../../src/frontend/ui/tab-model";

/** 活着、在 tmux 里干活的那一张（结束得了，同 `SessionFace::of` 对住在 tmux 里的那一判）。 */
const inTmux = () => {
  const f = liveFace("working", true);
  return { ...f, can: { ...f.can, end: { can: "yes" as const } } };
};
const tab = (sid: string, live: boolean, extra: Partial<Tab> = {}): Tab =>
  ({ sessionId: sid, origin: LOCAL, title: `T-${sid}`, label: `T-${sid}`, labelProject: null, projectDir: null, background: false, bgName: null, forkedFromSessionId: null, face: live ? inTmux() : goldenFace("ended"), pinned: false, group: null, ...extra }) as Tab;

let tabs: Tab[];
let host: TabBatchHost;
const calls = (f: unknown): unknown[][] => vi.mocked(f as () => void).mock.calls;
let run: TabBatchRun & { stop: ReturnType<typeof vi.fn>; start: ReturnType<typeof vi.fn>; pin: ReturnType<typeof vi.fn>; confirm: ReturnType<typeof vi.fn> };

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
    landPinned: vi.fn(),
    joinGroup: vi.fn(),
    foundGroup: vi.fn(() => null),
    leaveGroup: vi.fn(),
    closeTabs: vi.fn(),
  } as never;
  run = { stop: vi.fn(), start: vi.fn(), pin: vi.fn(), confirm: vi.fn(async () => true) } as never;
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

  const reply = (over: Record<string, unknown> = {}) => ({ results: [], said: "S-合好", failed: true, lines: ["L-一"], copyDetail: "D-详", windows: [], ...over });

  it("★ 恢复：说不清（那台暂时看不见）的不算可起、灰着的理由说看不见；交出去的是选中的全部，结局照回包摆", async () => {
    tabs = [tab("a", false), tab("u", false, { face: goldenFace("unseen") })];
    run.start.mockResolvedValue(reply());
    open();
    await press(copyText("tabBatch.menu.startTmux", { n: 1 }));
    expect(run.start.mock.calls[0][0].map((t: Tab) => t.sessionId), "跳不跳过由后端判：选中的全交").toEqual(["a", "u"]);
    expect(run.start.mock.calls[0][1]).toBe("tmux");
    expect(toastHead()).toBe("S-合好");
    expect(toastBody()).toBe("L-一");
    tabs = [tab("u", false, { face: goldenFace("unseen") }), tab("v", false, { face: goldenFace("unseen") })];
    open();
    const b = buttons().find((x) => labelOf(x) === copyText("tabBatch.menu.startWindow", { n: 0 }));
    expect(b?.querySelector("[data-part=why]")?.textContent).toBe(copyText("sessionState.unseen.noResume"));
  });

  it("恢复：本机后端问不到 ⇒ 一条失败提示（标题是那一项）", async () => {
    tabs = [tab("a", false), tab("b2", false)];
    run.start.mockRejectedValue(new Error("gone"));
    open();
    await press(copyText("tabBatch.menu.startWindow", { n: 2 }));
    expect(vi.mocked(failToast).mock.calls[0][0]).toBe(copyText("tabBatch.menu.startWindow", { n: 2 }));
  });

  it("★ 杀：能做的照脸上 can.end 数；确认框只列结束得了的（结束会话那一个）；交出去的是选中的全部，结局照回包摆", async () => {
    const ended = { ...goldenFace("ended"), can: { ...goldenFace("ended").can, end: { can: "no" as const, why: "核心·e" } } };
    tabs = [tab("e1", false, { face: ended }), tab("r", true)];
    run.stop.mockResolvedValue(reply());
    open();
    await press(copyText("tabBatch.menu.stop", { n: 1 }));
    expect(run.confirm.mock.calls[0][0]).toEqual(endConfirm(["T-r"]));
    expect(run.stop.mock.calls[0][0].map((t: Tab) => t.sessionId), "跳不跳过由后端判：选中的全交").toEqual(["e1", "r"]);
    expect(toastHead()).toBe("S-合好");
    expect(toastBody()).toBe("L-一");
    const opts = vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[2] as { level?: string; detail?: string; action?: { label: string; run: () => void; toastOnly?: boolean } };
    expect(opts.level).toBe("error");
    expect(opts.detail).toBe("D-详");
    expect(opts.action?.label, "有逐条 ⇒ 带［查看］").toBe(copyText("tabBatch.result.view"));
    expect(opts.action?.toastOnly).toBe(true);
    opts.action!.run();
    expect(showMessage, "［查看］打开「消息」并展开刚才那一条").toHaveBeenCalledWith({ title: "最新一条" });
  });

  it("确认框点了取消 ⇒ 一个都不交", async () => {
    run.confirm.mockResolvedValueOnce(false);
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    expect(run.stop).not.toHaveBeenCalled();
  });

  it("全都做成了 ⇒ 一句汇总，不带［查看］", async () => {
    run.stop.mockResolvedValue(reply({ failed: false, lines: [], copyDetail: "" }));
    tabs = [tab("a", true), tab("c", true)];
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    expect(toastBody()).toBe("");
    expect((vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[2] as { action?: unknown }).action).toBeUndefined();
  });

  it("本机后端问不到 ⇒ 一条失败提示（那一句照通道那一层）", async () => {
    run.stop.mockRejectedValue(new Error("gone"));
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    expect(vi.mocked(failToast)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(failToast).mock.calls[0][0]).toBe(copyText("tabBatch.stop.title", { n: 3 }));
  });

  it("远端那几行机器只出一次：确认框里标题不带 `[机器]` 前缀，机器写在括号里", async () => {
    const devbox: Machine = "devbox";
    tabs = [
      tab("r", true, { origin: devbox, title: "[devbox] [billing] 账单导出改成流式", projectDir: "/w/billing", label: "账单导出改成流式", labelProject: "billing" }),
      tab("l", true, { title: "[web] 首页", projectDir: "/w/web", label: "首页", labelProject: "web" }),
    ];
    run.stop.mockResolvedValue(reply());
    open();
    await press(copyText("tabBatch.menu.stop", { n: 2 }));
    const spec = run.confirm.mock.calls[0][0] as { list: string[] };
    expect(spec.list).toEqual([copyText("tabBatch.line.remote", { title: "billing 账单导出改成流式", machine: "devbox" }), "web 首页"]);
  });

  it("★ B3 · 固定：交出去的是选中的全部；回包里成了的那几个才翻（挑格，不先翻、不数）；toast 照回包那一句", async () => {
    run.pin.mockResolvedValue(reply({ results: [{ sid: "a", outcome: "done" }, { sid: "b", outcome: "skipped" }, { sid: "c", outcome: "failed" }] }));
    open();
    await press(copyText("tabBatch.menu.pin", { n: 2 }));
    expect(run.pin.mock.calls.map(([ts, on]) => [ts.map((t: Tab) => t.sessionId), on])).toEqual([[["a", "b", "c"], true]]);
    expect(calls(host.landPinned), "那台拒了的那一项不翻").toEqual([[["a"], true]]);
    expect(toastHead()).toBe("S-合好");
    run.pin.mockRejectedValue(new Error("gone"));
    open();
    await press(copyText("tabBatch.menu.unpin", { n: 1 }));
    expect(run.pin.mock.calls.at(-1)?.[1]).toBe(false);
    expect(vi.mocked(failToast).mock.calls.at(-1)?.[0]).toBe(copyText("tabBatch.menu.unpin", { n: 1 }));
    expect(calls(host.landPinned)).toHaveLength(1);
  });

  it("★ 关闭：只关关得了的那几个，不另出结果条", async () => {
    open();
    await press(copyText("tabBatch.menu.close", { n: 1 }));
    expect(calls(host.closeTabs)).toEqual([[["b"]]]);
    expect(showActionFailureToast).not.toHaveBeenCalled();
  });

  it("分组：一次交出能做的那几个；新建分组整批进同一个新组，不问名字", async () => {
    open();
    await press(copyText("tabBatch.menu.leave", { n: 1 }));
    expect(calls(host.leaveGroup)).toEqual([[["c"]]]);
    open();
    await press(copyText("tabBatch.menu.found", { n: 3 }));
    expect(askText, "不弹框问名字").not.toHaveBeenCalled();
    expect(calls(host.foundGroup)).toEqual([[["a", "b", "c"]]]);
  });

});
