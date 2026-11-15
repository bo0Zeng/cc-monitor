/**
 * **会话头上放哪几颗按钮**（用户自己排，默认 ＝ 拉前 · 看它的终端 · 在会话里找）—— 纯函数那一半。
 *
 * - 能放上去的：「⋯」里除「结束会话…」以外的每一项（下面 [`HEAD_BUTTONS`]）。按状态那一颗（恢复 ▾）
 *   照今天自己出、永远在最左，不进清单；「更多 ⋯」固定在最右，也不进清单。
 * - 存的是「会话头上那几颗、按顺序」；不在表里 ＝ 只在「⋯」里。没存过 ＝ 默认（[`DEFAULT_HEAD_BUTTONS`]，默认值只在这一处）。
 * - 认不出的 id（删了的项：`display` …）读时跳过、不报错，不迁移；重复的只留第一个。
 *
 * 读写盘（`config.json` 的 `sessionHead.buttons`）在 `session-head.ts`；编辑浮层在 `session-head-edit.ts`；排 · 勾 · 挪 · 拖的纯函数住 `button-row.ts`。
 */
import { sanitizeRow } from "./button-row";

/** 一颗能放到会话头上的按钮。 */
export type HeadButtonId = "cwd" | "front" | "terminal" | "popOut" | "find" | "account" | "pin" | "group" | "resume" | "close";

/** 全集（编辑浮层下段「只在「⋯」里的」按这个顺序列）。 */
export const HEAD_BUTTONS: readonly HeadButtonId[] = ["front", "terminal", "find", "cwd", "popOut", "account", "pin", "group", "resume", "close"];

/** 默认那一排：拉前 · 看它的终端（照 5.1.1 各自一颗）与「在会话里找」（其余都在「⋯」里，用户可自己摆上来）。 */
export const DEFAULT_HEAD_BUTTONS: readonly HeadButtonId[] = ["front", "terminal", "find"];

/** 盘上那一格 ⇒ 会话头上那几颗（按顺序）；不是数组 ⇒ `null`（＝ 没设过，用默认）。排 · 勾 · 挪 · 拖住 `button-row.ts`（与栏顶那一排共用）。 */
export function sanitizeHeadButtons(raw: unknown): HeadButtonId[] | null {
  return sanitizeRow(raw, HEAD_BUTTONS);
}

/**
 * 放不下时收几颗：标题至少留 `min`（或它本来就没那么长 ⇒ 它的全长），从排在最后的那颗起收进「⋯」。
 * `titleAt(k)` ＝ 露着前 `k` 颗时标题量得的宽；`natural` ＝ 标题不挤时的全长。回露着几颗。
 */
export function headButtonsThatFit(count: number, titleAt: (shown: number) => number, natural: number, min: number): number {
  const need = Math.min(min, natural);
  let k = count;
  while (k > 0 && titleAt(k) < need) k--;
  return k;
}
