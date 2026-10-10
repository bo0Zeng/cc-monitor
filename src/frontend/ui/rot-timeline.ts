/**
 * 轮换时间轴（稿 `轮换规则.md` §5.7，截图 08 · 09）：一条轴三处用 —— 会话「账号」面板（本会话轨 ＋ 本会话池的泳道）·
 * 设置「轮换」栏（这台全部号、无本会话轨、行头多「在用 N 会话」）· 规则编辑器的预览（「这条规则」那一轨）。
 *
 * - 输入是 `rotation-plan` 的回答（后端照 `decide` 算好：段 · 泳道 · 重置 · 开窗 · 刻度 · 顶行）；这里只排版。
 * - 本会话轨分两层：上一层是换号点（小菱形 ＋ 原因短码），下一层是段（段里写号名，放不下就收起）—— 两层不叠。
 *   号色按号粘着（规范 V2），同一轴里两号可能同色 ⇒ 每段的读屏名 / 悬停写出「起止 · 号 · 为什么换」。
 * - 悬停 / 键盘（Tab 进轴，←→ 按格走，Home / End）：整列一根细线 ＋ 一张卡，写那一格的「几点 · 用谁 · 谁不能用到几点」；读屏念同一句。
 * 判据：`tests/frontend/ui/rot-timeline.vitest.ts`。
 */
import type { PlanHead, PlanLane, PlanRead, PlanSeg } from "./quota-reads";
import type { SwitchWhy } from "./generated/SwitchWhy";
import { accountAvatarEl, accountColorSlot } from "./account-color";
import { accountLabel, fmtRel, slotLabel } from "./quota-lines";
import { whyOf } from "./acct-view";
import { attachTooltip } from "./kit/tooltip";
import { segmented } from "./kit/tabs";
import { copyText } from "./copy-table";
import s from "./rot-timeline.module.css";

export type TlView = "6h" | "24h" | "7d";
export const TL_VIEWS: TlView[] = ["6h", "24h", "7d"];

