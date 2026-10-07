/**
 * toast 的两条全局排版规则（按「 · 」分段、段内不拆行 · 右侧抽屉开着时让开它）与「消息」那一份记录
 * （时刻 · 还能做的动作 · 撤销期过了不再给撤销 · 出错没看过的琥珀点 · Ctrl+Z 撤最新那一条）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

type Kit = typeof import("../../../../src/frontend/ui/kit/toast");
let kit: Kit;

const toasts = (): HTMLElement[] => [...(document.getElementById("kit-toast-stack")?.children ?? [])] as HTMLElement[];

describe("toast 排版 ＋「消息」记录", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    document.documentElement.style.removeProperty("--kit-drawer-right");
    vi.resetModules();
    kit = await import("../../../../src/frontend/ui/kit/toast");
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("🔴 一句按「 · 」分段：每段一个不拆行的块，段与段之间才换行；多行逐行照排；空的第二行不画", () => {
    kit.toast("已结束 11 · 失败 1", "排序模型蒸馏 · gpu-01 离线\n第二行", { level: "error" });
    const [t] = toasts();
    const segs = (part: Element) => [...part.querySelectorAll(":scope > span")].map((x) => x.textContent);
    const head = t.querySelector<HTMLElement>("[data-part=detail]")!.previousElementSibling!;
    expect(segs(head).filter((x) => x !== "")).toEqual(["已结束 11", "失败 1"]);
    const detail = t.querySelector<HTMLElement>("[data-part=detail]")!;
    expect(segs(detail)).toEqual(["排序模型蒸馏", "gpu-01 离线", "第二行"]);
    expect(detail.querySelectorAll("br").length).toBe(1);
    kit.toast("已复制", "", { level: "info" });
    expect(toasts()[0].querySelector("[data-part=detail]")!.childNodes.length, "空的第二行要是真空（CSS :empty 收起）").toBe(0);
  });

  it("🔴 右侧抽屉开着 ⇒ toast 让到它左边（抽屉宽写进 --kit-drawer-right），关了还原", async () => {
    const { openDrawer } = await import("../../../../src/frontend/ui/kit/drawer");
    const d = openDrawer({ title: "账号", body: document.createElement("div"), width: 440 });
    expect(document.documentElement.style.getPropertyValue("--kit-drawer-right")).toBe("440px");
    await d.close();
    expect(document.documentElement.style.getPropertyValue("--kit-drawer-right")).toBe("");
  });

  it("🔴 记录带时刻与还能做的动作；撤销期过了（到点提交）⇒ 记录里不再给「撤销」，重试照留", () => {
    const undo = vi.fn();
    const commit = vi.fn();
    kit.undoToast("已关闭「周报草稿」", undo, commit);
    kit.toast("结束失败 · 排序模型蒸馏", "gpu-01 离线", { level: "error", action: { label: "重试", run: vi.fn() } });
    const [retry, closed] = kit.recentToasts();
    expect(closed.actions.map((a) => a.label)).toEqual(["撤销"]);
    expect(closed.at).toBeGreaterThan(0);
    vi.advanceTimersByTime(8000);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(closed.actions, "撤销期过了还给撤销 ⇒ 点了也撤不回").toEqual([]);
    expect(retry.actions.map((a) => a.label)).toEqual(["重试"]);
  });

  it("🔴 在「消息」里点撤销 ⇒ 那条 toast 一起收起、不再提交、只撤一次", () => {
    const undo = vi.fn();
    const commit = vi.fn();
    kit.undoToast("已关闭「周报草稿」", undo, commit);
    const [rec] = kit.recentToasts();
    kit.runRecordAction(rec, rec.actions[0]);
    expect(undo).toHaveBeenCalledTimes(1);
    expect(toasts()).toHaveLength(0);
    vi.advanceTimersByTime(9000);
    expect(commit, "撤了还到点提交").not.toHaveBeenCalled();
    expect(rec.actions).toEqual([]);
  });

  it("🔴 Ctrl+Z（undoLatest）撤最新那一条还开着的撤销提示；没有 ⇒ false", () => {
    const a = vi.fn();
    const b = vi.fn();
    kit.undoToast("已关闭 A", a, vi.fn());
    kit.undoToast("已关闭 B", b, vi.fn());
    expect(kit.undoLatest()).toBe(true);
    expect([a.mock.calls.length, b.mock.calls.length]).toEqual([0, 1]);
    expect(kit.undoLatest()).toBe(true);
    expect(a).toHaveBeenCalledTimes(1);
    expect(kit.undoLatest()).toBe(false);
  });

  it("🔴 出错提示没在「消息」里看过 ⇒ 有琥珀点；看过 ⇒ 没有；纯告知不算", () => {
    const seen = vi.fn();
    kit.onToastRecords(seen);
    kit.toast("已复制", "", { level: "info" });
    expect(kit.unseenErrors()).toBe(false);
    kit.toast("结束失败", "", { level: "error" });
    expect(kit.unseenErrors()).toBe(true);
    kit.markRecordsSeen();
    expect(kit.unseenErrors()).toBe(false);
    expect(seen).toHaveBeenCalled();
  });

  it("🔴 逐条明细不上 toast、只进记录；［查看］⇒ 打开「消息」并展开那一条，再点那一行收起", async () => {
    const { StatusMessages, showMessage } = await import("../../../../src/frontend/ui/status-messages");
    const sm = new StatusMessages();
    document.body.appendChild(sm.el);
    kit.toast("已结束 1 · 失败 1", "", { level: "error", more: ["失败 · billing · 门拦下了", "跳过 · notes · 已经结束了"] });
    kit.toast("已结束 1 · 失败 1", "", { level: "error", more: ["失败 · web · 离线"] });
    expect(toasts().length, "带明细的各自一条，不合流").toBe(2);
    expect(toasts()[1].textContent, "明细不画在 toast 上").not.toContain("门拦下了");
    const first = kit.recentToasts()[1];
    expect(first.more).toEqual(["失败 · billing · 门拦下了", "跳过 · notes · 已经结束了"]);
    showMessage(first);
    const list = document.querySelector<HTMLElement>("[data-role=messages-list]")!;
    expect(list, "「消息」打开了").not.toBeNull();
    const more = [...list.querySelectorAll<HTMLElement>("[data-role=message-more]")];
    expect(more.map((m) => [...m.children].map((c) => c.textContent)), "只展开那一条").toEqual([["失败 · billing · 门拦下了", "跳过 · notes · 已经结束了"]]);
    list.querySelector<HTMLElement>("[data-more=open] [aria-expanded]")!.click();
    expect(list.querySelectorAll("[data-role=message-more]").length, "再点收起").toBe(0);
  });

  it("★ 只在提示条上出的那一颗（［查看］：做的就是打开「消息」里这一条）在「消息」里不再出；别的动作照出", async () => {
    const { StatusMessages, showMessage } = await import("../../../../src/frontend/ui/status-messages");
    const sm = new StatusMessages();
    document.body.appendChild(sm.el);
    kit.toast("已结束 1 · 失败 1", "", { level: "error", more: ["失败 · web · 离线"], action: [{ label: "查看", run: () => {}, toastOnly: true }, { label: "重试", run: () => {} }] });
    expect([...toasts()[0].querySelectorAll("button")].map((b) => b.textContent), "提示条上两颗都在").toEqual(expect.arrayContaining(["查看", "重试"]));
    showMessage(kit.recentToasts()[0]);
    const list = document.querySelector<HTMLElement>("[data-role=messages-list]")!;
    const labels = [...list.querySelectorAll("button")].map((b) => b.textContent);
    expect(labels).toContain("重试");
    expect(labels).not.toContain("查看");
  });
});
