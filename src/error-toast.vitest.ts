/**
 * audit-0805 F07 下半第二刀（报告 B-6 环 5）：**toast 合流**。
 *
 * # 回核把报告改形了两次
 *
 * 1. 报告说环 5「逐台失败 **UI 无任何信号**」—— **不成立**，信号有四处
 *    （项目级 / 列表级 `failedHosts` / 全台失败 / 等待期「加载中…」行）。
 *    核实台账已改形为「真缺陷是**信号无聚合**」。
 * 2. ★ 本轮再改一格：「N 台 = N 个 toast」**成立，但路径不是台账暗示的历史扇出** ——
 *    `history.ts` 那条早就是 `failedHosts.join("、")` 一条。真正会 N 连发的是：
 *    ① 后端 `monitor-error` 事件（每台各一条 ERROR ⇒ 各弹一个）；
 *    ② `remote-health` 的节流键是 `${origin}|${kind}` —— 防的是**同一台**重复弹，
 *       **不防多台各弹一个**。
 *
 * # 为什么是合流不是限频
 *
 * `error-toast.ts` 头注写着「后端已做 60s/20 条限频，前端不再额外限制」。
 * 限频拦的是「同一个发射点刷很多条」；**拦不住「N 台同一瞬间各报一条」**。
 * 那是 N 个不同的错误，**一条都不该被丢** —— 但它们不该占 N 个位置。
 *
 * ⚠ 本文件是 `error-toast.ts` 的**第一批直接测试**（此前全仓只有别的文件 mock 它）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("./ipc/commands", () => ({ commands: { open_log_file: vi.fn().mockResolvedValue(null) } }));

import { showActionFailureToast } from "./error-toast";

const STACK = "ccm-toast-stack";
const toasts = (): HTMLElement[] =>
  Array.from(document.getElementById(STACK)?.children ?? []) as HTMLElement[];
const bodyOf = (el: HTMLElement): string =>
  el.querySelector(".ccm-toast-body")?.textContent ?? "";
const countOf = (el: HTMLElement): string => {
  const c = el.querySelector(".ccm-toast-count") as HTMLElement | null;
  return c && !c.hidden ? (c.textContent ?? "") : "";
};

describe("toast 合流（audit-0805 F07 下半第二刀）", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
  });
  afterEach(() => {
    // 把还挂着的计时器跑完，免得活 toast 漏到下一条用例
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("★ 五台同时报同一类错 → 只占一个位置，带 ×5", () => {
    for (const host of ["box1", "box2", "box3", "box4", "box5"]) {
      showActionFailureToast("⚠ 远端管道拥塞", `${host} 丢了若干实时行`, { level: "info" });
    }
    expect(
      toasts().length,
      `弹了 ${toasts().length} 个 toast —— 这正是报告 B-6 环 5 改形后剩下的那件事：` +
        "信号无聚合。五台同时超时就刷五条，屏幕右下角排成一列。",
    ).toBe(1);
    expect(countOf(toasts()[0]), "合流了却不告诉用户一共几条 —— 那是把 4 条错误藏了").toBe("×5");
  });

  it("★ 反向：不同标题不许合成一条", () => {
    showActionFailureToast("⚠ 远端管道拥塞", "a", { level: "info" });
    showActionFailureToast("⚠ 远端 daemon 版本不符", "b", { level: "info" });
    expect(
      toasts().length,
      "两类完全不同的提示被并成了一条 —— 合流的键取得太宽，用户会漏掉其中一类",
    ).toBe(2);
  });

  it("★ level 不同不许合流 —— 红色 error 与灰色 info 是两种严重度", () => {
    showActionFailureToast("远端提示", "info 那条", { level: "info" });
    showActionFailureToast("远端提示", "error 那条", { level: "error" });
    expect(
      toasts().length,
      "同标题但一红一灰被并成一条 —— 其中一种的视觉语义被抹掉了",
    ).toBe(2);
  });

  it("★ body 显示**最新**那条，不是卡在第一条", () => {
    showActionFailureToast("⚠ 远端管道拥塞", "box1 出问题", { level: "info" });
    showActionFailureToast("⚠ 远端管道拥塞", "box9 出问题", { level: "info" });
    expect(
      bodyOf(toasts()[0]),
      "合流之后 body 还停在第一条 —— 用户看到的是最早那台，而不是刚发生的这台",
    ).toBe("box9 出问题");
  });

  it("★ 合流要重置计时，否则最后一条刚合进来就被上一条的计时器抹掉", () => {
    showActionFailureToast("⚠ 远端管道拥塞", "第一条", { level: "info", durationMs: 1000 });
    vi.advanceTimersByTime(900);
    showActionFailureToast("⚠ 远端管道拥塞", "第二条", { level: "info", durationMs: 1000 });
    vi.advanceTimersByTime(200); // 距第一条 1100ms（早该消失），距第二条 200ms
    expect(
      toasts().length,
      "第二条刚合进来 200ms 就没了 —— 计时器没重置，用户根本来不及看最新那条",
    ).toBe(1);
    vi.advanceTimersByTime(900);
    expect(toasts().length, "重置之后也得按时消失，不许赖着不走").toBe(0);
  });

  it("★ 消失之后再来同一类 → 新建一条，计数从头算", () => {
    showActionFailureToast("⚠ 远端管道拥塞", "a", { level: "info", durationMs: 500 });
    showActionFailureToast("⚠ 远端管道拥塞", "b", { level: "info", durationMs: 500 });
    vi.advanceTimersByTime(600);
    expect(toasts().length, "该消失了").toBe(0);

    showActionFailureToast("⚠ 远端管道拥塞", "c", { level: "info", durationMs: 500 });
    expect(toasts().length, "消失之后没能新建 —— 合流表没清，节点已 remove 却还被当成活的").toBe(1);
    expect(
      countOf(toasts()[0]),
      "新一轮的计数还带着上一轮的尾巴 —— 用户会以为刚刚又发生了三次",
    ).toBe("");
  });
});
