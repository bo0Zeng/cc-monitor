/**
 * 历史页列表的 DOM（设计稿「文件与历史」乙4-③）：会话行（两层 52 高 / 按项目时一层紧凑）· 分段头 · 项目头 · 内容命中块 · 列表顶的提示条。
 * 只按后端给的事实画（状态 · 能做什么 · 标题都在行里），不判；动作经回调交给页面。
 */
import { button, setDisabled } from "../kit/button";
import { icon } from "../kit/icon";
import { tag } from "../kit/badge";
import { statusDot } from "../kit/status-dot";
import { banner } from "../kit/banner";
import { copyText } from "../copy-table";
import type { HistoryGroup, HistoryRow } from "../history-list-reads";
import type { Hit, SessionHits } from "./history-search";
import { rowTime } from "./history-time";
import s from "./history.module.css";

/** 一行的键（同一个会话在两台上是两行）。 */
export function rowKey(r: { origin?: string; sessionId: string }): string {
  return `${r.origin ?? ""}\u0000${r.sessionId}`;
}

/** 行上那几个动作（页面实现）。 */
export interface RowHooks {
  /** 鼠标选中这一行（右边跟着显示）。 */
  select(r: HistoryRow): void;
  resume(r: HistoryRow): void;
  menu(r: HistoryRow, at: HTMLElement | { x: number; y: number }): void;
  openWindow(r: HistoryRow): void;
  toggleForks(r: HistoryRow): void;
  /** 这一会话在主窗口里此刻「需要你」的那个词（等批准 …）；不在 / 不需要 ⇒ `null`。 */
  needs(r: HistoryRow): string | null;
}

/** 机器标签（与主窗口标签页上的同一个样子：图标 ＋ 名字）；本机不画。 */
export function machineTag(origin: string | undefined): HTMLElement | null {
  if (!origin) return null;
  const t = tag(origin);
  t.classList.add(s.hvMachine);
  t.prepend(icon("machine", "compact"));
  return t;
}

function badge(text: string, tone?: "live" | "need"): HTMLElement {
  const b = tag(text);
  if (tone) b.dataset.emph = tone;
  return b;
}

/** 状态点：只画在跑与需要你（`V10`）。 */
function dotOf(r: HistoryRow, needs: string | null): HTMLElement | null {
  if (needs) return statusDot("needs-you", needs, "compact");
  if (r.status === "live") return statusDot("running", copyText("history.row.live"), "compact");
  return null;
}

/** 标题那一格：改过的 ＞ 标题 ＞ 第一句；都没有 ⇒「（没有说过话的会话）」。 */
export function labelOf(r: HistoryRow): string {
  return r.untitled ? copyText("history.row.untitled") : r.label;
}

/** 行尾的［恢复］或［切过去］＋［⋯］（悬停 / 选中时换掉时间，占位不变）。 */
function actions(r: HistoryRow, h: RowHooks): HTMLElement {
  const a = document.createElement("span");
  a.className = s.hvActs;
  const live = r.can.resume === "switch";
  const go = button({
    label: live ? copyText("history.row.switch") : copyText("history.row.resume"),
    size: "compact",
    hint: live ? copyText("history.row.switchHint") : undefined,
    onClick: (ev) => {
      ev.stopPropagation();
      h.resume(r);
    },
  });
  if (r.can.resume === "bg") setDisabled(go, copyText("history.row.bgHint"));
  const more = button({
    label: copyText("history.row.more"),
    kind: "icon",
    icon: "more",
    size: "compact",
    hint: copyText("history.row.more"),
    onClick: (ev) => {
      ev.stopPropagation();
      h.menu(r, more);
    },
  });
  a.append(go, more);
  return a;
}

/**
 * 会话行。`compact` ＝ 按项目那一形（一层：标题 · 徽标 · 时间）；否则按时间那一形（两层）。
 * `forks` = 它下面挂着几个分叉（收着 / 展开）；`orphan` = 它是分叉、父会话不在清单里。
 */
