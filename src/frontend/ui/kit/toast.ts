/**
 * toast（C13）：操作回执 ＋ 撤销提示条（I11）。全产品只这一处。
 *
 * - 右下角堆叠，最多 3 条；第 4 条进来最老的那条收进「消息」记录（[`recentToasts`]，本次运行内最近 20 条，不落盘）。
 * - 一条 ＝ 一句话（＋ 一行灰字细节）＋ 至多一个动作按钮（撤销 · 查看 · 重试）＋ 关闭 ×；左侧状态图标。
 * - 停留：纯告知 4s；带动作 8s；出错不自己走（点 × 或做了动作才走）；鼠标悬停或键盘焦点在上面时停表。
 * - 同类合流：同一级别同一标题连发并成一条 `×N`，明细不丢（展开看每一条）。带动作的各自一条。
 * - toast 不是唯一入口：里面的动作在别处也必须做得到（调用方的责任，评审看）。
 */
import { button } from "./button";
import { icon, type IconName } from "./icon";
import { copyText } from "../copy-table";
import s from "./toast.module.css";

export type ToastLevel = "success" | "info" | "warn" | "error";

export interface ToastOptions {
  level?: ToastLevel;
  /** 点这条（不是点按钮）做的事，例如打开日志。 */
  onClick?: () => void;
  /** 动作按钮（一颗或几颗）：点了做它、收起这条，不再走 `onExpire`。 */
  action?: ToastAction | ToastAction[];
  /** 到点自己走（或被 × 掉）之后：撤销期过了，做那一步剩下的事。 */
  onExpire?: () => void;
  /** 逐条明细：不上 toast，只进记录（在「消息」里展开那一条看）。 */
  more?: readonly string[];
}

export interface ToastAction {
  label: string;
  run: () => void;
  /** 只在提示条上出：它做的就是打开「消息」里的这一条，在「消息」里不再出一遍。 */
  toastOnly?: boolean;
}

export const TOAST_PLAIN_MS = 4000;
export const TOAST_ACTION_MS = 8000;
export const TOAST_MAX_VISIBLE = 3;
export const TOAST_RECORD_MAX = 20;

export interface ToastRecord {
  level: ToastLevel;
  title: string;
  detail: string;
  count: number;
  /** 出来那一刻（毫秒）。 */
  at: number;
  /** 还能做的动作（「重试」「查看」）；撤销期过了的「撤销」不在这里。 */
  actions: ToastAction[];
  /** 出错的那几条：在「消息」里看过没有。 */
  seen: boolean;
  /** 逐条明细（`ToastOptions.more`）；没有 ⇒ 空。 */
  more: readonly string[];
}

const listeners = new Set<() => void>();

