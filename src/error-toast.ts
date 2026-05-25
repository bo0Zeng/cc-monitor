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
 * - 后端已经做了 60s/20 条限频，前端不再额外限制
 *
 * 参照 INVARIANT § 12（alert 不算错误反馈）落实：error toast 取代未来本可能用
 * 的 alert 成为关键失败默认反馈机制
 */

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

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
  const stack = ensureStack();

  const toast = document.createElement("div");
  toast.className = "ccm-toast ccm-toast-error";
  toast.title = "点击打开 log 文件查看完整堆栈";

  const headline = document.createElement("div");
  headline.className = "ccm-toast-headline";
  headline.textContent = `⚠ ${p.target || "monitor"}`;
  toast.appendChild(headline);

  const body = document.createElement("div");
  body.className = "ccm-toast-body";
  body.textContent = p.message || "(无消息)";
  toast.appendChild(body);

  toast.addEventListener("click", () => {
    void invoke("open_log_file").catch((err) => {
      console.warn("open_log_file failed:", err);
    });
    toast.remove();
  });

  // 最新的放最上面（用户视线先扫到）
  stack.insertBefore(toast, stack.firstChild);

  // 自动消失。比 #bring-terminal-toast 的 8s 短一些（ERROR 频率比拉前失败高）
  window.setTimeout(() => toast.remove(), 6000);
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
