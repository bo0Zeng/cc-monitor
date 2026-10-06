/**
 * 历史页的时间写法（设计稿「文件与历史」乙4-③ · 审稿 F8）：按看的人这台的本地日历算，纯函数（`now` 由调用方给）。
 *
 * - 分段：今天 · 昨天 · 本周（周一起）· 再往前按月（`9 月`；不是今年的 `2025 年 9 月`）。
 * - 行尾时间：今天 `14:02` · 昨天 `10-01 22:10` · 更早 `09-30`（不是今年的 `2025-09-30`）。
 */
import { copyText } from "../copy-table";

const DAY = 86_400_000;

function dayStart(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

const pad = (n: number): string => String(n).padStart(2, "0");

/** 分段的键（同一段同一个键；键的先后 = 时间的先后）。 */
export function sectionKey(at: number, now: number): string {
  const today = dayStart(now);
  if (at >= today) return "d0";
  if (at >= today - DAY) return "d1";
  // 本周：从这周一 0 点起（周日算上一周的末尾）。
  const dow = (new Date(today).getDay() + 6) % 7;
  if (at >= today - dow * DAY) return "w0";
  const d = new Date(at);
  return `m${d.getFullYear()}-${pad(d.getMonth() + 1)}`;
}

/** 分段头的字。 */
export function sectionLabel(key: string, now: number): string {
  if (key === "d0") return copyText("history.section.today");
  if (key === "d1") return copyText("history.section.yesterday");
  if (key === "w0") return copyText("history.section.week");
  const [y, m] = key.slice(1).split("-").map(Number);
  return y === new Date(now).getFullYear()
    ? copyText("history.section.month", { month: m })
    : copyText("history.section.yearMonth", { year: y, month: m });
}

/** 行尾那一格时间。 */
export function rowTime(at: number, now: number): string {
  const d = new Date(at);
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const today = dayStart(now);
  if (at >= today) return hm;
  const md = `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  if (at >= today - DAY) return `${md} ${hm}`;
  return d.getFullYear() === new Date(now).getFullYear() ? md : `${d.getFullYear()}-${md}`;
}

/** 内容头那一段：`09-30 03:00 – 10-01 04:57`；同一天 `02:01–14:02`（今天）或 `09-30 02:01–14:02`。 */
export function spanText(from: number, to: number, now: number): string {
  const a = new Date(from);
  const b = new Date(to);
  const hm = (d: Date): string => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const md = (d: Date): string => `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  if (dayStart(from) === dayStart(to)) {
    return dayStart(to) === dayStart(now) ? `${hm(a)}–${hm(b)}` : `${md(a)} ${hm(a)}–${hm(b)}`;
  }
  return `${md(a)} ${hm(a)} – ${md(b)} ${hm(b)}`;
}
