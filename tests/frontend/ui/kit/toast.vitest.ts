/**
 * toast（C13）与撤销提示条（I11）：停留时长 · 出错不自己走 · 悬停停表 · 最多 3 条 ＋ 记录 · 同类合流 ×N 明细不丢 · 撤销 / 到点提交。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { copyText } from "../../../../src/frontend/ui/copy-table";

type Kit = typeof import("../../../../src/frontend/ui/kit/toast");
let kit: Kit;

const toasts = (): HTMLElement[] => [...(document.getElementById("kit-toast-stack")?.children ?? [])] as HTMLElement[];
const detailOf = (el: HTMLElement): string => el.querySelector<HTMLElement>('[data-part="detail"]')?.textContent ?? "";
const countOf = (el: HTMLElement): string => {
  const c = el.querySelector<HTMLElement>('[data-part="count"]');
  return c && !c.hidden ? (c.textContent ?? "") : "";
};
const buttonsOf = (el: HTMLElement): HTMLButtonElement[] => [...el.querySelectorAll("button")];

describe("toast（C13）", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    vi.resetModules();
    kit = await import("../../../../src/frontend/ui/kit/toast");
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("失败 toast（failToast）：带详情 ⇒ 标题是出错那端写好的那一句、灰字只放一格事实、出［复制详情］；不带 ⇒ 标题留界面那句、无灰字、无按钮、原文进控制台", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const said = Object.assign(new Error("X 未启动 · 夹具原因"), { detail: "码：exit status: 1" });
    kit.failToast("打开 X 没成（夹具）", said, { fact: "devbox" });
    kit.failToast("存 Y 没成（夹具）", new TypeError("x is undefined"));
    const a = toasts().find((t) => t.textContent?.includes("X 未启动"))!;
    const b = toasts().find((t) => t !== a)!;
    expect(a, "带详情的那条：标题是出错那端写好的那一句").toBeDefined();
    expect(a.textContent).not.toContain("打开 X 没成");
    expect(detailOf(a)).toBe("devbox");
    expect(buttonsOf(a).map((x) => x.textContent)).toContain(copyText("detail.act.copy"));
    expect(b.textContent).toContain("存 Y 没成（夹具）");
    expect(b.textContent, "JS 自己抛的原文不上屏").not.toContain("x is undefined");
    expect(detailOf(b)).toBe("");
    expect(buttonsOf(b).map((x) => x.textContent)).not.toContain(copyText("detail.act.copy"));
    expect(warn.mock.calls.flat().some((x) => x instanceof TypeError), "原文进控制台").toBe(true);
    warn.mockRestore();
  });

  it("纯告知 4s 自己走；带动作 8s；出错不自己走", () => {
    kit.toast("已复制", "", { level: "info" });
    kit.toast(copyText("machineList.remove.done", { machine: "devbox" }), "", { level: "success", action: { label: copyText("kit.toast.undo"), run: () => {} } });
    kit.toast("恢复失败 · orders", "devbox 离线", { level: "error" });
    vi.advanceTimersByTime(kit.TOAST_PLAIN_MS + 1);
    expect(toasts().length).toBe(2);
    vi.advanceTimersByTime(kit.TOAST_ACTION_MS);
    expect(toasts().length).toBe(1);
    vi.advanceTimersByTime(60_000);
    expect(toasts().length, "出错的 toast 自己走了").toBe(1);
    buttonsOf(toasts()[0]).at(-1)!.click();
    expect(toasts().length, "点 × 没关").toBe(0);
  });

  it("鼠标悬停时停表，离开后接着走剩下的时间", () => {
    kit.toast("已复制", "", { level: "info" });
    vi.advanceTimersByTime(3000);
    toasts()[0].dispatchEvent(new MouseEvent("mouseenter"));
    vi.advanceTimersByTime(10_000);
    expect(toasts().length, "悬停时到点走了").toBe(1);
    toasts()[0].dispatchEvent(new MouseEvent("mouseleave"));
    vi.advanceTimersByTime(900);
    expect(toasts().length).toBe(1);
    vi.advanceTimersByTime(200);
    expect(toasts().length).toBe(0);
  });

  it("最多 3 条：第 4 条进来最老的那条收起（到点那条路：它的收尾照做）；「消息」里留最近 20 条", () => {
    const expired: string[] = [];
    for (const n of ["a", "b", "c", "d"]) kit.toast(n, "", { level: "error", onExpire: () => expired.push(n) });
    expect(toasts().length).toBe(kit.TOAST_MAX_VISIBLE);
    expect(expired).toEqual(["a"]);
    for (let i = 0; i < 30; i++) kit.toast(`t${i}`, "", { level: "error" });
    expect(kit.recentToasts().length).toBe(kit.TOAST_RECORD_MAX);
    expect(kit.recentToasts()[0].title).toBe("t29");
  });

  it("★ 同一级别同一标题合流成一条 ×N，显示最新那条，明细都在", () => {
    for (const host of ["box1", "box2", "box3"]) kit.toast("远端管道拥塞", `${host} 丢行`, { level: "info" });
    expect(toasts().length).toBe(1);
    expect(countOf(toasts()[0])).toBe("×3");
    expect(detailOf(toasts()[0])).toBe("box3 丢行");
    expect(toasts()[0].title, "合流把前两条明细丢了").toBe("box1 丢行\nbox2 丢行\nbox3 丢行");
  });

  it("★ 标题不同 · 级别不同 · 带动作的都不合流", () => {
    kit.toast("A", "", { level: "info" });
    kit.toast("B", "", { level: "info" });
    kit.toast("A", "", { level: "error" });
    expect(toasts().length).toBe(3);
  });

  it("★ 合流重置计时；走了之后再来同一类从头算", () => {
    kit.toast("X", "1", { level: "info" });
    vi.advanceTimersByTime(3500);
    kit.toast("X", "2", { level: "info" });
    vi.advanceTimersByTime(1000);
    expect(toasts().length, "最新那条刚合进来就被旧计时器抹掉").toBe(1);
    vi.advanceTimersByTime(kit.TOAST_PLAIN_MS);
    expect(toasts().length).toBe(0);
    kit.toast("X", "3", { level: "info" });
    expect(countOf(toasts()[0])).toBe("");
  });
});

describe("撤销提示条（I11）", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    vi.resetModules();
    kit = await import("../../../../src/frontend/ui/kit/toast");
  });
  afterEach(() => vi.useRealTimers());

  it("点「撤销」⇒ 只撤、不提交；到 8 秒 ⇒ 只提交", () => {
    const log: string[] = [];
    kit.undoToast("已关闭 orders", () => log.push("undo"), () => log.push("commit"));
    const [undo] = buttonsOf(toasts()[0]);
    expect(undo.textContent).toBe(copyText("kit.toast.undo"));
    undo.click();
    vi.advanceTimersByTime(20_000);
    expect(log).toEqual(["undo"]);

    kit.undoToast("已关闭 build", () => log.push("undo2"), () => log.push("commit2"));
    vi.advanceTimersByTime(kit.TOAST_ACTION_MS - 1);
    expect(log).toEqual(["undo"]);
    vi.advanceTimersByTime(2);
    expect(log).toEqual(["undo", "commit2"]);
  });

  it("点 × ⇒ 当作不撤：提交", () => {
    const log: string[] = [];
    kit.undoToast(copyText("machineList.remove.done", { machine: "devbox" }), () => log.push("undo"), () => log.push("commit"));
    buttonsOf(toasts()[0]).at(-1)!.click();
    expect(log).toEqual(["commit"]);
  });

  it("带复制详情的出错：动作排到句子下面那一行（修法在前、复制详情在后）、× 留右上；合流 ×N 复制出全部段；点了算看过", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn(() => Promise.resolve()) } });
    kit.toast("t-1", "devbox", { detail: "码：a", action: { label: "act-1", run: () => {} } });
    const [el] = toasts();
    expect(el.dataset.layout).toBe("two");
    const row = el.querySelector('[data-part="detail"]')!.parentElement!.lastElementChild as HTMLElement;
    expect([...row.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["act-1", copyText("detail.act.copy")]);
    const right = el.lastElementChild as HTMLElement;
    expect([...right.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"))).toEqual([copyText("kit.toast.close")]);
    kit.toast("t-2", "", { detail: "码：b" });
    kit.toast("t-2", "", { detail: "码：c" });
    const merged = toasts()[0];
    expect(kit.unseenErrors()).toBe(true);
    merged.querySelector<HTMLButtonElement>('[data-part="copy-detail"] button')!.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith(`t-2 ${copyText("kit.toast.count", { n: 2 })}\n\n码：b\n\n码：c`);
    expect(toasts()).toHaveLength(2);
    expect(kit.recentToasts()[0].seen).toBe(true);
  });

  it("没有复制详情的 toast 版式不变（动作仍在右边那一列）", () => {
    kit.toast("t-3", "", { level: "success", action: { label: "act-3", run: () => {} } });
    const [el] = toasts();
    expect(el.dataset.layout).toBeUndefined();
    expect(el.querySelector('[data-part="copy-detail"]')).toBeNull();
    expect(buttonsOf(el.lastElementChild as HTMLElement).map((b) => b.textContent)).toEqual(["act-3", ""]);
  });

  it("合流与不合流：每段复制详情只跟它那一条走（别的句子的详情不进来，同句合流段段按先后接）", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn(() => Promise.resolve()) } });
    kit.toast("t-a", "", { detail: "da-1" });
    kit.toast("t-b", "", { detail: "db-1" });
    kit.toast("t-a", "", { detail: "da-2" });
    const recs = kit.recentToasts();
    const a = recs.find((r) => r.title === "t-a")!;
    const b = recs.find((r) => r.title === "t-b")!;
    expect(a.copy.map(([t, d]) => [t, d])).toEqual([["t-a", "da-1"], ["t-a", "da-2"]]);
    expect(b.copy.map(([t, d]) => [t, d])).toEqual([["t-b", "db-1"]]);
    expect(kit.recordDetail(b)).toBe("t-b\ndb-1");
  });
});
