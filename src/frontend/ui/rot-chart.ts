/**
 * 合图：一条轴上画本会话一行 ＋ 每号一行。
 *
 * - 全照 `rotation-plan` 的回答（后端逐时刻跑策略推演好的）：本会话那行 ＝ `plan` 的段（过去实色、将来淡色虚框、停发空虚框）＋
 *   换号点（◆ 已换 · ◇ 估，悬停写时刻 · 换到谁 · 为什么）；每号那行 ＝ `rows[].bars`（`k` 定样子：在用号色 · 到线琥珀斜纹 ·
 *   时段不用灰斜纹 · 被拒 / 用满红斜纹 · 此刻不挑淡斜纹）＋ 重置刻度。界面不推演、不判到线。
 * - 行序 ＝ 调用方给的那一份（会话面板：后端排好的有效优先级，第一行在用）；行头那一格由调用方建（号名 · 用法标 · 用量）。
 * - 行下一句：策略给的为什么（此刻不挑 ⇒ `跳过 · {why}`，整行淡）；别的会话也钉在它上面 ⇒ 琥珀一句。
 * 判据：`tests/frontend/ui/rot-chart.vitest.ts`。
 */
import { accountColorOf } from "./account-color";
import type { Origin } from "./ipc/origin";
import { accountLabel } from "./acct-words";
import { copyText } from "./copy-table";
import { segmented } from "./kit/tabs";
import { icon } from "./kit/icon";
import { attachTooltip } from "./kit/tooltip";
import type { PlanReply } from "./generated/PlanReply";
import s from "./rot-chart.module.css";

export type ChartView = "12h" | "24h";

/** 一号一行：行头（调用方建）· 为什么 · 此刻不挑 · 在用 · 别的会话也钉着几个 · 选中。 */
export interface ChartRow {
  account: string;
  head: HTMLElement;
  why: string;
  skip?: boolean;
  inuse?: boolean;
  alsoUsed: number;
  selected?: boolean;
}

export interface ChartSpec {
  plan: PlanReply | null;
  view: ChartView;
  onView: (v: ChartView) => void;
  /** 本会话那行（设置窗的预览没有会话 ⇒ 缺）。 */
  session?: { title: string; sub: string };
  rows: readonly ChartRow[];
  /** 号在哪一台（号色照那台账号库画）。 */
  origin: Origin;
  /** 行尾 `›`（号详情）。 */
  onOpen?: (account: string) => void;
  /** 列表底 `＋ 加号…`。 */
  onAdd?: () => void;
  /** 窄（面板 360）：行头窄一档。 */
  narrow?: boolean;
}

export interface Chart {
  root: HTMLElement;
  /** 每号那一行（拖 · 选中 · 右键由调用方接）。 */
  rowEls: Map<string, HTMLElement>;
}

