/**
 * 反空真：人群 / 金样 / 抽出来的那一份是空的 ⇒ 当场抛。紧跟着的相等或零命中判据就不会对着空集绿。
 * 删除条件：TS 的扫描判据有了统一入口、由入口自己在人群为空时红。
 */
export function nonEmpty<T extends { readonly length: number } | { readonly size: number }>(xs: T): T {
  if (((xs as { readonly length?: number }).length ?? (xs as { readonly size: number }).size) === 0) throw new Error("空的：后面的判据会零命中地绿");
  return xs;
}
