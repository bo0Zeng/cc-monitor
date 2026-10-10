/**
 * **时长与距今的那一个读口**（桌面；手机端是 `DurationFormat.kt`，Rust 核心是 `copy_core::short_duration` · `rel_duration`，三边对同一份金样
 * `tests/__fixtures__/short-duration.golden.json`）。
 *
 * 静态的时长 / 距今由核心写成 `…Text`，界面照抄；这里只管**会走的钟**：核心交 `{text, from, to?}`（`text` 里的 `{dur}` 是空位，
 * `from` / `to` 是毫秒时刻），此刻按 `now` 填（[`clockNow`]）。短时长的写法（文案规范 N5）：`45s` · `6m` · `1h50m` · `2h` · `3d`
 * （满 24h 只写天；秒以下舍去、分钟以下舍去），单位格住文案表 `durationFormat.short.*`。界面里别处不许自己拼时长 / 距今（出口扫描判据钉着）。
 */
import { copyText } from "./copy-table";

const DAY = 86_400;

/** 一段时长（秒；负数当 0）⇒ 短时长。 */
export function fmtDur(secs: number): string {
  const d = Math.max(0, Math.floor(secs));
  if (d < 60) return copyText("durationFormat.short.sec", { n: d });
  if (d >= DAY) return copyText("durationFormat.short.day", { n: Math.floor(d / DAY) });
  const mins = Math.floor(d / 60);
  if (mins < 60) return copyText("durationFormat.short.min", { n: mins });
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m === 0 ? copyText("durationFormat.short.hour", { n: h }) : copyText("durationFormat.short.hourMin", { h, m });
}

/** 会走的钟（核心交的那一形）：`text` 里 `{dur}` 那一截 ＝ (`to` 或此刻) − `from`。 */
export interface Clock {
  text: string;
  from: number;
  to?: number | null;
}

/** 会走的钟此刻的字（`from` 比此刻还晚 ⇒ 按 0）。 */
export function clockNow(c: Clock, now: number): string {
  return c.text.replace("{dur}", spanNow(c.from, c.to ?? now));
}

/** 从 `from`（毫秒时刻）到 `to`（毫秒时刻，缺 ⇒ 此刻）多久 ⇒ 短时长（「已经多久」「用了多久」都经它）。 */
export function spanNow(from: number, to: number): string {
  return fmtDur((to - from) / 1000);
}

/**
 * **已等多久**：后端在那台算好的 `waitedMs`（答出那一刻，起点与读 pidfile 同一台的钟）＋ 本机从收到那一份起走过的时间
 * （两段各在一台钟上量，不跨机器减）。后端写的 `waitedText` 就是收到那一刻这里写出来的字。没有起点 ⇒ `null`。
 */
export function waitedNow(n: { waitedMs: number | null; receivedAt: number }, now: number): string | null {
  return n.waitedMs === null ? null : fmtDur((n.waitedMs + Math.max(0, now - n.receivedAt)) / 1000);
}
