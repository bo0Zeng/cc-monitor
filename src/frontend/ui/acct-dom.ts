/**
 * 额度与账号几处共用的小块 DOM：悬停卡那张表 · 号的头像（`_` ＝ 虚线 `~`）。样子住 `acct.module.css`。
 * 只给主窗口用（带一份 CSS Module，理由同 `record-file-notice.ts` 头注）。
 */
import { accountAvatarEl } from "./account-color";
import { copyText } from "./copy-table";
import type { QuotaCell } from "./generated/QuotaCell";
import type { Machine } from "./ipc/origin";
import s from "./acct.module.css";

/** 号的头像：`_`（起会话时没说是哪个号）画虚线框 `~`；别的号照它那台（`origin`，缺 ⇒ 本机）账号库的号色画。 */
export function acctAvatar(account: string, size = 16, origin?: Machine): HTMLElement {
  if (account !== "_") return accountAvatarEl(account, { size, origin });
  const el = document.createElement("span");
  el.className = s.acctHomeAvatar;
  el.textContent = copyText("acct.home.avatar");
  el.style.width = `${size}px`;
  el.style.height = `${size}px`;
  el.setAttribute("aria-hidden", "true");
  return el;
}

/** 一格的语气上色：fail ⇒ 红 · warn ⇒ 琥珀；常规不染。 */
function shade(el: HTMLElement, tone: QuotaCell["tone"] | undefined): void {
  if (tone === "fail") el.dataset.shade = "refused";
  else if (tone === "warn") el.dataset.shade = "warn";
}

/**
 * 悬停卡：首行 `头像 名 类型 [标签…]`，其余每行四格（键 · 值 · ↻ · 距今）对齐成表。行是会话那台写好的格（`session-account` 成品），这里只摆：
 * 每格照它自己的语气上色（fail 红 · warn 琥珀，常规不染）。`hint` ＝ 号名那一格的悬停提示（几点采的）。
 */
export function hoverTable(account: string | null, rows: readonly (readonly QuotaCell[])[], hint?: string): HTMLElement {
  const root = document.createElement("div");
  root.className = s.acctHover;
  const [head, ...body] = rows;
  const h = document.createElement("div");
  h.className = s.acctHoverHead;
  if (account !== null) h.appendChild(acctAvatar(account));
  head?.forEach((cell, i) => {
    const c = document.createElement("span");
    c.className = i === 0 ? s.acctHoverName : s.acctHoverTag;
    c.textContent = cell.text;
    if (i === 0 && hint !== undefined) c.title = hint;
    h.appendChild(c);
  });
  const grid = document.createElement("div");
  grid.className = s.acctHoverGrid;
  body.forEach((r) => {
    for (let k = 0; k < 4; k++) {
      const c = document.createElement("span");
      c.className = k === 0 ? s.acctHoverKey : s.acctHoverCell;
      c.textContent = r[k]?.text ?? "";
      if (k > 0) shade(c, r[k]?.tone);
      grid.appendChild(c);
    }
  });
  root.append(h, grid);
  return root;
}
