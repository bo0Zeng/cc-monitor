/**
 * **用户自己排的一排按钮**（会话头那一排 · 标签栏顶那一排）—— 纯函数那一半，两处共用一份。
 *
 * - `all`：能放上去的全集（编辑清单下段按这个顺序列）；`on`：此刻摆着的那几颗（按顺序）。不在 `on` 里 ＝ 只在「⋯」里。
 * - 读盘那一格：不是数组 ⇒ `null`（＝ 没设过，用各自的缺省）；认不出的 id 跳过、重复的只留头一个，不报错、不迁移。
 */

/** 盘上那一格 ⇒ 摆着的那几颗（按顺序）；不是数组 ⇒ `null`。 */
export function sanitizeRow<Id extends string>(raw: unknown, all: readonly Id[]): Id[] | null {
  if (!Array.isArray(raw)) return null;
  const known = new Set<string>(all);
  const out: Id[] = [];
  for (const x of raw) if (typeof x === "string" && known.has(x) && !out.includes(x as Id)) out.push(x as Id);
  return out;
}

/** 编辑清单：上段 ＝ 摆着的（按顺序，勾着）；下段 ＝ 只在「⋯」里的（按全集顺序）。 */
export function rowEditRows<Id extends string>(on: readonly Id[], all: readonly Id[]): { id: Id; on: boolean }[] {
  return [...on.map((id) => ({ id, on: true })), ...all.filter((id) => !on.includes(id)).map((id) => ({ id, on: false }))];
}

/** 勾 / 不勾：勾上 ＝ 放到末尾；取消 ＝ 只留在「⋯」。 */
export function toggleRow<Id extends string>(on: readonly Id[], id: Id): Id[] {
  return on.includes(id) ? on.filter((x) => x !== id) : [...on, id];
}

/** Alt+↑ / Alt+↓：摆着的那一颗挪一格（到头了不动；只在「⋯」里的没有顺序可挪）。 */
export function nudgeRow<Id extends string>(on: readonly Id[], id: Id, delta: -1 | 1): Id[] {
  const i = on.indexOf(id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= on.length) return [...on];
  const out = [...on];
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}

/**
 * 拖一行到清单里第 `at` 格（插入线在第 `at` 行之前；`at` ＝ 行数 ⇒ 末尾）。落在上段（摆着的那几颗之间或紧跟其后）⇒ 放到那个位置
 * （没勾的拖上来 ＝ 勾上）；落在下段 ⇒ 只在「⋯」里（下段没有顺序）。
 */
export function dropRow<Id extends string>(on: readonly Id[], id: Id, at: number, all: readonly Id[]): Id[] {
  const rows = rowEditRows(on, all).map((r) => r.id);
  const from = rows.indexOf(id);
  if (from < 0) return [...on];
  // 先拿掉被拖的那一行，插入位置跟着左移。
  const rest = on.filter((x) => x !== id);
  const target = at > from ? at - 1 : at;
  if (target > rest.length) return rest;
  return [...rest.slice(0, target), id, ...rest.slice(target)];
}
