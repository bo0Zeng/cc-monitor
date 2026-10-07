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
});
