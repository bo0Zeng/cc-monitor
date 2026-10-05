/**
 * 后端 ERROR 级事件（`monitor-error`）⇒ 出错 toast ＋［打开日志］。
 *
 * 后端那一侧限频；同一来源同一时刻多台各报一条的，由 toast 合流成一条 `×N`（明细不丢）。
 */
import { listen } from "@tauri-apps/api/event";
import { commands } from "./ipc/commands";
import { copyText } from "./copy-table";
import { toast } from "./kit/toast";

interface MonitorErrorPayload {
  level: string;
  target: string;
  message: string;
  timestamp: number;
}

export function bindErrorToast(): void {
  void listen<MonitorErrorPayload>("monitor-error", (e) => {
    const p = e.payload;
    toast(p.target || "monitor", p.message || copyText("errorToast.showErrorToast.noMessage"), {
      level: "error",
      action: {
        label: copyText("errorToast.showErrorToast.openLog"),
        run: () => void commands.open_log_file().catch((err) => console.warn("open_log_file failed:", err)),
      },
    });
  });
}
