/**
 * 侧栏顶上「需手动 n」点开的那个浮层（400 宽）：在等的会话 · 计划项 · 一条分隔线 · 会话外的每一项（各台 `sessions-needs` 本机合好的那一份，照它的序）；
 * 底部断开 / 清单无法读取的那几台各一条虚线。没有小标题：会话行是琥珀点 ＋ 标题，会话外那几行是号 / MCP 图标，靠形就分得开。
 *
 * 这里只排版：会话行的字照会话事实那一份（`session-face.ts`）；会话外那一行第一行是后端写好的 `said`、第二行是问来那台的机器名
 * （`fix` 为空 ⇒ 后端给的 `why`，不画按钮）。点会话行 ⇒ 切过去；点会话外那一行或它的［登录…］⇒ 开登录抽屉。↑↓ 选行 · Enter ＝ 点 · Esc 收起（浮层那一套）。
 */
import { copyText } from "./copy-table";
import { button } from "./kit/button";
import { tag, countBadge } from "./kit/badge";
import { icon } from "./kit/icon";
import { statusDot } from "./kit/status-dot";
import { machineName } from "./ipc/chan-caller";
import { isRemoteOrigin, type Origin } from "./ipc/origin";
import type { ManualItem } from "./login-reads";
import s from "./needs-panel.module.css";

/** 一行里几格之间那一个分隔。 */
const SEP = copyText("kit.text.sep");

/** 一个在等的会话（已照「需手动」的序排好）。 */
export interface PanelSession {
  sid: string;
  origin: Origin;
  title: string;
  /** `等批准 · 已等 4 分钟` 那一句。 */
  state: string;
  /** 在等的那一件（命令 · 问题）；没有 ⇒ `null`。 */
  what: string | null;
}

/** 计划那一侧的一片（几条需手动 · 去那一片的第一条）。 */
export interface PanelPlan {
  origin: Origin;
  workspace: string;
  slice: string;
  n: number;
  first: number;
}

export interface PanelFacts {
  sessions: readonly PanelSession[];
  plans: readonly PanelPlan[];
  manual: readonly { origin: Origin; item: ManualItem }[];
  /** 断开的那几台（不计）。 */
  offline: readonly Origin[];
  /** 会话外那一截读不出的那几台（那台那一句）。 */
  unreadable: readonly { origin: Origin; said: string }[];
}

export interface PanelActs {
  answer(sid: string): void;
  plan(i: number): void;
  login(origin: Origin, item: ManualItem): void;
}

