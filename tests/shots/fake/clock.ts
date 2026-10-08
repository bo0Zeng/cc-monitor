/**
 * 假后端替真后端写钟面（真后端按那台本地钟写 `timeText` · `startText` · `started_text` · `captured_at_text` 那几格；这里按截图机的本地钟）。
 */
const p = (n: number): string => String(n).padStart(2, "0");

/** 毫秒时刻 ⇒ `HH:MM`。 */
export function hm(ms: number): string {
  const d = new Date(ms);
  return `${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 毫秒时刻 ⇒ `HH:MM:SS`。 */
export function hms(ms: number): string {
  const d = new Date(ms);
  return `${hm(ms)}:${p(d.getSeconds())}`;
}
