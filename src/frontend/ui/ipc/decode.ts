/**
 * 线上来的东西在界面这一侧的解码：一段 JSON 文本 ⇒ JSON 值（界面里线上的 `JSON.parse` 住这里，命令回包那一处暂在 `chan-caller.ts::readJson`；
 * 浏览器本地 · 会话存储里的那一口在 `local-storage.ts::parseStored`）。
 * 加两端契约的严格收那两个小件：键集合恰好是哪几格 · 必有几格 ＋ 可缺几格。
 * 多一格 / 缺一格都不收；各读口按自己的说法抛，这里只答是 / 不是。界面里只有这一份。
 */

/** 线上来的一段 JSON 文本 ⇒ JSON 值。解不出来就抛（那是两端契约对不上，不是「空答案」）。 */
export function decodeJson(text: string): unknown {
  return JSON.parse(text);
}

/** 同 {@link decodeJson}，解不出来 ⇒ `null`（流里一格读不懂、跳过它的那几处用）。 */
export function tryDecodeJson<T>(text: string): T | null {
  try {
    return decodeJson(text) as T;
  } catch {
    return null;
  }
}

/** `required` 一格不缺，其余的键只许在 `optional` 里（多一格不收）。 */
export function optionalKeys(v: Record<string, unknown>, required: readonly string[], optional: readonly string[]): boolean {
  const got = Object.keys(v);
  return required.every((k) => got.includes(k)) && got.every((k) => required.includes(k) || optional.includes(k));
}

/** 键集合恰好是 `keys`（多一格 / 缺一格都不收，顺序不论）。 */
export function exactKeys(v: Record<string, unknown>, keys: readonly string[]): boolean {
  return optionalKeys(v, keys, []);
}