export function sessionRow(
  r: HistoryRow,
  h: RowHooks,
  o: { compact: boolean; forks: number; forksOpen: boolean; child: boolean; orphan: boolean; now: number },
): HTMLElement {
  const el = document.createElement("div");
  el.className = s.hvRow;
  el.dataset.key = rowKey(r);
  el.dataset.compact = String(o.compact);
  if (o.child) el.dataset.child = "true";
  if (r.context) el.dataset.context = "true";
  el.setAttribute("role", "option");
  el.tabIndex = -1;
  const needs = h.needs(r);
  const dot = dotOf(r, needs);
  if (dot) {
    dot.classList.add(s.hvDot);
    el.appendChild(dot);
  }
  const l1 = document.createElement("div");
  l1.className = s.hvL1;
  if (r.starred) {
    const st = icon("starFill", "compact");
    st.classList.add(s.hvStar);
    l1.appendChild(st);
  }
  const t = document.createElement("span");
  t.className = s.hvTitle;
  t.textContent = labelOf(r);
  t.title = labelOf(r);
  if (r.untitled) t.dataset.untitled = "true";
  l1.appendChild(t);
  const badges: HTMLElement[] = [];
  if (r.agentTag) badges.push(badge(r.agentTag));
  if (r.isBg) badges.push(badge(copyText("history.row.bg")));
  if (r.hidden) badges.push(badge(copyText("history.row.hidden")));
  if (needs) badges.push(badge(needs, "need"));
  else if (o.compact && r.status === "live") badges.push(badge(copyText("history.row.live"), "live"));
  if (o.compact) l1.append(...badges);
  const tm = document.createElement("span");
  tm.className = s.hvTime;
  tm.textContent = rowTime(r.at, o.now);
  l1.append(tm, actions(r, h));
  el.appendChild(l1);
  if (!o.compact) {
    const l2 = document.createElement("div");
    l2.className = s.hvL2;
    const p = document.createElement("span");
    p.textContent = r.projectName || copyText("history.row.noDir");
    l2.appendChild(p);
    const m = machineTag(r.origin);
    if (m) l2.appendChild(m);
    l2.append(...badges);
    if (o.forks > 0) l2.appendChild(forkToggle(r, h, o.forks, o.forksOpen));
    if (o.orphan) {
      const f = document.createElement("span");
      f.textContent = copyText("history.row.orphan");
      l2.appendChild(f);
    }
    el.appendChild(l2);
  } else if (o.forks > 0) {
    l1.insertBefore(forkToggle(r, h, o.forks, o.forksOpen), tm);
  }
  el.addEventListener("click", () => h.select(r));
  el.addEventListener("dblclick", () => h.openWindow(r));
  el.addEventListener("auxclick", (ev) => {
    if (ev.button === 1) h.openWindow(r);
  });
  el.addEventListener("contextmenu", (ev) => {
    ev.preventDefault();
    h.select(r);
    h.menu(r, { x: ev.clientX, y: ev.clientY });
  });
  return el;
}

function forkToggle(r: HistoryRow, h: RowHooks, n: number, open: boolean): HTMLElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = s.hvForks;
  b.textContent = copyText("history.row.forks", { n });
  b.setAttribute("aria-expanded", String(open));
  b.addEventListener("click", (ev) => {
    ev.stopPropagation();
    h.toggleForks(r);
  });
  return b;
}

/** 按时间看的分段头（粘在顶上）。 */
export function sectionHead(label: string): HTMLElement {
  const el = document.createElement("div");
  el.className = s.hvSec;
  el.textContent = label;
  return el;
}