/** 「消息」那一枚跟着记录变（新的一条进来 · 撤销期过了 · 看过了）。返回退订。 */
export function onToastRecords(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function changed(): void {
  for (const cb of listeners) cb();
}

/** 有没有还没在「消息」里看过的出错提示。 */
export function unseenErrors(): boolean {
  return record.some((r) => r.level === "error" && !r.seen);
}

/** 在「消息」里点一条的动作：那条 toast 还开着就一起收起（撤销 ＝ 不再提交）；做一次就从记录里摘掉。 */
export function runRecordAction(r: ToastRecord, a: ToastAction): void {
  const t = live.find((x) => x.record === r);
  if (t) drop(t, false);
  r.actions = r.actions.filter((x) => x !== a);
  changed();
  a.run();
}

/** 打开「消息」⇒ 那几条都算看过。 */
export function markRecordsSeen(): void {
  let any = false;
  for (const r of record) if (!r.seen) any = r.seen = true;
  if (any) changed();
}

const STACK_ID = "kit-toast-stack";
const LEVEL_ICON: Record<ToastLevel, IconName> = { success: "success", info: "info", warn: "warning", error: "error" };

interface Live {
  key: string;
  el: HTMLElement;
  details: string[];
  detailEl: HTMLElement;
  countEl: HTMLElement;
  record: ToastRecord;
  remaining: number;
  started: number;
  timer: ReturnType<typeof setTimeout> | null;
  paused: boolean;
  onExpire?: () => void;
  /** 撤销提示条的那一步（`Ctrl+Z` 撤最新的一条）。 */
  undo?: () => void;
}

const live: Live[] = [];
const record: ToastRecord[] = [];
let actionSeq = 0;

/** 「消息」里能找回的最近几条（新的在前）。 */
export function recentToasts(): readonly ToastRecord[] {
  return record;
}

function remember(r: ToastRecord): void {
  record.unshift(r);
  record.length = Math.min(record.length, TOAST_RECORD_MAX);
  changed();
}

function stack(): HTMLElement {
  let st = document.getElementById(STACK_ID);
  if (!st) {
    st = document.createElement("div");
    st.id = STACK_ID;
    st.className = s.toastStack;
    st.setAttribute("role", "region");
    st.setAttribute("aria-label", copyText("kit.toast.region"));
    document.body.appendChild(st);
  }
  return st;
}

/**
 * 把一句写进 `el`：按「 · 」分段，每段一个不拆行的 span（只在段与段之间换行）；多行的逐行照排。
 * 一段本身比整条还宽 ⇒ 那一段照常折（`overflow-wrap` 兜底，不溢出）。
 */
export function setSegmented(el: HTMLElement, text: string): void {
  const sep = copyText("kit.text.sep");
  el.replaceChildren();
  if (text === "") return; // 空的那一行留空（CSS 的 `:empty` 把它收起）
  text.split("\n").forEach((line, i) => {
    if (i > 0) el.appendChild(document.createElement("br"));
    line.split(sep).forEach((seg, j) => {
      if (j > 0) el.appendChild(document.createTextNode(sep));
      const span = document.createElement("span");
      span.className = s.toastSeg;
      span.textContent = seg;
      el.appendChild(span);
    });
  });
}

function lifetime(level: ToastLevel, hasAction: boolean): number | null {
  if (level === "error") return null;
  return hasAction ? TOAST_ACTION_MS : TOAST_PLAIN_MS;
}

function drop(t: Live, expired: boolean): void {
  const i = live.indexOf(t);
  if (i < 0) return;
  live.splice(i, 1);
  if (t.timer !== null) clearTimeout(t.timer);
  t.el.remove();
  if (expired && t.undo) {
    // 撤销期过了（那一步已经提交）⇒ 「消息」里不再给「撤销」。
    t.record.actions = t.record.actions.filter((a) => a.run !== t.undo);
    changed();
  }
  if (expired) t.onExpire?.();
}

function arm(t: Live): void {
  if (t.timer !== null) clearTimeout(t.timer);
  t.timer = null;
  if (t.paused || !Number.isFinite(t.remaining)) return;
  t.started = Date.now();
  // 调度：一次性 —— 到点收起这一条；悬停 / 焦点时清掉、离开后按剩下的时间重排
  t.timer = setTimeout(() => drop(t, true), t.remaining);
}

function pause(t: Live, on: boolean): void {
  if (on === t.paused) return;
  t.paused = on;
  if (on && t.timer !== null) {
    clearTimeout(t.timer);
    t.timer = null;
    t.remaining -= Date.now() - t.started;
  }
  if (!on) arm(t);
}

/**
 * 出一条 toast。`title` 一句话（≤40 字）、`detail` 一行灰字（可空）。
 * 返回收起它的函数（做了动作之外的路要提前收时用）。
 */
export function toast(title: string, detail: string, opts: ToastOptions = {}): () => void {
  const level = opts.level ?? "error";
  // 带动作 · 带逐条明细的各自一条（明细要原样留在自己那条记录上）。
  const key = opts.action || opts.more?.length ? `action\u0000${++actionSeq}` : `${level}\u0000${title}`;
  const same = live.find((t) => t.key === key);
  if (same) {
    same.details.push(detail);
    same.record.count += 1;
    same.record.detail = detail;
    setSegmented(same.detailEl, detail);
    same.countEl.textContent = copyText("kit.toast.count", { n: same.record.count });
    same.countEl.hidden = false;
    same.el.title = same.details.join("\n");
    same.remaining = lifetime(level, false) ?? Number.POSITIVE_INFINITY;
    arm(same);
    return () => drop(same, false);
  }

  const el = document.createElement("div");
  el.className = s.toast;
  el.dataset.level = level;
  el.setAttribute("role", level === "error" ? "alert" : "status");
  const text = document.createElement("div");
  text.className = s.toastText;
  const head = document.createElement("div");
  head.className = s.toastTitle;
  setSegmented(head, title);
  const countEl = document.createElement("span");
  countEl.className = s.toastCount;
  countEl.dataset.part = "count";
  countEl.hidden = true;
  head.appendChild(countEl);
  const detailEl = document.createElement("div");
  detailEl.className = s.toastDetail;
  detailEl.dataset.part = "detail";
  setSegmented(detailEl, detail);
  text.append(head, detailEl);
  el.append(icon(LEVEL_ICON[level]), text);

  const acts0 = opts.action === undefined ? [] : Array.isArray(opts.action) ? opts.action : [opts.action];
  const rec: ToastRecord = { level, title, detail, count: 1, at: Date.now(), actions: acts0, seen: level !== "error", more: opts.more ?? [] };
  const t: Live = {
    key,
    el,
    details: [detail],
    detailEl,
    countEl,
    record: rec,
    remaining: lifetime(level, opts.action !== undefined) ?? Number.POSITIVE_INFINITY,
    started: 0,
    timer: null,
    paused: false,
    onExpire: opts.onExpire,
  };

  const acts = document.createElement("div");
  acts.className = s.toastActions;
  for (const act of opts.action === undefined ? [] : Array.isArray(opts.action) ? opts.action : [opts.action]) {
    acts.appendChild(
      button({
        label: act.label,
        kind: "ghost",
        size: "compact",
        onClick: (ev) => {
          ev.stopPropagation();
          drop(t, false);
          act.run();
        },
      }),
    );
  }
  acts.appendChild(
    button({
      label: copyText("kit.toast.close"),
      kind: "icon",
      size: "compact",
      icon: "close",
      onClick: (ev) => {
        ev.stopPropagation();
        drop(t, true);
      },
    }),
  );
  el.appendChild(acts);
  if (opts.onClick) {
    const cb = opts.onClick;
    el.dataset.act = "click";
    el.addEventListener("click", () => {
      cb();
      drop(t, false);
    });
  }
  el.addEventListener("mouseenter", () => pause(t, true));
  el.addEventListener("mouseleave", () => pause(t, el.contains(document.activeElement)));
  el.addEventListener("focusin", () => pause(t, true));
  el.addEventListener("focusout", (ev) => pause(t, el.contains(ev.relatedTarget as Node | null) || el.matches(":hover")));

  stack().prepend(el);
  live.push(t);
  remember(rec);
  while (live.length > TOAST_MAX_VISIBLE) drop(live[0], true);
  arm(t);
  return () => drop(t, false);
}

/**
 * 撤销提示条：做完一件撤得回的事之后出 `已删除 X ［撤销］`，8 秒。
 * 撤销 ⇒ 调 `undo`；没撤（到点 · 点 ×）⇒ 调 `commit`（推荐做法：界面延后 8 秒再真提交，撤销 ＝ 不提交）。
 */
export function undoToast(title: string, undo: () => void, commit: () => void): () => void {
  const dismiss = toast(title, "", { level: "success", action: { label: copyText("kit.toast.undo"), run: undo }, onExpire: commit });
  const t = live[live.length - 1];
  if (t) t.undo = undo;
  return dismiss;
}

/** `Ctrl+Z`：撤最新那一条还开着的撤销提示（收起它、不再提交）。没有 ⇒ `false`。 */
export function undoLatest(): boolean {
  for (let i = live.length - 1; i >= 0; i--) {
    const t = live[i];
    if (!t.undo) continue;
    const undo = t.undo;
    drop(t, false);
    undo();
    return true;
  }
  return false;
}
