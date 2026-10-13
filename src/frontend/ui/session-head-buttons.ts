/**
 * **会话头上放哪几颗按钮**（ui3 · 10-11 稿「会话头按钮」方案②：用户自己排，默认 ＝ 方案①那一排）—— 纯函数那一半。
 *
 * - 能放上去的：「⋯」里除「结束会话…」以外的每一项（下面 [`HEAD_BUTTONS`]）。按状态那一颗（恢复 ▾ · 在终端里打开 · 重新连接）
 *   照今天自己出、永远在最左，不进清单；「更多 ⋯」固定在最右，也不进清单。
 * - 存的是「会话头上那几颗、按顺序」；不在表里 ＝ 只在「⋯」里。没存过 ＝ 默认（[`DEFAULT_HEAD_BUTTONS`]，默认值只在这一处）。
 * - 认不出的 id（以后删了某项）读时跳过、不报错；重复的只留第一个。
 *
 * 读写盘（`config.json` 的 `sessionHead.buttons`）在 `session-head.ts`；编辑浮层在 `session-head-edit.ts`。
 */

/** 一颗能放到会话头上的按钮。 */
export type HeadButtonId = "cwd" | "front" | "terminal" | "popOut" | "find" | "account" | "display" | "pin" | "group" | "resume" | "close";

/** 全集（编辑浮层下段「只在「⋯」里的」按这个顺序列）。 */
export const HEAD_BUTTONS: readonly HeadButtonId[] = ["cwd", "front", "terminal", "popOut", "find", "account", "display", "pin", "group", "resume", "close"];

/** 默认那一排（方案①）。 */
export const DEFAULT_HEAD_BUTTONS: readonly HeadButtonId[] = ["cwd", "front", "terminal", "popOut", "account", "display", "find"];

const KNOWN = new Set<string>(HEAD_BUTTONS);

/** 盘上那一格 ⇒ 会话头上那几颗（按顺序）；不是数组 ⇒ `null`（＝ 没设过，用默认）。 */
export function sanitizeHeadButtons(raw: unknown): HeadButtonId[] | null {
  if (!Array.isArray(raw)) return null;
  const out: HeadButtonId[] = [];
  for (const x of raw) if (typeof x === "string" && KNOWN.has(x) && !out.includes(x as HeadButtonId)) out.push(x as HeadButtonId);
  return out;
}

/** 编辑浮层的清单：上段 ＝ 会话头上的（按顺序，勾着）；下段 ＝ 只在「⋯」里的（按全集顺序）。 */
export function headEditRows(on: readonly HeadButtonId[]): { id: HeadButtonId; on: boolean }[] {
  return [...on.map((id) => ({ id, on: true })), ...HEAD_BUTTONS.filter((id) => !on.includes(id)).map((id) => ({ id, on: false }))];
}

/** 勾 / 不勾：勾上 ＝ 放到会话头末尾；取消 ＝ 只留在「⋯」。 */
export function toggleHeadButton(on: readonly HeadButtonId[], id: HeadButtonId): HeadButtonId[] {
  return on.includes(id) ? on.filter((x) => x !== id) : [...on, id];
}

/** Alt+↑ / Alt+↓：会话头上的那一颗挪一格（到头了不动；只在「⋯」里的那几行没有顺序可挪）。 */
export function nudgeHeadButton(on: readonly HeadButtonId[], id: HeadButtonId, delta: -1 | 1): HeadButtonId[] {
  const i = on.indexOf(id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= on.length) return [...on];
  const out = [...on];
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}

/**
 * 拖一行到清单里第 `at` 格（插入线在第 `at` 行之前；`at` ＝ 行数 ⇒ 末尾）。落在上段（会话头上那几颗之间或紧跟其后）⇒ 放到会话头那个位置
 * （没勾的拖上来 ＝ 勾上）；落在下段 ⇒ 只在「⋯」里（下段没有顺序）。
 */
export function dropHeadButton(on: readonly HeadButtonId[], id: HeadButtonId, at: number): HeadButtonId[] {
  const rows = headEditRows(on).map((r) => r.id);
  const from = rows.indexOf(id);
  if (from < 0) return [...on];
  // 先拿掉被拖的那一行，插入位置跟着左移。
  const rest = on.filter((x) => x !== id);
  const target = at > from ? at - 1 : at;
  if (target > rest.length) return rest;
  return [...rest.slice(0, target), id, ...rest.slice(target)];
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
