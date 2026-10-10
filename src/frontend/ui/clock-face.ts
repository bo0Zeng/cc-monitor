/**
 * **界面自己出的事的钟面**（状态栏提示是几点出的）：那件事生在这一台、不经后端，没有后端写好的 `…Text` 可照抄，
 * 而看的人就在这一台 ⇒ 按这一台的钟本来就对。写法与核心写「几点」那一处（后端 `common/time.rs::hm`）同一套：`HH:MM`，
 * 两边对同一份金样 `tests/__fixtures__/clock-face.golden.json`。界面里别处不许自己换算时刻（出口扫描判据钉着；本文件登在欠账名单「留」那一格）。
 */

const pad = (n: number): string => String(n).padStart(2, "0");

/** 时 · 分 ⇒ `HH:MM`（两处共用的那一个排法）。 */
const hhmm = (h: number, m: number): string => `${pad(h)}:${pad(m)}`;

/** 毫秒时刻 ＋ 那一刻的偏移（分钟，东正）⇒ `HH:MM`（秒舍去）。纯函数，金样喂它。 */
export function clockFace(ms: number, offsetMin: number): string {
  const day = 1_440;
  const local = ((Math.floor(ms / 60_000) + offsetMin) % day + day) % day;
  return hhmm(Math.floor(local / 60), local % 60);
}

/** 这一台钟上那一刻的 `HH:MM`：那一刻的偏移由平台按这一台的时区给（时 · 分对 UTC 的差，夏令时按那一刻），排法走 [`clockFace`]。 */
export function clockFaceHere(ms: number): string {
  const d = new Date(ms);
  const utcMin = Math.floor(ms / 60_000) % 1_440;
  return clockFace(ms, (d.getHours() * 60 + d.getMinutes() - utcMin + 1_440) % 1_440);
}
