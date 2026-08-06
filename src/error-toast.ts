/**
 * 后端 ERROR 级别 tracing event 的前端可视化（v2.0.0 落地 issue #4）。
 *
 * 解决 v1.7.0-1.7.7 "windows_subsystem=windows 无 stderr → ERROR 用户看不到"
 * 的结构性问题。后端 logging::ErrorEmitterLayer 拦截 Level::ERROR → emit
 * `monitor-error` 事件 → 这里 listen 后弹红色 toast（点击直接打开 log 文件）。
 *
 * 设计要点：
 * - 多条 ERROR 用 `.ccm-toast-stack` 容器**垂直堆叠**显示（不互相覆盖）；
 *   每个 toast 6s 自动消失
 * - 点击 toast → 调 `open_log_file` IPC，跳到 log 文件供详细查看
 * - 后端已经做了 60s/20 条限频〔据本注释，**没有判据读它**〕，前端不再额外**限频**
 *   ⚠ audit-0805 F07：**限频不等于聚合**。限频拦的是「同一个发射点短时间内刷很多条」；
 *   拦不住「**N 台远端同一瞬间各报一条**」—— 那是 N 个不同的 ERROR，一条都不该被丢，
 *   但它们不该占 N 个位置。所以前端做的是**合流**（同标题并成一条 + `×N` 计数），
 *   不是限频：**一条都没少报，只是不再刷屏**。
 *
 * 参照 INVARIANT § 12（alert 不算错误反馈）落实：error toast 取代未来本可能用
 * 的 alert 成为关键失败默认反馈机制
 */

import { listen } from "@tauri-apps/api/event";
import { commands } from "./ipc/commands";

interface MonitorErrorPayload {
  level: string;
  target: string;
  message: string;
  timestamp: number;
}

const STACK_ID = "ccm-toast-stack";

export function bindErrorToast(): void {
  void listen<MonitorErrorPayload>("monitor-error", (e) => {
    showErrorToast(e.payload);
  });
}

function showErrorToast(p: MonitorErrorPayload): void {
  appendToast({
    headline: `⚠ ${p.target || "monitor"}`,
    body: p.message || "(无消息)",
    level: "error",
    durationMs: 6000,
    onClick: () => {
      void commands.open_log_file().catch((err) => {
        console.warn("open_log_file failed:", err);
      });
    },
    title: "点击打开 log 文件查看完整堆栈",
  });
}

/**
 * 前端通用失败 / 信息提示。INVARIANT § 12 要求：关键失败不能用 alert（用户看不清就关了），
 * 必须配持续可见的 toast。`level: "error"` 红色（与后端 ERROR 同栈），`level: "info"` 灰色。
 * 多条堆叠不互相覆盖；默认 5s 自动消失。
 */
export function showActionFailureToast(
  headline: string,
  body: string,
  opts: { level?: "error" | "info"; durationMs?: number; onClick?: () => void } = {},
): void {
  const level = opts.level ?? "error";
  appendToast({
    headline,
    body,
    level,
    durationMs: opts.durationMs ?? 5000,
    onClick: opts.onClick,
    title: opts.onClick ? "点击查看" : undefined,
  });
}

interface ToastSpec {
  headline: string;
  body: string;
  level: "error" | "info";
  durationMs: number;
  onClick?: () => void;
  title?: string;
}

/** 一条**还活着**的 toast，用于同标题合流。 */
interface LiveToast {
  el: HTMLElement;
  bodyEl: HTMLElement;
  countEl: HTMLElement;
  count: number;
  timer: ReturnType<typeof setTimeout> | null;
}

