/**
 * 假后端替真后端写钟面（真后端按那台本地钟写 `timeText` · `startText` · `started_text` · `captured_at_text` 那几格；这里按截图机的本地钟）。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";

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

// ── 历史页那几格（真后端 `common::time::{row_time, section_text, span_text, hit_time}`；这里按截图机的本地日历）──

const DAY = 86_400_000;
const dayStart = (ms: number): number => {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
};
const md = (ms: number): string => {
  const d = new Date(ms);
  return `${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};
const year = (ms: number): number => new Date(ms).getFullYear();

/** 行尾那一格。 */
export function rowTime(at: number, now = Date.now()): string {
  const today = dayStart(now);
  if (at >= today) return hm(at);
  if (at >= today - DAY) return `${md(at)} ${hm(at)}`;
  return year(at) === year(now) ? md(at) : `${year(at)}-${md(at)}`;
}

/** 分段头（文案表那几条：今天 · 昨天 · 本周 · 按月）。 */
export function sectionText(at: number, now = Date.now()): string {
  const today = dayStart(now);
  if (at >= today) return copyText("history.section.today");
  if (at >= today - DAY) return copyText("history.section.yesterday");
  const dow = (new Date(today).getDay() + 6) % 7;
  if (at >= today - dow * DAY) return copyText("history.section.week");
  const d = new Date(at);
  return year(at) === year(now) ? copyText("history.section.month", { month: d.getMonth() + 1 }) : copyText("history.section.yearMonth", { year: year(at), month: d.getMonth() + 1 });
}

/** 内容头那一段。 */
export function spanText(from: number, to: number, now = Date.now()): string {
  if (dayStart(from) === dayStart(to)) return dayStart(to) === dayStart(now) ? `${hm(from)}–${hm(to)}` : `${md(from)} ${hm(from)}–${hm(to)}`;
  return `${md(from)} ${hm(from)} – ${md(to)} ${hm(to)}`;
}

/** 会话内查找那一行的时刻。 */
export function hitTime(at: number, yesterday: string, now = Date.now()): string {
  const today = dayStart(now);
  if (at >= today) return hm(at);
  if (at >= today - DAY) return `${yesterday} ${hm(at)}`;
  return year(at) === year(now) ? `${md(at)} ${hm(at)}` : `${year(at)}-${md(at)} ${hm(at)}`;
}
