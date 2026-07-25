// account-ux U1：账号视觉身份——名字 → 稳定色 slot（0–7）。
//
// 账号名是 manifest 的稳定 key ⇒ 用它 hash 出 slot，颜色永远粘着实体：加账号不重排、
// 排名变化不换色。slot → 具体 CSS 色 token（--acct-cN / --acct-inkN）的映射留 U4/U5
// （视觉落地时在 styles.css + tabs/chip 里用）。本模块只做确定性 hash，纯函数、vitest 锁死。
//
// 选 FNV-1a 32-bit：简单、无依赖、分布够均匀（头像撞色只是两账号同色，缩写不同仍可分）。

/** 账号色板槽位数（与 styles.css 的 --acct-c0..c7 一一对应，U4/U5 落地）。 */
export const ACCOUNT_COLOR_SLOTS = 8;

/**
 * 账号名 → 稳定色槽位 [0, ACCOUNT_COLOR_SLOTS)。确定性、纯函数。
 * 空串也返回一个确定槽位（不抛）。用 UTF-16 code unit 迭代（CJK 亦确定）。
 */
export function accountColorSlot(name: string): number {
  let h = 0x811c9dc5; // FNV offset basis (32-bit)
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 0x01000193); // FNV prime，Math.imul 保 32-bit 溢出语义
  }
  return (h >>> 0) % ACCOUNT_COLOR_SLOTS; // >>>0 转无符号后取模
}