/** 画一份浮层内容（`selected` ＝ 键盘选着第几行；浮层开着时数据变了按同一个下标重画）。 */
export function renderNeedsPanel(f: PanelFacts, acts: PanelActs, selected: number): { root: HTMLElement; rows: number } {
  const root = document.createElement("div");
  root.className = s.npPanel;
  root.tabIndex = 0;
  root.setAttribute("role", "listbox");
  root.setAttribute("aria-label", copyText("tabBar.needs.label"));
  const rows: { el: HTMLElement; go: () => void }[] = [];
  const add = (el: HTMLElement, go: () => void): void => {
    el.addEventListener("click", go);
    el.addEventListener("mousemove", () => pick(rows.findIndex((r) => r.el === el)));
    rows.push({ el, go });
    root.appendChild(el);
  };

  for (const t of f.sessions) {
    const r = row(statusDot("needs-you", copyText("sessionFace.dot.needs"), "compact"));
    r.dataset.row = "session";
    const l1 = lineOne();
    if (isRemoteOrigin(t.origin)) l1.appendChild(tag(t.origin));
    l1.appendChild(titled(t.title));
    const l2 = lineTwo();
    l2.appendChild(plain(t.state));
    if (t.what) l2.append(plain(SEP), coded(t.what));
    r.main.append(l1, l2);
    add(r.el, () => acts.answer(t.sid));
  }
  for (const p of f.plans) {
    const r = row(icon("plan", "compact"));
    r.dataset.row = "plan";
    const l1 = lineOne();
    l1.appendChild(titled(p.slice));
    const badge = p.n > 1 ? countBadge(p.n, "warn") : null;
    if (badge) l1.appendChild(badge);
    const l2 = lineTwo();
    l2.textContent = isRemoteOrigin(p.origin) ? [p.workspace, p.origin].join(SEP) : p.workspace;
    r.main.append(l1, l2);
    add(r.el, () => acts.plan(p.first));
  }

  const items = f.manual;
  if (rows.length > 0 && items.length > 0) root.appendChild(rule());
  for (const { origin, item } of items) {
    const r = row(icon(item.kind === "mcpLogin" ? "plug" : "account", "compact"));
    r.dataset.row = "manual";
    r.icon.className = `${s.npLead} ${s.npKindIcon}`;
    const l1 = lineOne();
    l1.appendChild(titled(item.said));
    const l2 = lineTwo();
    const why = item.kind === "mcpLogin" && item.fix === null ? (item.why ?? "") : "";
    l2.textContent = why ? [machineName(origin), why].join(SEP) : machineName(origin);
    r.main.append(l1, l2);
    if (item.fix === null) {
      // 没有解法：不画按钮，这一行也不能点。
      r.el.dataset.inert = "true";
      root.appendChild(r.el);
      continue;
    }
    const go = button({ label: copyText("newSession.account.login"), size: "compact", onClick: (ev) => {
      ev.stopPropagation();
      acts.login(origin, item);
    } });
    go.dataset.action = "manual-login";
    go.tabIndex = -1;
    r.el.appendChild(go);
    add(r.el, () => acts.login(origin, item));
  }

  if (rows.length === 0 && root.querySelector("[data-inert]") === null) root.appendChild(empty());

  for (const origin of f.offline) {
    const n = note(copyText("needs.panel.offline", { machine: machineName(origin) }));
    n.dataset.note = "offline";
    root.appendChild(n);
  }
  for (const u of f.unreadable) {
    const n = note(copyText("needs.panel.unreadable", { machine: machineName(u.origin) }));
    n.dataset.note = "unreadable";
    // 那台那一句在悬停里。
    n.title = u.said;
    root.appendChild(n);
  }

  let at = -1;
  function pick(i: number): void {
    if (i === at) return;
    rows[at]?.el.removeAttribute("aria-selected");
    at = i;
    rows[at]?.el.setAttribute("aria-selected", "true");
    root.dataset.at = String(at);
  }
  pick(rows.length === 0 ? -1 : Math.max(0, Math.min(selected, rows.length - 1)));
  root.addEventListener("keydown", (ev) => {
    if (ev.isComposing || rows.length === 0) return;
    if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      ev.preventDefault();
      const d = ev.key === "ArrowDown" ? 1 : -1;
      pick((at + d + rows.length) % rows.length);
      rows[at].el.scrollIntoView?.({ block: "nearest" });
    } else if (ev.key === "Enter") {
      ev.preventDefault();
      rows[at]?.go();
    }
  });
  return { root, rows: rows.length };
}

function row(lead: Element): { el: HTMLElement; main: HTMLElement; icon: Element; dataset: DOMStringMap } {
  const el = document.createElement("div");
  el.className = s.npRow;
  el.setAttribute("role", "option");
  const ic = document.createElement("span");
  ic.className = s.npLead;
  ic.appendChild(lead);
  const main = document.createElement("div");
  main.className = s.npMain;
  el.append(ic, main);
  return { el, main, icon: ic, dataset: el.dataset };
}

function lineOne(): HTMLElement {
  const d = document.createElement("div");
  d.className = s.npL1;
  return d;
}

function lineTwo(): HTMLElement {
  const d = document.createElement("div");
  d.className = s.npL2;
  return d;
}

function plain(t: string): HTMLElement {
  const e = document.createElement("span");
  e.textContent = t;
  return e;
}

function titled(t: string): HTMLElement {
  const e = plain(t);
  e.className = s.npTitle;
  return e;
}

function coded(t: string): HTMLElement {
  const e = plain(t);
  e.className = s.npCode;
  return e;
}

function note(t: string): HTMLElement {
  const e = plain(t);
  e.className = s.npNote;
  return e;
}

function rule(): HTMLElement {
  const e = plain("");
  e.className = s.npSep;
  return e;
}

function empty(): HTMLElement {
  const e = plain(copyText("needs.panel.empty"));
  e.className = s.npEmpty;
  return e;
}
