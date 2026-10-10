/**
 * 短时长（桌面秒级走字那一个读口，文案规范 N5）：`45s` · `6m` · `1h50m` · `2h` · `3d`（满 24h 只写天）。负数当 0。
 * 核心写好的字（会话状态一句里那一截 · 已等多久 `waitedText`）出自 Rust `copy_core::short_duration`；桌面要它跟着走时只用这一个读口填，
 * 两侧各对金样 `tests/__fixtures__/short-duration.golden.json`。
 */
const DAY = 86_400;

export function fmtDur(secs: number): string {
  const d = Math.max(0, Math.floor(secs));
  if (d < 60) return `${d}s`;
  if (d >= DAY) return `${Math.floor(d / DAY)}d`;
  const mins = Math.floor(d / 60);
  if (mins < 60) return `${mins}m`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m === 0 ? `${h}h` : `${h}h${m}m`;
}