/** 建一个元素：`cls` 里给它挂类（写成 `(e) => (e.className = s.x)`，叠没叠的量具认得出挂到哪）。 */
function el<K extends keyof HTMLElementTagNameMap>(
  tagName: K,
  cls: ((e: HTMLElementTagNameMap[K]) => void) | null,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tagName);
  cls?.(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

/** 换号点的原因短码（轴上那一小格）。 */
export function whyShort(w: SwitchWhy | null): string {
  if (w === null) return "";
  if (w === "preempt") return copyText("rot.why.preempt");
  if (w === "leaveFallback") return copyText("rot.why.leave");
  if (typeof w === "string") return "";
  if ("threshold" in w)
    return w.threshold.n === 0
      ? copyText("rot.why.off")
      : copyText("rot.why.trig", { n: w.threshold.n });
  if ("full" in w) return copyText("rot.why.full");
  if ("stint" in w) return copyText("rot.why.stint", { n: w.stint.n });
  if ("held" in w) return copyText("rot.why.held");
  if ("wait" in w)
    return copyText("rot.why.wait", { acct: accountLabel(w.wait.account) });
  return "";
}

/** 悬停 / 读屏那句里的「为什么换」：与换号记录同一套说法（`team ≥90%` · `lab ✕` …）；时段停用另说（触发 0 ＝ 那段封顶为 0）。 */
export function whyLong(
  prev: string | null,
  to: string | null,
  w: SwitchWhy | null,
): string {
  if (w === null) return "";
  const from = prev ?? to ?? "";
  if (typeof w !== "string" && "threshold" in w && w.threshold.n === 0)
    return copyText("rot.pv.whyOff", { acct: accountLabel(from) });
  return whyOf({ at: 0, from, to: to ?? from, why: w }).why;
}

/** 一段的读屏名 / 悬停那一句：`起止 · 号 · 为什么换`。 */
function segSay(seg: PlanSeg, prev: string | null): string {
  const acct =
    seg.account === null ? copyText("rot.pv.held") : accountLabel(seg.account);
  const because = whyLong(prev, seg.account, seg.why);
  return because
    ? copyText("rot.pv.segWhy", {
        from: seg.fromText,
        to: seg.toText,
        acct,
        why: because,
      })
    : copyText("rot.pv.seg", { from: seg.fromText, to: seg.toText, acct });
}

/** 顶行那一句（后端给的 `head`）：卡住 ⇒ 琥珀条那一句；否则「在用 … · 距触发 … · 估 几点 到 N%」。`parts` ＝ 各段（排版时段内不折行）。没有 ⇒ `null`。 */
export function headLine(
  p: PlanRead,
): { text: string; parts: string[]; blocked: boolean } | null {
  const h: PlanHead | undefined = p.head;
  if (!h) return null;
  if ("blocked" in h) {
    const b = h.blocked;
    if (!b.account || b.at === undefined) {
      const text = copyText("rot.tl.blockedNone");
      return { text, parts: [text], blocked: true };
    }
    const parts = [
      copyText("rot.tl.blocked", {
        acct: accountLabel(b.account),
        at: b.atText ?? "",
        rel: fmtRel(b.at, p.now) ?? "",
      }),
    ];
    if (b.w) parts.push(copyText("rot.tl.blockedW", { w: slotLabel(b.w) }));
    return { text: parts.join(copyText("kit.text.sep")), parts, blocked: true };
  }
  if (!h.account) return null;
  const parts = [
    h.w !== undefined && h.pct !== undefined
      ? copyText("rot.tl.now", {
          acct: accountLabel(h.account),
          w: slotLabel(h.w),
          pct: h.pct,
        })
      : copyText("rot.tl.nowBare", { acct: accountLabel(h.account) }),
  ];
  if (h.toTrigger !== undefined)
    parts.push(copyText("rot.tl.toTrig", { n: h.toTrigger }));
  if (h.est) parts.push(copyText("rot.tl.est", { at: h.est.atText, pct: h.est.pct }));
  return { text: parts.join(copyText("kit.text.sep")), parts, blocked: false };
}

/** 顶行右侧的视窗分段 `6h | 24h | 7d`。 */
export function viewSwitch(
  current: TlView,
  onChange: (v: TlView) => void,
): HTMLElement {
  const sw = segmented<TlView>({
    items: TL_VIEWS.map((v) => ({ key: v, label: v })),
    current,
    onChange,
    label: copyText("rot.tl.spanLabel"),
  });
  sw.dataset.tlView = current;
  return sw;
}

export interface AxisOpts {
  /** 本会话 / 这条规则那一轨的名字；`null` ＝ 不画那一轨（设置里）。 */
  track: string | null;
  /** 行头写「在用 N 会话」（设置里）。 */
  usedBy?: boolean;
  /** 窄一档的行头（面板里，宽 408）。 */
  compact?: boolean;
}

function laneSay(l: PlanLane, at: number): string | null {
  const sp = l.spans.find((x) => x.from <= at && at < x.to);
  if (!sp) return null;
  const acct = accountLabel(l.account);
  const what =
    sp.state === "refused"
      ? copyText("rot.tlSt.refused", { acct })
      : sp.state === "capped"
        ? copyText("rot.tlSt.capped", { acct, n: sp.n ?? "" })
        : sp.state === "off"
          ? copyText("rot.tlSt.off", { acct })
          : copyText("rot.tlSt.overage", { acct });
  return copyText("rot.tlSt.until", { what, at: sp.toText });
}

/** 一格（刻度上的一个时刻）的那一句：`22:30 · 用 team · personal 过封顶 99 至 23:00 · …`。 */
export function cellSay(
  p: PlanRead,
  segs: PlanSeg[],
  at: number,
  atText: string,
): string {
  const parts = [atText];
  const seg = segs.find((x) => x.from <= at && at < x.to);
  if (seg)
    parts.push(
      seg.account === null
        ? copyText("rot.tlHover.held")
        : copyText("rot.tlHover.use", { acct: accountLabel(seg.account) }),
    );
  for (const l of p.lanes) {
    const t = laneSay(l, at);
    if (t) parts.push(t);
  }
  return parts.join(copyText("kit.text.sep"));
}

/** 轴本体：刻度 · （本会话轨：换号点一层 ＋ 段一层）· 泳道 · 现在竖线 · 悬停列 · 图例。 */
export function timelineAxis(p: PlanRead, opts: AxisOpts): HTMLElement {
  const root = el("div", (e) => (e.className = s.tl));
  root.dataset.tl = "";
  if (opts.compact) root.dataset.compact = "true";
  if (opts.usedBy) root.dataset.usedBy = "true";
  const t0 = p.from ?? p.now;
  const len = Math.max(1, p.until - t0);
  const frac = (t: number): number =>
    Math.min(1, Math.max(0, (t - t0) / len));
  const pos = (t: number): string => `${(frac(t) * 100).toFixed(3)}%`;
  const wid = (a: number, b: number): string =>
    `${((frac(b) - frac(a)) * 100).toFixed(3)}%`;
  const grid = el("div", (e) => (e.className = s.tlGrid));
  root.appendChild(grid);
  const row = (
    label: HTMLElement | null,
    id: string,
    track: (e: HTMLElement) => void,
  ): HTMLElement => {
    const r = el("div", (e) => (e.className = s.tlRow));
    r.dataset.tlRow = id;
    const head = label ?? el("span", null);
    const t = el("div", (e) => (e.className = s.tlTrack));
    track(t);
    r.append(head, t);
    grid.appendChild(r);
    return t;
  };

  // 刻度：后端给的 grid 里带 label 的那几格；没有 grid（编辑器那一问）⇒ 两头的时刻。
  const ticks = row(null, "ticks", (e) => (e.className = s.tlTicks));
  const labels =
    p.grid?.filter((g) => g.label !== undefined) ??
    [
      { at: t0, atText: p.fromText ?? p.nowText, label: p.fromText ?? p.nowText },
      {
        at: p.until,
        atText: p.plan.at(-1)?.toText ?? "",
        label: p.plan.at(-1)?.toText ?? "",
      },
    ];
  for (const g of labels) {
    const t = el("span", (e) => (e.className = s.tlTick), g.label);
    t.style.left = pos(g.at);
    t.dataset.tlTick = "";
    ticks.appendChild(t);
  }

  const segs: PlanSeg[] = [...(p.past ?? []), ...p.plan];
  if (opts.track !== null) {
    const name = el("span", (e) => (e.className = s.tlLaneName), opts.track);
    const t = row(name, "track", (e) => (e.className = s.tlTrackSession));
    const marks = el("div", (e) => (e.className = s.tlWhyLayer));
    const bars = el("div", (e) => (e.className = s.tlSegLayer));
    t.append(marks, bars);
    const pastN = p.past?.length ?? 0;
    let prev: string | null = null;
    segs.forEach((seg, i) => {
      const b = el("span", (e) => (e.className = s.tlSeg));
      b.style.left = pos(seg.from);
      b.style.width = wid(seg.from, seg.to);
      b.dataset.tlSeg = seg.account ?? "";
      b.dataset.when = i < pastN ? "past" : "future";
      const acct =
        seg.account === null
          ? copyText("rot.pv.held")
          : accountLabel(seg.account);
      if (seg.account === null) b.dataset.held = "true";
      else
        b.style.setProperty(
          "--tl-c",
          `var(--acct-c${accountColorSlot(seg.account)})`,
        );
      if (seg.account !== null && i < pastN)
        b.style.color = `var(--acct-ink${accountColorSlot(seg.account)})`;
      const say = segSay(seg, prev);
      b.setAttribute("role", "img");
      b.setAttribute("aria-label", say);
      attachTooltip(b, say);
      const n = el("span", (e) => (e.className = s.tlSegName), acct);
      n.dataset.tlSegName = "";
      n.setAttribute("aria-hidden", "true");
      b.appendChild(n);
      bars.appendChild(b);
      const why = whyShort(seg.why);
      if (why && seg.from > t0) {
        const m = el("span", (e) => (e.className = s.tlWhy));
        m.style.left = pos(seg.from);
        m.dataset.tlWhy = "";
        m.setAttribute("aria-hidden", "true");
        m.append(
          el("span", (e) => (e.className = s.tlDiamond)),
          el("span", (e) => (e.className = s.tlWhyText), why),
        );
        marks.appendChild(m);
      }
      prev = seg.account;
    });
    const end = p.plan.at(-1)?.to;
    if (end !== undefined && end < p.until) {
      const dash = el("span", (e) => (e.className = s.tlNoPlan), copyText("rot.tl.noPlan"));
      dash.style.left = pos(end);
      dash.dataset.tlNoPlan = "";
      bars.appendChild(dash);
    }
    fit(t);
  }

  for (const lane of p.lanes) {
    const name = el("span", (e) => (e.className = s.tlLaneName));
    name.append(
      accountAvatarEl(lane.account, { size: 14 }),
      el("span", (e) => (e.className = s.tlLaneAcct), accountLabel(lane.account)),
    );
    if (lane.pct !== undefined && lane.pct !== null)
      name.appendChild(el("span", (e) => (e.className = s.tlPct), `${lane.pct}%`));
    if (opts.usedBy && lane.usedBy !== undefined && lane.usedBy > 0) {
      // ≥2 个会话共用一个号 ⇒ 琥珀（烧得更快，要你留意）。
      const chip = el("span", (e) => (e.className = s.tlUsedBy), copyText("rot.tl.usedBy", { n: lane.usedBy }));
      chip.dataset.tlUsedBy = String(lane.usedBy);
      if (lane.usedBy >= 2) chip.dataset.warn = "true";
      name.appendChild(chip);
    }
    const t = row(name, lane.account, (e) => (e.className = s.tlLaneTrack));
    for (const sp of lane.spans) {
      const b = el("span", (e) => (e.className = s.tlSpan));
      b.style.left = pos(sp.from);
      b.style.width = wid(sp.from, sp.to);
      b.dataset.state = sp.state;
      t.appendChild(b);
    }
    for (const r of lane.resets) {
      const m = el("span", (e) => (e.className = r.w === "7d" ? s.tlReset7 : s.tlReset5));
      m.style.left = pos(r.at);
      m.dataset.tlReset = r.w;
      attachTooltip(m, copyText("rot.tlMark.reset", { w: slotLabel(r.w), at: r.atText }));
      t.appendChild(m);
    }
    for (const w of lane.warm ?? []) {
      const m = el("span", (e) => (e.className = s.tlWarm));
      m.style.left = pos(w.at);
      m.dataset.tlWarm = "";
      attachTooltip(m, copyText("rot.tlMark.warm", { at: w.atText }));
      t.appendChild(m);
    }
  }

  // 现在竖线（贯穿全部轨，刻度那一行写时刻）。
  const now = el("div", (e) => (e.className = s.tlNow));
  now.style.left = pos(p.now);
  now.dataset.tlNow = "";
  now.appendChild(el("span", (e) => (e.className = s.tlNowText), p.nowText));
  const overlay = el("div", (e) => (e.className = s.tlOverlay));
  overlay.appendChild(now);
  grid.appendChild(overlay);
  if (p.grid && p.grid.length > 0) hover(root, overlay, p, segs, pos);
  fitTicks(ticks, now);

  root.appendChild(legend(p, opts));
  return root;
}

/** 悬停列 ＋ 键盘：整列一根细线、一张卡（读屏念同一句）。 */
function hover(
  root: HTMLElement,
  overlay: HTMLElement,
  p: PlanRead,
  segs: PlanSeg[],
  pos: (t: number) => string,
): void {
  const cells = p.grid!;
  const line = el("div", (e) => (e.className = s.tlCursor));
  line.dataset.tlCursor = "";
  const card = el("div", (e) => (e.className = s.tlCard));
  card.dataset.tlCard = "";
  card.setAttribute("role", "status");
  card.setAttribute("aria-live", "polite");
  overlay.append(line, card);
  const nowAt = cells.reduce(
    (best, g, i) =>
      Math.abs(g.at - p.now) < Math.abs(cells[best].at - p.now) ? i : best,
    0,
  );
  let at = -1;
  const show = (i: number): void => {
    at = Math.max(0, Math.min(cells.length - 1, i));
    const g = cells[at];
    line.dataset.shown = "true";
    card.dataset.shown = "true";
    line.style.left = pos(g.at);
    card.textContent = cellSay(p, segs, g.at, g.atText);
    card.dataset.side = (g.at - (p.from ?? p.now)) / Math.max(1, p.until - (p.from ?? p.now)) > 0.6 ? "left" : "right";
    card.style.left = pos(g.at);
    root.dataset.tlAt = String(g.at);
  };
  const hide = (): void => {
    delete line.dataset.shown;
    delete card.dataset.shown;
    delete root.dataset.tlAt;
  };
  root.tabIndex = 0;
  root.setAttribute("role", "group");
  root.setAttribute("aria-label", copyText("rot.tl.axis"));
  overlay.addEventListener("mousemove", (ev) => {
    const r = overlay.getBoundingClientRect();
    if (r.width <= 0) return;
    const f = (ev.clientX - r.left) / r.width;
    const t = (p.from ?? p.now) + f * (p.until - (p.from ?? p.now));
    let best = 0;
    for (let i = 1; i < cells.length; i++)
      if (Math.abs(cells[i].at - t) < Math.abs(cells[best].at - t)) best = i;
    show(best);
  });
  overlay.addEventListener("mouseleave", () => {
    if (document.activeElement !== root) hide();
  });
  root.addEventListener("focus", () => show(at < 0 ? nowAt : at));
  root.addEventListener("blur", hide);
  root.addEventListener("keydown", (ev) => {
    const k = ev.key;
    if (k === "ArrowRight" || k === "ArrowLeft" || k === "Home" || k === "End") {
      ev.preventDefault();
      const cur = at < 0 ? nowAt : at;
      show(
        k === "ArrowRight"
          ? cur + 1
          : k === "ArrowLeft"
            ? cur - 1
            : k === "Home"
              ? 0
              : cells.length - 1,
      );
    } else if (k === "Escape") hide();
  });
}

/** 段里的号名、换号点的原因字：放不下就收起（悬停 / 读屏照样念全句）。没排版的环境（量不出宽）一律留着。 */
function fit(track: HTMLElement): void {
  const measure = (): void => {
    for (const n of track.querySelectorAll<HTMLElement>("[data-tl-seg-name]")) {
      const seg = n.parentElement!;
      delete n.dataset.fit;
      if (seg.clientWidth > 0 && n.scrollWidth > seg.clientWidth - 4)
        n.dataset.fit = "no";
    }
    // 先全部放开量一遍（收起的字也要量出宽），再一起定谁收；菱形永远在原处。
    const marks = [...track.querySelectorAll<HTMLElement>("[data-tl-why]")];
    for (const m of marks) delete m.dataset.fit;
    const boxes = marks.map((m) => ({
      diamond: m.firstElementChild!.getBoundingClientRect(),
      text: m.lastElementChild!.getBoundingClientRect(),
    }));
    whyTextKeeps(boxes).forEach((keep, i) => {
      if (!keep && boxes[i].text.width > 0) marks[i].dataset.fit = "no";
    });
  };
  if (typeof ResizeObserver === "undefined") return;
  new ResizeObserver(measure).observe(track);
}

/** 换号点之间留的最小空：原因字的右缘离任何一颗菱形不足这么多就不写字。 */
const WHY_GAP = 6;

/**
 * 换号点挤时谁收字（界面小修 10-09 · B）：一条的字盖住任何一颗（别的）菱形，或右缘离右边那颗不足 6px ⇒ 这一条不写字，
 * 只留菱形；原因照样在段的悬停与读屏里。两颗挨着时结果是前一颗收字、后一颗留字。
 */
export function whyTextKeeps(
  marks: readonly { diamond: { left: number; right: number }; text: { left: number; right: number } }[],
): boolean[] {
  return marks.map(({ text: t }, i) =>
    marks.every(
      ({ diamond: d }, j) => j === i || !(d.right > t.left && d.left < t.right + WHY_GAP),
    ),
  );
}

/** 刻度字压着「现在」那个时刻（或彼此叠了）⇒ 那一格的字不写（`现在` 的字优先）。 */
function fitTicks(ticks: HTMLElement, now: HTMLElement): void {
  const measure = (): void => {
    const nowText = now.firstElementChild?.getBoundingClientRect();
    let right = -Infinity;
    for (const t of ticks.querySelectorAll<HTMLElement>("[data-tl-tick]")) {
      delete t.dataset.fit;
      const r = t.getBoundingClientRect();
      if (r.width === 0) continue;
      const hitNow =
        nowText !== undefined &&
        nowText.width > 0 &&
        r.left < nowText.right + 4 &&
        nowText.left < r.right + 4;
      if (hitNow || r.left < right + 4) t.dataset.fit = "no";
      else right = r.right;
    }
  };
  if (typeof ResizeObserver === "undefined") return;
  new ResizeObserver(measure).observe(ticks);
}

function legend(p: PlanRead, opts: AxisOpts): HTMLElement {
  const lg = el("div", (e) => (e.className = s.tlLegend));
  lg.dataset.tlLegend = "";
  const items: [string, string][] = [];
  if (opts.track !== null) {
    if ((p.past?.length ?? 0) > 0) items.push([copyText("rot.tlLg.past"), "past"]);
    items.push([copyText("rot.tlLg.future"), "future"]);
  }
  items.push(
    [copyText("rot.tlLg.refused"), "refused"],
    [copyText("rot.tlLg.capped"), "capped"],
    [copyText("rot.tlLg.off"), "off"],
    [copyText("rot.tlLg.r5"), "r5"],
    [copyText("rot.tlLg.r7"), "r7"],
  );
  if (p.lanes.some((l) => (l.warm?.length ?? 0) > 0))
    items.push([copyText("rot.tlLg.warm"), "warm"]);
  for (const [text, kind] of items) {
    const it = el("span", (e) => (e.className = s.tlLegendItem));
    it.dataset.tlLg = kind;
    const sw = el("span", (e) => (e.className = s.tlSwatch));
    sw.dataset.kind = kind;
    it.append(sw, el("span", null, text));
    lg.appendChild(it);
  }
  return lg;
}
