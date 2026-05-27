/**
 * 时间 / 字节 格式化（P2.2）。
 *
 * 之前 formatTime 有两份（cards/index.ts 接 ISO string、views/history.ts 接 ms
 * number）、formatBytes 有两份（data-section / diagnostics-section 精度不同）。
 * 集中收口避免后续漂移。
 *
 * 保留两套时间语义：
 * - **formatTimestampShort**：消息卡片显示时间戳，永远 `hh:mm`
 * - **formatTimestampSmart**：会话活动时间，当天 `hh:mm`，跨天 `yyyy-MM-dd hh:mm`
 */

/** 输入 ISO 字符串或 unix ms，返回 `hh:mm`（解析失败返原值字符串）。 */
export function formatTimestampShort(input: string | number): string {
  try {
    const d = typeof input === "number" ? new Date(input) : new Date(input);
    if (Number.isNaN(d.getTime())) return String(input);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return String(input);
  }
}

/** 输入 unix ms，当天显示 `hh:mm`，跨天显示完整 `yyyy-MM-dd hh:mm`；0/NaN 返 "—"。 */
export function formatTimestampSmart(ms: number): string {
  if (!ms) return "—";
  try {
    const d = new Date(ms);
    if (Number.isNaN(d.getTime())) return String(ms);
    const today = new Date();
    const sameDay =
      d.getFullYear() === today.getFullYear() &&
      d.getMonth() === today.getMonth() &&
      d.getDate() === today.getDate();
    if (sameDay) {
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    }
    return d.toLocaleString([], {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return String(ms);
  }
}

/** 字节数 → 人类可读：B / KB(1d) / MB(1d) / GB(2d) */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
