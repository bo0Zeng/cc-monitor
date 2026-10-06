/**
 * 额度与账号几处共用的小块 DOM：悬停卡那张表 · 号的头像（`_` ＝ 虚线 `~`）。样子住 `acct.module.css`。
 * 只给主窗口用（带一份 CSS Module，理由同 `record-file-notice.ts` 头注）。
 */
import { accountAvatarEl } from "./account-color";
import { copyText } from "./copy-table";
import s from "./acct.module.css";

/** 号的头像：`_`（起会话时没说是哪个号）画虚线框 `~`。 */
export function acctAvatar(account: string, size = 16): HTMLElement {
  if (account !== "_") return accountAvatarEl(account, { size });
  const el = document.createElement("span");
  el.className = s.acctHomeAvatar;
  el.textContent = copyText("acct.home.avatar");
  el.style.width = `${size}px`;
  el.style.height = `${size}px`;
  el.setAttribute("aria-hidden", "true");
  return el;
}

/**
 * 悬停卡：首行 `头像 名 类型 [标签…]`，其余每行四格（键 · 值 · ↻ · 距今）对齐成表。行是排版模型给的字（`acct-view.ts`），这里只摆。
 * `tones` ＝ 按行标的色（`refused` 红 · `warn` 琥珀），没给的中性。
 */
export function hoverTable(account: string | null, rows: readonly (readonly string[])[], tones: Readonly<Record<number, "refused" | "warn">> = {}): HTMLElement {
  const root = document.createElement("div");
  root.className = s.acctHover;
  const [head, ...body] = rows;
  const h = document.createElement("div");
  h.className = s.acctHoverHead;
  if (account !== null) h.appendChild(acctAvatar(account));
  head?.forEach((cell, i) => {
    const c = document.createElement("span");
    c.className = i === 0 ? s.acctHoverName : s.acctHoverTag;
    c.textContent = cell;
    h.appendChild(c);
  });
  const grid = document.createElement("div");
  grid.className = s.acctHoverGrid;
  body.forEach((r, i) => {
    for (let k = 0; k < 4; k++) {
      const c = document.createElement("span");
      c.className = k === 0 ? s.acctHoverKey : s.acctHoverCell;
      c.textContent = r[k] ?? "";
      const tone = tones[i + 1];
      if (tone && k > 0) c.dataset.shade = tone;
      grid.appendChild(c);
    }
  });
  root.append(h, grid);
  return root;
}