/** 按项目看的项目头：▸ 名字 · 机器标签 · 路径 · 右端「N 个会话」＋ 在跑的点；悬停出［＋ 新会话］。读不了的那一组写「加载失败［重试］」。 */
export function groupHead(
  g: HistoryGroup,
  o: { open: boolean; onToggle: () => void; onNew: () => void; onRetry: () => void },
): HTMLElement {
  const el = document.createElement("div");
  el.className = s.hvGroup;
  el.dataset.key = `${g.origin ?? ""}\u0000${g.key}`;
  el.setAttribute("role", "treeitem");
  el.setAttribute("aria-expanded", String(o.open));
  el.tabIndex = -1;
  el.appendChild(icon(o.open ? "caretDown" : "caretRight", "compact"));
  const n = document.createElement("span");
  n.className = s.hvGroupName;
  n.textContent = g.projectName || copyText("history.row.noDir");
  el.appendChild(n);
  const m = machineTag(g.origin);
  if (m) el.appendChild(m);
  const p = document.createElement("span");
  p.className = s.hvGroupPath;
  p.textContent = g.projectPath;
  p.title = g.projectPath;
  el.appendChild(p);
  if (g.failed !== null) {
    const f = document.createElement("span");
    f.className = s.hvGroupFailed;
    f.textContent = copyText("history.group.failed");
    f.title = g.failed;
    el.appendChild(f);
    el.appendChild(
      button({
        label: copyText("history.group.retry"),
        size: "compact",
        onClick: (ev) => {
          ev.stopPropagation();
          o.onRetry();
        },
      }),
    );
  } else {
    const c = document.createElement("span");
    c.className = s.hvGroupCount;
    c.textContent = copyText("history.group.count", { n: g.count });
    el.appendChild(c);
    if (g.hasLive) el.appendChild(statusDot("running", copyText("history.row.live"), "compact"));
    const add = button({
      label: copyText("history.group.newSession"),
      size: "compact",
      onClick: (ev) => {
        ev.stopPropagation();
        o.onNew();
      },
    });
    add.classList.add(s.hvGroupNew);
    el.appendChild(add);
  }
  el.addEventListener("click", () => o.onToggle());
  return el;
}

/** 内容搜索的一处命中：谁说的一颗小徽标 ＋ 一行原文（命中词加底色）。 */
function hitLine(hit: Hit, onOpen: () => void): HTMLElement {
  const el = document.createElement("div");
  el.className = s.hvHit;
  el.tabIndex = -1;
  const w = badge(whoOf(hit.kind));
  el.appendChild(w);
  const x = document.createElement("span");
  x.className = s.hvHitText;
  const mark = document.createElement("mark");
  mark.className = s.hvMark;
  mark.textContent = hit.matched;
  x.append(hit.before, mark, hit.after);
  el.appendChild(x);
  el.addEventListener("click", (ev) => {
    ev.stopPropagation();
    onOpen();
  });
  return el;
}

/** 命中种类 ⇒ 谁说的（`user` 你 · `assistant` 那一家 · `tool` 工具 · `report` agent 回报）。 */
function whoOf(kind: string): string {
  switch (kind) {
    case "user":
      return copyText("history.searchHit.you");
    case "tool":
      return copyText("history.searchHit.tool");
    case "report":
      return copyText("history.searchHit.report");
    default:
      return copyText("history.searchHit.agent");
  }
}

/** 每个会话最多摆几处命中（其余写「+N 处 ▸」，点了右边打开这个会话）。 */
export const HITS_SHOWN = 3;

/** 内容搜索的一块：会话行 ＋ 前几处命中 ＋「+N 处 ▸」。 */
export function hitsBlock(row: HTMLElement, sh: SessionHits, open: (uuid: string | null) => void): HTMLElement {
  const el = document.createElement("div");
  el.className = s.hvHits;
  el.appendChild(row);
  for (const hit of sh.hits.slice(0, HITS_SHOWN)) el.appendChild(hitLine(hit, () => open(hit.uuid)));
  const more = sh.hitCount - Math.min(sh.hits.length, HITS_SHOWN);
  if (more > 0) {
    const m = document.createElement("button");
    m.type = "button";
    m.className = s.hvMore;
    m.textContent = copyText("history.searchSession.more", { n: more });
    m.addEventListener("click", () => open(null));
    el.appendChild(m);
  }
  return el;
}

/** 列表顶的一条（连不上的那台 · 读不了 · 注解没并上 · Codex 只按标题）：警示条 ＋ 至多一颗按钮。 */
export function strip(tone: "warn" | "error" | "info", text: string, act?: { label: string; run: () => void }): HTMLElement {
  const acts = act ? [button({ label: act.label, size: "compact", onClick: act.run })] : [];
  if (tone === "info") {
    const el = document.createElement("div");
    el.className = s.hvInfo;
    el.append(icon("info", "compact"), text, ...acts);
    return el;
  }
  const b = banner(tone, text, acts);
  b.classList.add(s.hvStrip);
  return b;
}
