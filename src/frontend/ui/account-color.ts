// 号色：账号库里的事实 —— 那台后端加号时分第一个空格、存进清单（`accounts/manage/model.rs::Manifest::with_colors`），
// `accounts-list` 每号带 `color`（令牌表 `--acct-c<格>` 的第几格）。界面只照它画，不按名字算（按名字哈希时 team / lab 撞同一格）。
// 那台没给这个号（清单还没问到 · 不在库里）⇒ 不猜：画中性的头像（不设 `--acct`）。
import { appStore } from "./app-store";
import { copyText } from "./copy-table";
import { LOCAL, type Machine } from "./ipc/origin";

/** 那台账号库里这个号那一行（问不到那一台时照它上次那一份）；不在库里 ⇒ `undefined`。 */
function rowOf(name: string, origin: Machine) {
  const st = appStore.accounts.get().get(origin);
  const accounts = st?.available ? st.accounts : (st?.last?.accounts ?? []);
  return accounts.find((a) => a.name === name);
}

/** 那台账号库给这个号的号色那一格；那台没给 ⇒ `null`。 */
export function accountColorOf(name: string, origin: Machine = LOCAL): number | null {
  return rowOf(name, origin)?.color ?? null;
}

/** 号牌上那一两个字（那台后端写好）；那台没给 ⇒ `—`（不在这里另取）。 */
export function accountPlateOf(name: string, origin: Machine = LOCAL): string {
  return rowOf(name, origin)?.plate ?? copyText("acct.val.none");
}

/**
 * 给一个元素挂上号色那一格：设 `--acct` / `--acct-ink` 为 tokens.css 那一格（`var(--acct-c<格>)` / `var(--acct-ink<格>)`），
 * CSS 只读这两个名字；`null` ⇒ 撤掉（中性）。号色有几格、各是什么色只在 tokens.css 写一次。
 */
export function paintAccountColor(el: HTMLElement, slot: number | null): void {
  if (slot === null) {
    el.style.removeProperty("--acct");
    el.style.removeProperty("--acct-ink");
    return;
  }
  el.style.setProperty("--acct", `var(--acct-c${slot})`);
  el.style.setProperty("--acct-ink", `var(--acct-ink${slot})`);
}

// ------------------------------------------------------------ 视图 helper
/**
 * 账号头像元素——圆角方块 `.acct-avatar`（号色照 {@link paintAccountColor}）+ 号牌字（那台后端写好的 `plate`）。
 * 不透明填充 + 自带 ink（tokens.css 那一格），主题无关。chip / tab 徽章共用。
 * `origin` ＝ 号在哪一台（缺省本机）；`color` · `plate` ＝ 那份成品自己带的号色 · 号牌字（给了就照它，不另查账号清单；号色 `null` ⇒ 中性）；size 可选覆盖边长（px，默认走 CSS）；ghost=true 走幽灵态（lastAccount 软来源，描边淡填充）。
 */
export function accountAvatarEl(
  name: string,
  opts: { size?: number; ghost?: boolean; origin?: Machine; color?: number | null; plate?: string } = {},
): HTMLSpanElement {
  const slot = opts.color !== undefined ? opts.color : accountColorOf(name, opts.origin);
  const el = document.createElement("span");
  el.className = `acct-avatar${opts.ghost ? " ghost" : ""}`;
  paintAccountColor(el, slot);
  el.textContent = opts.plate ?? accountPlateOf(name, opts.origin);
  el.setAttribute("aria-hidden", "true");
  if (opts.size) {
    el.style.width = `${opts.size}px`;
    el.style.height = `${opts.size}px`;
    el.style.fontSize = `${Math.max(8, Math.round(opts.size * 0.5))}px`;
  }
  return el;
}