/**
 * `(level, headline)` → 活着的那一条。
 *
 * # 为什么要合流〔audit-0805 F07 下半第二刀，报告 B-6 环 5〕
 *
 * 报告说环 5「逐台失败**UI 无任何信号**」—— **核实后不成立**，信号有四处。
 * 真缺陷是**信号无聚合**：`appendToast` 原来无条件新建一个节点。
 *
 * 而「N 台 = N 个 toast」的路径**不是**报告暗示的历史扇出（那条已经聚合过了：
 * `history.ts` 是 `failedHosts.join("、")` 一条），是这两条：
 *   ① **后端 `monitor-error` 事件**：每台远端各报一条 ERROR ⇒ 各弹一个（`bindErrorToast`）；
 *   ② **`remote-health` 的节流键含 `origin`**（`remote-health.ts` 的 `${origin}|${kind}`）——
 *      它防的是「**同一台**重复弹」，**不防「多台各弹一个」**。5 台同时 overflow = 5 个
 *      同标题 toast，每个 8 秒。
 *
 * ⇒ 合流键取 `(level, headline)`：同一类提示只占一个位置，带 `×N` 计数、body 显示**最新**那条。
 * ⚠ **`level` 必须进键**：红色 error 与灰色 info 是两种严重度，合成一条会把其中一种的
 * 视觉语义抹掉。
 */
const liveToasts = new Map<string, LiveToast>();

function toastKey(level: ToastSpec["level"], headline: string): string {
  return `${level}\u0000${headline}`;
}

/** 重新计时。合流时**必须**重置 —— 否则最后一条刚合进来就被上一条的计时器抹掉。 */
function armDismiss(key: string, t: LiveToast, durationMs: number): void {
  if (t.timer !== null) clearTimeout(t.timer);
  t.timer = setTimeout(() => {
    t.el.remove();
    liveToasts.delete(key);
  }, durationMs);
}

function appendToast(spec: ToastSpec): void {
  const key = toastKey(spec.level, spec.headline);
  const existing = liveToasts.get(key);
  if (existing) {
    existing.count += 1;
    existing.bodyEl.textContent = spec.body; // 显示最新一条，不是卡在第一条
    existing.countEl.textContent = `×${existing.count}`;
    existing.countEl.hidden = false;
    armDismiss(key, existing, spec.durationMs);
    return;
  }

  const stack = ensureStack();

  const toast = document.createElement("div");
  toast.className = spec.level === "error" ? "ccm-toast ccm-toast-error" : "ccm-toast";
  if (spec.title) toast.title = spec.title;

  const headline = document.createElement("div");
  headline.className = "ccm-toast-headline";
  headline.textContent = spec.headline;
  const countEl = document.createElement("span");
  countEl.className = "ccm-toast-count";
  countEl.hidden = true; // 只有真合流过才显示
  headline.appendChild(countEl);
  toast.appendChild(headline);

  const body = document.createElement("div");
  body.className = "ccm-toast-body";
  body.textContent = spec.body;
  toast.appendChild(body);

  const live: LiveToast = { el: toast, bodyEl: body, countEl, count: 1, timer: null };

  if (spec.onClick) {
    const cb = spec.onClick;
    toast.addEventListener("click", () => {
      cb();
      if (live.timer !== null) clearTimeout(live.timer);
      liveToasts.delete(key);
      toast.remove();
    });
  }

  stack.insertBefore(toast, stack.firstChild);
  liveToasts.set(key, live);
  armDismiss(key, live, spec.durationMs);
}

/**
 * 拿/建一个 fixed bottom-right 的容器，所有 toast append 到这里自动垂直堆叠。
 * 之所以用一个独立 stack 容器而不是直接 body.append 多个 fixed toast，是因为
 * 多个 fixed 元素都设 `right: 20px; bottom: 20px` 会**互相覆盖**。stack 用
 * flex-column-reverse 自然堆叠，每个 toast 静态布局不抢位置。
 */
function ensureStack(): HTMLElement {
  let stack = document.getElementById(STACK_ID);
  if (!stack) {
    stack = document.createElement("div");
    stack.id = STACK_ID;
    document.body.appendChild(stack);
  }
  return stack;
}
