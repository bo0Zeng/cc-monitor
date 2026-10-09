/**
 * 壳推来的「要让你知道」的出错（`monitor-error`，形状 `UiErrorPayload`）⇒ 出错 toast：
 * 标题是壳照文案键写好的那一句，详情挂［复制详情］；那台连不上（`reconnect` 有值）⇒［重新连接］，另有［看日志］。
 * 日志行不走这条路（壳那边只推结构化的事件），这里也不拼任何字。
 *
 * 壳那一侧限频；同一句连发由 toast 合流成一条 `×N`（明细不丢）。
 */
import { listen } from "@tauri-apps/api/event";
import { commands } from "./ipc/commands";
import { copyText } from "./copy-table";
import { toast, type ToastAction } from "./kit/toast";
import type { UiErrorPayload } from "./generated/UiErrorPayload";

/** 一条出错事件的动作：修法在前（重新连接那一台）、看日志在后。 */
export function uiErrorActions(p: UiErrorPayload): ToastAction[] {
  const out: ToastAction[] = [];
  const origin = p.reconnect;
  if (origin !== undefined) {
    out.push({
      label: copyText("errorToast.showErrorToast.reconnect"),
      run: () => void commands.backend_start({ origin }).catch((err) => console.warn("backend_start failed:", err)),
    });
  }
  out.push({
    label: copyText("errorToast.showErrorToast.openLog"),
    run: () => void commands.open_log_file().catch((err) => console.warn("open_log_file failed:", err)),
  });
  return out;
}

export function showUiError(p: UiErrorPayload): void {
  toast(p.said, "", { detail: p.detail, level: "error", action: uiErrorActions(p) });
}

export function bindErrorToast(): void {
  void listen<UiErrorPayload>("monitor-error", (e) => showUiError(e.payload));
}
