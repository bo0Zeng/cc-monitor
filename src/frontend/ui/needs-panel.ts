/**
 * 侧栏顶上「需手动 n」点开的那个浮层（400 宽）：在等的会话 · 计划项 · 一条分隔线 · 会话外的每一项（各台 `sessions-needs` 本机合好的那一份，照它的序）；
 * 底部断开 / 清单无法读取的那几台各一条虚线。没有小标题：会话行是琥珀点 ＋ 标题，会话外那几行是号 / MCP 图标，靠形就分得开。
 *
 * 这里只排版：会话行第二行是那台写好的会走的那一句（[`waitingNow`]），agent 发的多一行谁要批（`needs.ask.run.text`）；会话外那一行第一行是后端写好的 `said`、第二行是问来那台的机器名
 * （`fix` 为空 ⇒ 后端给的 `why`：为什么没有解法 / 怎么手动修，不画按钮）。点会话行 ⇒ 切过去；点会话外那一行或它的［登录…］⇒ 开登录抽屉。↑↓ 选行 · Enter ＝ 点 · Esc 收起（浮层那一套）。
 */
import { copyText } from "./copy-table";
import { button } from "./kit/button";
import { tag } from "./kit/badge";
import { icon, type IconName } from "./kit/icon";
import { statusDot } from "./kit/status-dot";
import { machineName } from "./ipc/chan-caller";
import { isRemoteOrigin, type Machine } from "./ipc/origin";
import type { ManualItem } from "./login-reads";
import type { BookSession, PlanNeedItem } from "./needs-book";
import type { NeedsCell } from "./generated/NeedsCell";
import { clockNow } from "./duration-format";
import s from "./needs-panel.module.css";

/**
 * 会话那一行第二行此刻的字：那台写好的会走的那一句（`clock.text`），已等 ＝ `waitedMs`（那台答出那一刻）＋ 本机从收到这一份起走过的
 * （当成起点在 `receivedAt − waitedMs` 的会走的钟交 `clockNow` 填，不跨机器减）；没有 ⇒ 那台写的种类那一句（`needs.text`）。
 */
export function waitingNow(b: Pick<BookSession, "session" | "clock" | "receivedAt">, now: number): string {
  const n = b.session.needs;
  return b.clock && n.waitedMs !== null ? clockNow({ text: b.clock.text, from: b.receivedAt - n.waitedMs }, now) : n.text;
}

/** 会话外那一行的图标：号 · MCP · 权限太宽的秘密文件。 */
const MANUAL_ICON: Record<ManualItem["kind"], IconName> = { accountLogin: "account", mcpLogin: "plug", secretFile: "warning" };

/** 一行里几格之间那一个分隔。 */
const SEP = copyText("kit.text.sep");

/** 一个在等的会话（已照「需手动」的序排好）。 */
export interface PanelSession {
  sid: string;
  origin: Machine;
  title: string;
  /** `等批准 · 已等 4 分钟` 那一句（[`waitingNow`]）。 */
  state: string;
  /** 在等的那一件（命令里说得出用意的那一段 · 问题）；没有 ⇒ `null`。 */
  what: string | null;
  /** 谁要批（agent 发的那一条，后端写好的 `needs.ask.run.text`）；主会话 ⇒ `null`。 */
  by: string | null;
}

/** 计划那一侧的一条（`NeedsBook.plans()` 第 `i` 条）：落在哪一格（没有 ⇒ `null`）· 哪一片 · 哪个工作区 · 哪台。 */
export interface PanelPlan {
  origin: Machine;
  workspace: string;
  slice: string;
  cell: NeedsCell | null;
  i: number;
}

/** 计划项 ⇒ 浮层那几行：一条一行（点它去那一条的格）。 */
export function planRows(items: readonly PlanNeedItem[]): PanelPlan[] {
  return items.map((it, i) => ({ origin: it.origin, workspace: it.workspace, slice: it.slice, cell: it.cell, i }));
}

export interface PanelFacts {
  sessions: readonly PanelSession[];
  plans: readonly PanelPlan[];
  manual: readonly { origin: Machine; item: ManualItem }[];
  /** 这一问没问到的那几台（不计；那一句在悬停里）。 */
  offline: readonly { origin: Machine; said: string }[];
  /** 会话外那一截读不出的那几台（那台那一句）。 */
  unreadable: readonly { origin: Machine; said: string }[];
}

export interface PanelActs {
  answer(sid: string): void;
  plan(i: number): void;
  login(origin: Machine, item: ManualItem): void;
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
    const r = row(statusDot("needs-you", copyText("tabBar.needs.label"), "compact"));
    r.dataset.row = "session";
    const l1 = lineOne();
    if (isRemoteOrigin(t.origin)) l1.appendChild(tag(t.origin));
    l1.appendChild(titled(t.title));
    const l2 = lineTwo();
    l2.appendChild(plain(t.state));
    if (t.what) l2.append(plain(SEP), coded(t.what));
    r.main.append(l1, l2);
    if (t.by) {
      // agent 发的那一条：谁要批单起一行（长标签只截它自己，不挤第二行的命令）。
      const l3 = lineTwo();
      l3.dataset.part = "by";
      l3.textContent = t.by;
      r.main.append(l3);
    }
    add(r.el, () => acts.answer(t.sid));
  }
  for (const p of f.plans) {
    const r = row(icon("plan", "compact"));
    r.dataset.row = "plan";
    // 第一行是那一格的「编号 标题」（后端写好）；没有格可落 ⇒ 片名，第二行就不再写片名。
    const l1 = lineOne();
    l1.appendChild(titled(p.cell?.label ?? p.slice));
    const l2 = lineTwo();
    l2.textContent = [...(p.cell ? [p.slice] : []), p.workspace, ...(isRemoteOrigin(p.origin) ? [p.origin] : [])].join(SEP);
    r.main.append(l1, l2);
    add(r.el, () => acts.plan(p.i));
  }

  const items = f.manual;
  if (rows.length > 0 && items.length > 0) root.appendChild(rule());
  for (const { origin, item } of items) {
    const r = row(icon(MANUAL_ICON[item.kind], "compact"));
    r.dataset.row = "manual";
    r.icon.className = `${s.npLead} ${s.npKindIcon}`;
    const l1 = lineOne();
    l1.appendChild(titled(item.said));
    const l2 = lineTwo();
    const why = item.kind !== "accountLogin" && item.fix === null ? (item.why ?? "") : "";
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

  for (const d of f.offline) {
    const n = note(copyText("needs.panel.offline", { machine: machineName(d.origin) }));
    n.dataset.note = "offline";
    n.title = d.said;
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
