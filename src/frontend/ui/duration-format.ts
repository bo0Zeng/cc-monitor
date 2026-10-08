import { copyText } from "./copy-table.ts";

/**
 * 时长 → 给人看的一格（〔rules.json C-W6〕表里只放 `{dur}`，值由它出；单位格住表里 `durationFormat.unit.*`）：不满 1 秒写毫秒 · 不满 1 分钟按十分之一秒四舍五入、
 * 整数不带「.0」· 不满 1 小时写「N 分钟」或「N 分 M 秒」· 往上写「N 小时」或「N 小时 M 分」（秒数舍去）。
 * Rust 那一份 `copy_core::format_duration` 同形（两侧各对金样 `tests/__fixtures__/duration-format.golden.json`）。
 */
export function formatDuration(ms: number): string {
  const n = Math.max(0, Math.round(ms));
  if (n < 1000) return copyText("durationFormat.unit.ms", { n: String(n) });
  const tenths = Math.floor((n + 50) / 100);
  if (tenths < 600) return copyText("durationFormat.unit.sec", { n: tenths % 10 === 0 ? String(tenths / 10) : `${Math.floor(tenths / 10)}.${tenths % 10}` });
  const s = Math.floor((n + 500) / 1000);
  if (s < 3600) return s % 60 === 0 ? copyText("durationFormat.unit.min", { n: String(s / 60) }) : copyText("durationFormat.unit.minSec", { m: String(Math.floor(s / 60)), s: String(s % 60) });
  const h = String(Math.floor(s / 3600));
  const m = Math.floor((s % 3600) / 60);
  return m === 0 ? copyText("durationFormat.unit.hour", { n: h }) : copyText("durationFormat.unit.hourMin", { h, m: String(m) });
}
