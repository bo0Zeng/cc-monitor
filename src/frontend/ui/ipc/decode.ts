/**
 * 两端契约的严格收那三个小件：是不是对象 · 键集合恰好是哪几格 · 必有几格 ＋ 可缺几格。
 * 多一格 / 缺一格都不收；各读口按自己的说法抛，这里只答是 / 不是。界面里只有这一份。
 */

/** 普通对象（不是 `null`、不是数组）。 */
export const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);

/** `required` 一格不缺，其余的键只许在 `optional` 里（多一格不收）。 */
export function optionalKeys(v: Record<string, unknown>, required: readonly string[], optional: readonly string[]): boolean {
  const got = Object.keys(v);
  return required.every((k) => got.includes(k)) && got.every((k) => required.includes(k) || optional.includes(k));
}

/** 键集合恰好是 `keys`（多一格 / 缺一格都不收，顺序不论）。 */
export function exactKeys(v: Record<string, unknown>, keys: readonly string[]): boolean {
  return optionalKeys(v, keys, []);
}
