/**
 * 测试夹具里带时刻的记录补上后端出口那一下写的那一格钟面（`timeText`：看的那一台钟上的 `HH:MM`）。
 *
 * 生产里这一格只由后端写（通用记录的 `timeText`，`agents/record.rs::Record::stamp`，按请求 / 流带来的时区）；界面零换算。这里按跑测试这台的本地钟写
 * （`vitest.config.ts` 钉了时区），只给要看「卡上那一格时刻」的夹具用。已有 `timeText` 的、没时刻的、解不出的原样不动。
 */
export function withTimeText<T>(rec: T): T {
  const r = rec as { at?: unknown; timeText?: unknown };
  if (r === null || typeof r !== "object" || typeof r.at !== "string" || r.timeText !== undefined) return rec;
  const d = new Date(r.at);
  if (Number.isNaN(d.getTime())) return rec;
  const p = (n: number): string => String(n).padStart(2, "0");
  return { ...(rec as object), timeText: `${p(d.getHours())}:${p(d.getMinutes())}` } as T;
}
