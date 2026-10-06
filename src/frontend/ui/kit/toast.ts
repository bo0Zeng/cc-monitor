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
}

export interface ToastAction {
  label: string;
  run: () => void;
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
  if (expired) t.onExpire?.();
}

function arm(t: Live): void {
  if (t.timer !== null) clearTimeout(t.timer);
  t.timer = null;
  if (t.paused || !Number.isFinite(t.remaining)) return;
  t.started = Date.now();
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
  const key = opts.action ? `action\u0000${++actionSeq}` : `${level}\u0000${title}`;
  const same = live.find((t) => t.key === key);
  if (same) {
    same.details.push(detail);
    same.record.count += 1;
    same.record.detail = detail;
    same.detailEl.textContent = detail;
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
  head.textContent = title;
  const countEl = document.createElement("span");
  countEl.className = s.toastCount;
  countEl.dataset.part = "count";
  countEl.hidden = true;
  head.appendChild(countEl);
  const detailEl = document.createElement("div");
  detailEl.className = s.toastDetail;
  detailEl.dataset.part = "detail";
  detailEl.textContent = detail;
  text.append(head, detailEl);
  el.append(icon(LEVEL_ICON[level]), text);

  const rec: ToastRecord = { level, title, detail, count: 1 };
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
  return toast(title, "", { level: "success", action: { label: copyText("kit.toast.undo"), run: undo }, onExpire: commit });
}
