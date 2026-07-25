// account-ux U1：账号视觉身份——名字 → 稳定色 slot（0–7）。
//
// 账号名是 manifest 的稳定 key ⇒ 用它 hash 出 slot，颜色永远粘着实体：加账号不重排、
// 排名变化不换色。slot → 具体 CSS 色 token（--acct-cN / --acct-inkN）的映射留 U4/U5
// （视觉落地时在 styles.css + tabs/chip 里用）。本模块只做确定性 hash，纯函数、vitest 锁死。
//
// 选 FNV-1a 32-bit：简单、无依赖、分布够均匀（头像撞色只是两账号同色，缩写不同仍可分）。
import { badgeText } from "./accounts";

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

// ------------------------------------------------------------ 视图 helper（U4/U5 共用）
/**
 * account-ux U4：账号头像元素——圆角方块 `.acct-avatar.acct-c<slot>` + 缩写（badgeText）。
 * 不透明填充 + 自带 ink（styles.css 的 --acct-cN/--acct-inkN），主题无关。chip / tab 徽章共用。
 * size 可选覆盖边长（px，默认走 CSS）。ghost=true 走幽灵态（U5：lastAccount 软来源，描边淡填充）。
 */
export function accountAvatarEl(
  name: string,
  opts: { size?: number; ghost?: boolean } = {},
): HTMLSpanElement {
  const slot = accountColorSlot(name);
  const el = document.createElement("span");
  el.className = `acct-avatar acct-c${slot}${opts.ghost ? " ghost" : ""}`;
  el.textContent = badgeText(name);
  el.setAttribute("aria-hidden", "true");
  if (opts.size) {
    el.style.width = `${opts.size}px`;
    el.style.height = `${opts.size}px`;
    el.style.fontSize = `${Math.max(8, Math.round(opts.size * 0.5))}px`;
  }
  return el;
}
