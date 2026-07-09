/**
 * 远端健康提示通道（SS-F，issue #32 起）。
 *
 * 后端远端数据源（ssh_source）把「拥塞丢行（#32 overflow）/ 版本不符（#33）」等
 * **非致命**健康事件经 `remote-health` Tauri 事件回传；这里单一 listener 收下、按
 * `origin|kind` 节流（拥塞期一台机器可能连发，避免 toast 刷屏）后弹一个灰色 info
 * toast（复用 error-toast.ts 的 toast 栈，不另造 UI）。
 *
 * 设计要点：
 * - **单一通道**：所有远端健康提示走这一个事件 + 这一个 listener。#33 版本协商复用，
 *   只换 `kind`/`message`，不另开事件（主计划 SS-F 最终形态）。
 * - **节流**：每个 (origin,kind) 至少隔 THROTTLE_MS 才再弹一次。纯函数
 *   `shouldShowHealthToast` 便于单测（remote-health.test.ts）。
 * - 致命错误仍走 error-toast 的红色 `monitor-error` 栈；本通道是 info 级提示。
 */

import { listen } from "@tauri-apps/api/event";
import { showActionFailureToast } from "./error-toast";
import { shouldShowHealthToast } from "./remote-health-throttle";

/** 后端 `RemoteHealthPayload`（camelCase serde，见 src-tauri/src/bridge.rs）。 */
interface RemoteHealthPayload {
  origin: string | null;
  kind: string;
  message: string;
}

/** kind → toast 标题（未知 kind 回退到通用「远端提示」）。 */
function headlineFor(kind: string): string {
  switch (kind) {
    case "overflow":
      return "⚠ 远端管道拥塞";
    case "version":
      return "⚠ 远端 daemon 版本不符";
    case "degraded":
      return "远端降级模式";
    case "snapshot":
      return "⚠ 远端历史快照拉取失败";
    default:
      return "⚠ 远端提示";
  }
}

const lastShown = new Map<string, number>();

/**
 * 订阅 `remote-health` 事件，按 (origin,kind) 节流后弹 info toast。
 * 在主窗口启动时调用一次（与 bindErrorToast 并列）。
 */
export function bindRemoteHealthToast(): void {
  void listen<RemoteHealthPayload>("remote-health", (e) => {
    const p = e.payload;
    const key = `${p.origin ?? ""}|${p.kind}`;
    const now = Date.now();
    if (!shouldShowHealthToast(lastShown.get(key), now)) return;
    lastShown.set(key, now);
    showActionFailureToast(headlineFor(p.kind), p.message || "(无消息)", {
      level: "info",
      durationMs: 8000,
    });
  });
}