/** 建一个元素：`cls` 里给它挂类（写成 `(e) => (e.className = s.x)`，叠没叠的量具认得出挂到哪）。 */
function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: (e: HTMLElementTagNameMap[K]) => void, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  cls(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

/** 号色（号色的字色随它走）：照那台账号库给的那一格；那台没给 ⇒ 中性。 */
function tint(e: HTMLElement, account: string, origin: Origin): void {
  const slot = accountColorOf(account, origin);
  e.style.setProperty("--rc-c", slot === null ? "var(--border-strong)" : `var(--acct-c${slot})`);
  e.style.setProperty("--rc-ink", slot === null ? "var(--text)" : `var(--acct-ink${slot})`);
}

interface Axis {
  pos(t: number): string;
  wid(a: number, b: number): string;
}

function axisOf(p: PlanReply): Axis {
  const span = Math.max(1, p.until - p.from);
  const x = (t: number): number => (Math.max(p.from, Math.min(p.until, t)) - p.from) / span;
  return {
    pos: (t) => `${(x(t) * 100).toFixed(3)}%`,
    wid: (a, b) => `${((x(b) - x(a)) * 100).toFixed(3)}%`,
  };
}

/** 此刻之前那一截的底色（最先画）。 */
function pastShade(track: HTMLElement, p: PlanReply, ax: Axis): void {
  const past = el("div", (e) => (e.className = s.rcPast));
  past.style.left = "0";
  past.style.width = ax.wid(p.from, p.now);
  track.appendChild(past);
}

/** 此刻那根竖线（最后画：压在段与刻度上面）。 */
function nowLine(track: HTMLElement, p: PlanReply, ax: Axis): void {
  const nv = el("div", (e) => (e.className = s.rcNow));
  nv.style.left = ax.pos(p.now);
  track.appendChild(nv);
}

function head(spec: ChartSpec): HTMLElement {
  const row = el("div", (e) => (e.className = s.rcHead));
  const lc = el("div", (e) => (e.className = s.rcLc));
  const sw = segmented<ChartView>({
    items: [
      { key: "12h", label: "12h" },
      { key: "24h", label: "24h" },
    ],
    current: spec.view,
    onChange: spec.onView,
    label: copyText("rot.chart.view"),
  });
  sw.dataset.rcView = spec.view;
  lc.appendChild(sw);
  const axis = el("div", (e) => (e.className = s.rcAxis));
  axis.dataset.rcAxis = "true";
  const p = spec.plan;
  if (p) {
    const ax = axisOf(p);
    // 离此刻太近的刻度字让给此刻那一格（不叠字）。
    const near = (p.until - p.from) * 0.07;
    for (const g of p.grid) {
      const t = el("span", (e) => (e.className = s.rcTick), g.label !== undefined && Math.abs(g.at - p.now) > near ? g.label : "");
      t.dataset.rcTick = g.atText;
      t.style.left = ax.pos(g.at);
      if (t.textContent === "") t.dataset.minor = "true";
      axis.appendChild(t);
    }
    const now = el("span", (e) => (e.className = s.rcNowText), p.nowText);
    now.dataset.rcNow = "true";
    now.style.left = ax.pos(p.now);
    axis.appendChild(now);
  }
  row.append(lc, axis, el("div", (e) => (e.className = s.rcGo)));
  return row;
}

function sessionRow(spec: ChartSpec, sess: { title: string; sub: string }): HTMLElement {
  const row = el("div", (e) => (e.className = s.rcRow));
  row.dataset.rcSession = "true";
  const lc = el("div", (e) => (e.className = s.rcLc));
  lc.append(el("div", (e) => (e.className = s.rcL1Title), sess.title), el("div", (e) => (e.className = s.rcL2), sess.sub));
  const track = el("div", (e) => (e.className = s.rcTrack));
  const p = spec.plan;
  if (p) {
    const ax = axisOf(p);
    pastShade(track, p, ax);
    const marks: HTMLElement[] = [];
    let prev: string | null | undefined;
    for (const seg of p.plan) {
      if (seg.to <= p.from || seg.from >= p.until) {
        prev = seg.account;
        continue;
      }
      if (seg.account === null) {
        const g = el("div", (e) => (e.className = s.rcHeld));
        g.dataset.rcHeld = "true";
        g.style.left = ax.pos(seg.from);
        g.style.width = ax.wid(seg.from, seg.to);
        const say = copyText("rot.chart.held", { from: seg.fromText, to: seg.toText });
        g.setAttribute("aria-label", say);
        attachTooltip(g, seg.why ? `${say}${copyText("kit.text.sep")}${seg.why}` : say);
        track.appendChild(g);
      } else {
        const b = el("div", (e) => (e.className = s.rcSeg));
        b.dataset.rcSeg = seg.account;
        b.dataset.when = seg.est || seg.from >= p.now ? "future" : "past";
        b.style.left = ax.pos(seg.from);
        b.style.width = ax.wid(seg.from, seg.to);
        tint(b, seg.account, spec.origin);
        b.appendChild(el("span", (e) => (e.className = s.rcSegName), accountLabel(seg.account)));
        attachTooltip(b, copyText("rot.chart.seg", { from: seg.fromText, to: seg.toText, acct: accountLabel(seg.account) }));
        track.appendChild(b);
      }
      // 换号点：这一段跟上一段不是同一个号（头一段之前没有点）。
      if (prev !== undefined && prev !== seg.account && seg.account !== null && seg.from > p.from) {
        const d = el("div", (e) => (e.className = s.rcSwitch));
        d.dataset.rcSwitch = seg.account;
        if (seg.est) d.dataset.est = "true";
        d.style.left = ax.pos(seg.from);
        d.tabIndex = 0;
        const at = seg.est ? copyText("rot.chart.switchEst", { at: seg.fromText, acct: accountLabel(seg.account) }) : copyText("rot.chart.switchAt", { at: seg.fromText, acct: accountLabel(seg.account) });
        const say = seg.why ? `${at}${copyText("kit.text.sep")}${seg.why}` : at;
        d.setAttribute("aria-label", say);
        attachTooltip(d, say);
        marks.push(d);
      }
      prev = seg.account;
    }
    track.append(...marks);
    nowLine(track, p, ax);
  }
  row.append(lc, track, el("div", (e) => (e.className = s.rcGo)));
  return row;
}

function acctRow(spec: ChartSpec, r: ChartRow): HTMLElement {
  const row = el("div", (e) => (e.className = s.rcRow));
  row.dataset.rcRow = r.account;
  if (r.inuse) row.dataset.inuse = "true";
  if (r.skip) row.dataset.skip = "true";
  if (r.selected) row.dataset.ticked = "true";
  const lc = el("div", (e) => (e.className = s.rcLc));
  lc.appendChild(r.head);
  const track = el("div", (e) => (e.className = s.rcTrack));
  const p = spec.plan;
  if (p) {
    const ax = axisOf(p);
    pastShade(track, p, ax);
    const pr = p.rows.find((x) => x.account === r.account);
    // 时段带（这个号的每号参数 slots，后端按视窗切好、写好那一段的字「≤80%」「不用」）：最先画，压在最底下。
    for (const sl of pr?.slots ?? []) {
      const band = el("div", (e) => (e.className = s.rcSlot));
      band.dataset.rcSlot = String(sl.line);
      if (sl.line === 0) band.dataset.unused = "true";
      band.style.left = ax.pos(sl.from);
      band.style.width = ax.wid(sl.from, sl.to);
      band.appendChild(el("b", (e) => (e.className = s.rcSlotText), sl.text));
      track.appendChild(band);
    }
    const base = el("div", (e) => (e.className = s.rcBase));
    base.style.left = ax.pos(p.now);
    base.style.width = ax.wid(p.now, p.until);
    track.appendChild(base);
    let leftDone = false;
    for (const b of pr?.bars ?? []) {
      const e = el("div", (e) => (e.className = s.rcBar));
      e.dataset.rcBar = b.k;
      if (b.est) e.dataset.est = "true";
      e.style.left = ax.pos(b.from);
      e.style.width = ax.wid(b.from, b.to);
      if (b.k === "use") tint(e, r.account, spec.origin);
      // 余 N（离此刻的线还剩几点，后端写好的字）：写在这个号头一段在用的段里。
      if (b.k === "use" && !leftDone && pr?.leftText) {
        leftDone = true;
        const t = el("span", (e) => (e.className = s.rcBarText), pr.leftText);
        t.dataset.rcLeft = "true";
        e.appendChild(t);
        // 段上悬停 / 读屏也写这一句（段太窄时字挪到段外，见 `placeLabels`）。
        e.setAttribute("aria-label", pr.leftText);
        attachTooltip(e, pr.leftText);
      }
      track.appendChild(e);
    }
    // 在用的号估的到线时刻（后端按最近的用速估好）：从那一刻画到这一窗下一次重置（视窗止）。
    if (pr?.est) {
      const until = (pr.resets.find((t) => t.w === pr.est?.w && t.at > (pr.est?.at ?? 0))?.at ?? p.until);
      const e = el("div", (e) => (e.className = s.rcEst));
      e.dataset.rcEst = pr.est.w;
      e.style.left = ax.pos(pr.est.at);
      e.style.width = ax.wid(pr.est.at, until);
      e.textContent = copyText("rot.chart.est", { at: pr.est.atText });
      track.appendChild(e);
    }
    for (const t of pr?.resets ?? []) {
      const m = el("div", (e) => (e.className = s.rcReset));
      m.style.left = ax.pos(t.at);
      const label = el("span", (e) => (e.className = s.rcResetText), t.w === "7d" ? copyText("rot.chart.reset7d", { at: t.atText }) : copyText("rot.chart.reset", { at: t.atText }));
      label.dataset.rcReset = t.w;
      label.style.left = ax.pos(t.at);
      if (t.at > p.until - (p.until - p.from) * 0.08) label.dataset.edge = "end";
      track.append(m, label);
    }
    // 落在视窗外的重置（后端写好的那一句）：行尾右对齐一小格。
    if ((pr?.farResets ?? []).length > 0) {
      const far = el("span", (e) => (e.className = s.rcFar), (pr?.farResets ?? []).map((f) => f.text).join(copyText("kit.text.sep")));
      far.dataset.rcFar = "true";
      track.appendChild(far);
    }
    nowLine(track, p, ax);
  }
  const go = el("button", (e) => (e.className = s.rcGo));
  go.type = "button";
  go.appendChild(icon("caretRight", "compact"));
  go.tabIndex = -1;
  go.setAttribute("aria-label", copyText("rot.chart.open", { name: accountLabel(r.account) }));
  if (spec.onOpen) go.addEventListener("click", () => spec.onOpen?.(r.account));
  else go.disabled = true;
  const why = el("div", (e) => (e.className = s.rcWhy));
  why.dataset.rcWhy = "true";
  why.append(r.skip ? copyText("rot.row.skip", { why: r.why }) : r.why);
  if (r.alsoUsed > 0) {
    if (r.why !== "" || r.skip) why.append(copyText("kit.text.sep"));
    why.appendChild(el("span", (e) => (e.className = s.rcWarn), copyText("rot.row.alsoUsed", { n: r.alsoUsed })));
  }
  row.append(lc, track, go, why);
  return row;
}

function legend(): HTMLElement {
  const lg = el("div", (e) => (e.className = s.rcLegend));
  lg.dataset.rcLegend = "true";
  const item = (k: string, text: string): HTMLElement => {
    const i = el("span", (e) => (e.className = s.rcLgItem));
    const sw = el("i", (e) => (e.className = s.rcSwatch));
    sw.dataset.k = k;
    i.append(sw, text);
    return i;
  };
  lg.append(
    item("use", copyText("rot.chart.lgUse")),
    item("line", copyText("rot.chart.lgLine")),
    item("refused", copyText("rot.chart.lgRefused")),
    item("off", copyText("rot.chart.lgOff")),
    item("slot", copyText("rot.chart.lgSlot")),
    item("skip", copyText("rot.chart.lgSkip")),
    item("base", copyText("rot.chart.lgFull")),
    item("future", copyText("rot.chart.lgFuture")),
    item("switch", copyText("rot.chart.lgSwitch")),
    item("est", copyText("rot.chart.lgEst")),
  );
  return lg;
}

/** 段里那一句（余 N）放不下 ⇒ 整句挪到段的右端外面；放得下 ⇒ 留在段里。只认量出来的宽，不按字数猜。 */
function placeLabels(root: HTMLElement): void {
  // 视窗外重置那一句（行右上角）叠在时段那一段的字（≤80%）上 ⇒ 挪到行的下沿。
  for (const far of root.querySelectorAll<HTMLElement>("[data-rc-far]")) {
    const fr = far.getBoundingClientRect();
    const row = far.closest<HTMLElement>("[data-rc-row]");
    const hit = [...(row?.querySelectorAll<HTMLElement>("[data-rc-slot] > b") ?? [])].some((b) => {
      const r = b.getBoundingClientRect();
      return r.left < fr.right && fr.left < r.right && r.top < fr.bottom && fr.top < r.bottom;
    });
    if (hit) far.dataset.place = "below";
  }
  for (const t of root.querySelectorAll<HTMLElement>("[data-rc-left]")) {
    if (t.scrollWidth <= t.clientWidth) continue;
    t.dataset.place = "after";
    const bar = t.closest<HTMLElement>("[data-rc-bar]");
    if (bar) bar.dataset.labelOut = "true";
  }
}

/** ★ 合图。 */
export function combinedChart(spec: ChartSpec): Chart {
  const root = el("div", (e) => (e.className = s.rcWrap));
  const box = el("div", (e) => (e.className = s.rcChart));
  if (spec.narrow) box.dataset.narrow = "true";
  box.appendChild(head(spec));
  if (spec.session) box.appendChild(sessionRow(spec, spec.session));
  const rowEls = new Map<string, HTMLElement>();
  const list = el("div", (e) => (e.className = s.rcRows));
  list.setAttribute("role", "list");
  for (const r of spec.rows) {
    const e = acctRow(spec, r);
    e.setAttribute("role", "listitem");
    rowEls.set(r.account, e);
    list.appendChild(e);
  }
  box.appendChild(list);
  if (spec.onAdd) {
    const add = el("button", (e) => (e.className = s.rcAdd), copyText("rot.row.addAcct"));
    add.type = "button";
    add.dataset.rcAdd = "true";
    add.addEventListener("click", () => spec.onAdd?.());
    box.appendChild(add);
  }
  root.append(box, legend());
  // 调度：一次性 —— 排好版之后量一次：段里放不下「余 N」⇒ 挪到段的右端外面（不截字）
  requestAnimationFrame(() => placeLabels(root));
  return { root, rowEls };
}
