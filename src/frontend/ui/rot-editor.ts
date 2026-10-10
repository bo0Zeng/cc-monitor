/**
 * 轮换的几块编辑件（会话「账号」面板里「本会话」那一份，与设置里的规则编辑器同一组件）：
 * 换法（按顺序 · 抢回 · 单段预算 N 点）· 最多等几分钟 · 每号封顶浮层（不设 · 固定 · 按时段）· 只读的封顶标签 · 「存为规则…」浮层。
 *
 * - 界面不判对错：封顶浮层按「存」先交后端逐格校验（`rotation-plan`），错的格照它标红；存规则的名称对错也是后端回的。
 * - 浮层里改的是草稿，按「存」才写；Esc / 取消 不写。
 * 判据：`tests/frontend/ui/rot-editor.vitest.ts` · `tests/frontend/ui/acct-panel.vitest.ts`（面板里那几处）。
 */
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { button } from "./kit/button";
import { field } from "./kit/field";
import { closePopover, openPopover } from "./kit/popover";
import { segmented } from "./kit/tabs";
import { accountLabel, slotLabel } from "./acct-words";
import { attachTooltip } from "./kit/tooltip";
import { accountAvatarEl } from "./account-color";
import {
  checkRotation,
  readPlan,
  saveRule,
  type CapAt,
  type PlanRead,
  type RuleRow,
} from "./quota-reads";
import type { CapValue } from "./generated/CapValue";
import type { CellError } from "./generated/CellError";
import type { Rotation } from "./generated/Rotation";
import type { RotationSlot } from "./generated/RotationSlot";
import type { QuotaRead } from "./acct-words";
import s from "./rot-editor.module.css";

/** 所有号 / 所有窗口的那一格键。 */
const ALL = "*";

/** 建一个元素：`cls` 里给它挂类（写成 `(e) => (e.className = s.x)`，叠没叠的量具认得出挂到哪）。 */
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  cls: ((e: HTMLElementTagNameMap[K]) => void) | null,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  cls?.(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

function numInput(value: number, label: string): HTMLInputElement {
  const i = document.createElement("input");
  i.type = "text";
  i.inputMode = "numeric";
  i.className = s.rotNum;
  i.value = String(value);
  i.setAttribute("aria-label", label);
  return i;
}

// ─────────────────────────────── 顺序（面板「本会话」与规则编辑器同一套）

const START: RotationSlot = { start: true };

export type Row = { account: string; start: boolean; on: boolean };

/** 这一份轮换画成哪几行：顺序里的（占位换成起始号）＋ 这台账号库里其余的号（不勾，排后面）。 */
export function rowsOf(
  r: Rotation,
  start: string,
  quota: QuotaRead | null,
  agent: string,
): Row[] {
  const out: Row[] = [];
  const seen = new Set<string>();
  for (const slot of r.order) {
    const a = typeof slot === "string" ? slot : start;
    if (seen.has(a)) continue;
    seen.add(a);
    out.push({
      account: a,
      start: typeof slot !== "string",
      on: typeof slot !== "string" || r.enabled.includes(a),
    });
  }
  const others = [...(quota?.accounts ?? []), ...(quota?.unseen ?? [])]
    .filter((x) => x.agent === agent && x.account !== "_")
    .map((x) => x.account);
  for (const a of others) {
    if (seen.has(a)) continue;
    seen.add(a);
    out.push({ account: a, start: false, on: false });
  }
  return out;
}

/** 勾 / 不勾一个号 ⇒ 只改 `enabled`；`order` 逐字不动（序里没有的号勾上时排到末尾），不添占位。 */
export function toggled(r: Rotation, account: string, on: boolean): Rotation {
  const enabled = on
    ? r.enabled.includes(account)
      ? r.enabled
      : [...r.enabled, account]
    : r.enabled.filter((a) => a !== account);
  const order =
    on && !r.order.includes(account) ? [...r.order, account] : r.order;
  return { ...r, order, enabled };
}

/**
 * 把第 `from` 行挪到 `to` ⇒ 新的 `order`：按行的新次序写回原来就有的那几格（占位只在原来有时才有；起始号若另外具名在序里，
 * 那一格跟在占位后面，不丢）；序外的行只有被挪的那一行才进序。`enabled` 不动。
 */
export function moved(
  r: Rotation,
  rows: Row[],
  from: number,
  to: number,
): Rotation {
  const order: RotationSlot[] = [];
  move(rows, from, to).forEach((row) => {
    if (row.start) order.push(START);
    if (r.order.includes(row.account) || (row === rows[from] && !row.start))
      order.push(row.account);
  });
  return { ...r, order };
}

/** 第 `from` 项挪到 `to`（新数组）。 */
export function move<T>(xs: T[], from: number, to: number): T[] {
  const out = [...xs];
  const [x] = out.splice(from, 1);
  out.splice(to, 0, x);
  return out;
}

// ─────────────────────────────── 触发（规则一级的线：`cap["*"]` 的 5h · 7d 两格；面板与规则编辑器同一组件）

/** 触发那一行的两窗。 */
export const LINE_WINDOWS = ["5h", "7d"] as const;
export type LineWindow = (typeof LINE_WINDOWS)[number];

/** 触发那一行的两格（`cap["*"]`）；没设的那一窗不在里面。 */
export function linesOf(r: Rotation): Partial<Record<LineWindow, CapValue>> {
  const row = r.cap?.[ALL] ?? {};
  const out: Partial<Record<LineWindow, CapValue>> = {};
  for (const w of LINE_WINDOWS) {
    const v = row[w];
    if (v !== undefined) out[w] = v;
  }
  return out;
}

/** 触发这一行此刻是不是「到线」（有一格设了）。 */
export function atLineMode(r: Rotation): boolean {
  return Object.keys(linesOf(r)).length > 0;
}

/** 两格都清掉（「满」）。 */
function withoutLines(r: Rotation): Rotation {
  let out = r;
  for (const w of LINE_WINDOWS) out = withCap(out, ALL, w, undefined);
  return out;
}

export interface TriggerOpts {
  /** 单选组的名（同一页里唯一）。 */
  name: string;
  readonly: boolean;
  /** 后端回的逐格错（`cap.*.5h` · `cap.*.7d` 那两格标红）。 */
  errors: CellError[];
  /** 填错那一格上次填的字（后端拒了、盘上没变 ⇒ 照原样画回去、框红）。 */
  draft?: Partial<Record<LineWindow, string>>;
  origin: Origin;
  write: (r: Rotation) => void;
  /** 记下这一格填的字（交后端之前）。 */
  onDraft?: (w: LineWindow, text: string) => void;
  /** 格子拿到 / 交出焦点（面板据此在编辑时不重画）。 */
  onFocus?: () => void;
  onBlur?: () => void;
}

/**
 * `触发 (•)满 ( )到线  5h [90] %  7d [—] %`：单选由两格推出来（有一格设了 ＝ 到线），不另存。
 * 点「到线」且两格都空 ⇒ 5h 预填 90 写盘；点「满」⇒ 两格清掉写盘。格子 Enter / 失焦即写、Esc 还原；空 ＝ 那一窗满了才换。
 * 范围由后端判（1–99），错的那一格框红 ＋ 行尾红字。盘上是按时段的那一格画成按钮，点开封顶浮层（与封顶表同一浮层）。
 */
export function triggerLine(r: Rotation, o: TriggerOpts): HTMLElement {
  const box = el("span", (e) => (e.className = s.rotTrigger));
  box.dataset.rotTrigger = "";
  const lines = linesOf(r);
  const on = atLineMode(r);
  const radio = (
    checked: boolean,
    label: string,
    key: string,
    pick: () => void,
  ): HTMLLabelElement => {
    const l = el("label", (e) => (e.className = s.rotCheck));
    const i = document.createElement("input");
    i.type = "radio";
    i.name = o.name;
    i.checked = checked;
    i.disabled = o.readonly;
    i.dataset.rotTrig = key;
    i.addEventListener("change", pick);
    l.append(i, document.createTextNode(label));
    return l;
  };
  const radios = el("span", (e) => (e.className = s.rotHow));
  radios.append(
    radio(!on, copyText("acct.rot.trigFull"), "full", () => {
      if (on) o.write(withoutLines(r));
    }),
    radio(on, copyText("acct.rot.trigLine"), "line", () => {
      if (!on) o.write(withCap(r, ALL, "5h", 90));
    }),
  );
  const cells = el("span", (e) => (e.className = s.rotTrigger));
  cells.dataset.rotLines = "";
  let bad = false;
  for (const w of LINE_WINDOWS) {
    const v = lines[w];
    const cell = el("span", (e) => (e.className = s.rotCheck));
    cell.dataset.rotLine = w;
    cell.appendChild(
      el("span", (e) => (e.className = s.rotLabel), slotLabel(w)),
    );
    const wrong = o.errors.some((e) => e.cell === `cap.${ALL}.${w}`);
    if (wrong) bad = true;
    if (Array.isArray(v)) {
      // 按时段（只有命令行 / AI 写得出）：画成按钮，点开即封顶浮层；界面不另开入口去写时段。
      const b = button({
        label: copyText("rot.cap.slotsN", { n: v.length }),
        kind: "secondary",
        size: "compact",
        onClick: () => openCapEditor(b, o.origin, r, ALL, w, o.write),
      });
      b.disabled = o.readonly;
      b.dataset.rotLineSlots = w;
      cell.appendChild(b);
    } else {
      const saved = v === undefined ? "" : String(v);
      const shown = wrong && o.draft?.[w] !== undefined ? o.draft[w] : saved;
      const i = numInput(0, "");
      i.setAttribute("aria-label", copyText("acct.rot.lineAria", { w: slotLabel(w) }));
      i.value = shown ?? "";
      i.placeholder = copyText("rot.list.useNone");
      i.disabled = o.readonly;
      i.dataset.rotLineNum = w;
      if (wrong) i.dataset.error = "true";
      if (v === undefined) {
        const empty = copyText("acct.rot.lineEmpty", { w: slotLabel(w) });
        i.setAttribute("aria-description", empty);
        attachTooltip(i, empty);
      }
      let cancelled = false;
      const commit = (): void => {
        if (cancelled) {
          cancelled = false;
          return;
        }
        const t = i.value.trim();
        if (t === saved && !wrong) return;
        o.onDraft?.(w, t);
        if (t === "") {
          if (v !== undefined) o.write(withCap(r, ALL, w, undefined));
          return;
        }
        // 写错的数也交后端（它回 range、框红），这里不另判范围。
        const n = /^\d{1,3}$/.test(t) ? Number(t) : -1;
        o.write(withCap(r, ALL, w, n));
      };
      i.addEventListener("focus", () => {
        cancelled = false;
        o.onFocus?.();
      });
      i.addEventListener("change", commit);
      i.addEventListener("blur", () => o.onBlur?.());
      i.addEventListener("keydown", (ev) => {
        if (ev.isComposing) return;
        if (ev.key === "Enter") i.blur();
        if (ev.key === "Escape") {
          i.value = saved;
          cancelled = true;
          ev.stopPropagation();
          i.blur();
        }
      });
      cell.appendChild(i);
    }
    cell.appendChild(
      el("span", (e) => (e.className = s.rotUnit), copyText("acct.rot.pctUnit")),
    );
    cells.appendChild(cell);
  }
  box.append(radios, cells);
  if (bad) {
    const e = el(
      "span",
      (e) => (e.className = s.rotErr),
      copyText("rot.ed.pctErr"),
    );
    e.dataset.rotLineErr = "";
    box.appendChild(e);
  }
  return box;
}

/** 后端预览里这个号此刻那一格是「过线」⇒ 过线的那一窗（语义位）；不是 ⇒ `null`（列表行尾「7d 到线」那一枚签，界面不比数）。 */
export function lineNow(plan: PlanRead | null, account: string): string | null {
  if (!plan) return null;
  const lane = plan.lanes.find((l) => l.account === account);
  const sp = lane?.spans.find(
    (x) => x.from <= plan.now && plan.now < x.to && x.state === "capped",
  );
  return sp?.w ?? null;
}

// ─────────────────────────────── 换法

export type How = "order" | "preempt" | "stint";

/** 单段预算对「所有号所有窗口」写的那一个数。 */
export function stintAll(r: Rotation): number | null {
  return r.stint?.[ALL]?.[ALL] ?? null;
}

export function howOf(r: Rotation): How {
  if (stintAll(r) !== null) return "stint";
  return r.preempt ? "preempt" : "order";
}

/** 换成哪一种 ⇒ 整份新的（只动 `preempt` 与 `stint["*"]["*"]`，号自己的单段预算不动）。 */
export function withHow(
  r: Rotation,
  how: How,
  n = stintAll(r) ?? 10,
  alsoPreempt = r.preempt ?? false,
): Rotation {
  const stint = { ...(r.stint ?? {}) };
  delete stint[ALL];
  if (how === "stint") stint[ALL] = { [ALL]: n };
  const out: Rotation = {
    ...r,
    preempt: how === "preempt" || (how === "stint" && alsoPreempt),
    stint,
  };
  if (Object.keys(stint).length === 0) delete out.stint;
  return out;
}

/** `换法 [按顺序 | 抢回 | 单段预算] [10] 点 ☐同时抢回`：点即写（与触发那一格同一套：改即生效）。 */
export function howControl(
  r: Rotation,
  readonly: boolean,
  write: (r: Rotation) => void,
): HTMLElement {
  const box = el("span", (e) => (e.className = s.rotHow));
  box.appendChild(
    el("span", (e) => (e.className = s.rotLabel), copyText("rot.how.label")),
  );
  const how = howOf(r);
  const seg = segmented<How>({
    items: [
      { key: "order", label: copyText("rot.how.order") },
      { key: "preempt", label: copyText("rot.how.preempt") },
      { key: "stint", label: copyText("rot.how.stint") },
    ],
    current: how,
    label: copyText("rot.how.label"),
    onChange: (k) => write(withHow(r, k)),
  });
  if (readonly) {
    seg.dataset.disabled = "true";
    for (const b of seg.querySelectorAll<HTMLButtonElement>("button"))
      b.disabled = true;
  }
  seg.dataset.rotHow = how;
  box.appendChild(seg);
  if (how === "stint") {
    const n = numInput(stintAll(r) ?? 10, copyText("rot.how.stint"));
    n.disabled = readonly;
    n.addEventListener("change", () => {
      const v = Number(n.value.trim());
      if (Number.isInteger(v)) write(withHow(r, "stint", v));
    });
    box.append(
      n,
      el("span", (e) => (e.className = s.rotUnit), copyText("rot.how.unit")),
    );
    const also = document.createElement("label");
    also.className = s.rotCheck;
    const c = document.createElement("input");
    c.type = "checkbox";
    c.checked = r.preempt ?? false;
    c.disabled = readonly;
    c.addEventListener("change", () =>
      write(withHow(r, "stint", stintAll(r) ?? 10, c.checked)),
    );
    also.append(c, document.createTextNode(copyText("rot.how.stintPlus")));
    box.appendChild(also);
  }
  return box;
}

/** `最多等 [10] 分`（0 ＝ 不等）：失焦 / 回车写。范围后端判。 */
export function waitControl(
  r: Rotation,
  readonly: boolean,
  write: (r: Rotation) => void,
): HTMLElement {
  const box = el("span", (e) => (e.className = s.rotHow));
  box.appendChild(
    el("span", (e) => (e.className = s.rotLabel), copyText("rot.wait.label")),
  );
  const n = numInput(r.wait, copyText("rot.wait.label"));
  n.disabled = readonly;
  n.dataset.rotWait = "true";
  n.addEventListener("change", () => {
    const v = Number(n.value.trim());
    if (Number.isInteger(v) && v !== r.wait) write({ ...r, wait: v });
  });
  box.append(
    n,
    el("span", (e) => (e.className = s.rotUnit), copyText("rot.wait.unit")),
  );
  return box;
}

// ─────────────────────────────── 兜底

/** 标 / 不标一个号为兜底（只动 `fallback`）。 */
export function withFallback(
  r: Rotation,
  account: string,
  on: boolean,
): Rotation {
  const cur = r.fallback ?? [];
  const next = on
    ? cur.includes(account)
      ? cur
      : [...cur, account]
    : cur.filter((a) => a !== account);
  const out: Rotation = { ...r, fallback: next };
  if (next.length === 0) delete out.fallback;
  return out;
}

/** 号那一行的「兜底」开关（只在顺序里具名的号上有）：点即写。 */
export function fallbackToggle(
  r: Rotation,
  account: string,
  name: string,
  write: (r: Rotation) => void,
): HTMLButtonElement {
  const on = (r.fallback ?? []).includes(account);
  // 开 ⇒ 实心的「兜底」标；关 ⇒ 平时不显示（透明、仍占位、仍在 Tab 序里），行悬停或行内有焦点才出（`rot-editor.module.css`）。
  const b = button({
    label: copyText("rot.fallback.tag"),
    kind: "ghost",
    size: "compact",
    hint: copyText("rot.fallback.hint"),
    onClick: () => write(withFallback(r, account, !on)),
  });
  b.classList.add(s.rotFallback);
  b.setAttribute("aria-label", copyText("rot.fallback.aria", { name }));
  b.setAttribute("aria-pressed", String(on));
  b.dataset.rotFallback = account;
  return b;
}

/** 只读视图里开着的兜底：与开关「开」同一个实心标；关着的不画。 */
export function fallbackMark(): HTMLElement {
  const t = document.createElement("span");
  t.className = s.rotFallbackMark;
  t.textContent = copyText("rot.fallback.tag");
  t.title = copyText("rot.fallback.hint");
  t.dataset.rotFallbackMark = "";
  return t;
}

// ─────────────────────────────── 封顶

/** 一个号的封顶写成几枚只读标签（`5h ≤80` · `17:00-02:00 停用` · `02:00-17:00 ≤99`）。 */
export function capTags(r: Rotation, account: string): string[] {
  const per = r.cap?.[account];
  if (!per) return [];
  const out: string[] = [];
  for (const [w, v] of Object.entries(per)) {
    if (v === undefined) continue;
    if (typeof v === "number")
      out.push(
        w === ALL
          ? copyText("rot.capTag.fixedAll", { n: v })
          : copyText("rot.capTag.fixed", { w: slotLabel(w), n: v }),
      );
    else
      for (const x of v)
        out.push(
          x.n === 0
            ? copyText("rot.capTag.off", { at: x.at })
            : copyText("rot.capTag.slot", { at: x.at, n: x.n }),
        );
  }
  return out;
}

/** 封顶按钮上的字：没设 `封顶` · 固定 `≤99` · 按时段 `时段 2`。 */
export function capShort(v: CapValue | undefined): string {
  if (v === undefined) return copyText("rot.cap.label");
  if (typeof v === "number") return copyText("rot.cap.fixedShort", { n: v });
  return copyText("rot.cap.slotsN", { n: v.length });
}

/** 把一格封顶换成 `v`（`undefined` ＝ 不设，整格删掉；号的那一行空了也删）。 */
export function withCap(
  r: Rotation,
  account: string,
  w: string,
  v: CapValue | undefined,
): Rotation {
  const cap = { ...(r.cap ?? {}) };
  const row = { ...(cap[account] ?? {}) };
  if (v === undefined) delete row[w];
  else row[w] = v;
  if (Object.keys(row).length === 0) delete cap[account];
  else cap[account] = row;
  const out: Rotation = { ...r, cap };
  if (Object.keys(cap).length === 0) delete out.cap;
  return out;
}

type CapMode = "none" | "fixed" | "slots";

interface Slot {
  from: string;
  to: string;
  n: string;
}

/** 一天里的分钟（`24:00` 收）；写错 ⇒ `null`（只用来画色带，对错后端判）。 */
function minuteOf(t: string): number | null {
  const m = /^(\d{2}):(\d{2})$/.exec(t);
  if (!m) return null;
  const v = Number(m[1]) * 60 + Number(m[2]);
  return v <= 24 * 60 ? v : null;
}

/** 24h 色带：每段一截（跨午夜的画成两截），`0` 斜纹 ＝ 停用。 */
function band(slots: Slot[]): HTMLElement {
  const b = el("div", (e) => (e.className = s.rotBand));
  for (const x of slots) {
    const [f, t] = [minuteOf(x.from), minuteOf(x.to)];
    if (f === null || t === null || f === t) continue;
    const pieces =
      f < t
        ? [[f, t]]
        : [
            [f, 24 * 60],
            [0, t],
          ];
    for (const [a, z] of pieces) {
      const p = el("span", (e) => (e.className = s.rotBandPiece));
      p.style.left = `${(a / (24 * 60)) * 100}%`;
      p.style.width = `${((z - a) / (24 * 60)) * 100}%`;
      if (x.n.trim() === "0") p.dataset.zero = "true";
      b.appendChild(p);
    }
  }
  return b;
}

/** 「其余时段 ＝ …」那一层（后端 `effective` 的 `below`）写成的字。 */
export function restText(c: CapAt): string {
  const what =
    c.v === null
      ? copyText("rot.cap.restNone")
      : c.layer === "all"
        ? copyText("rot.cap.restAll", { n: c.v })
        : c.layer === "trigger"
          ? copyText("rot.cap.restTrig", { w: slotLabel(c.w ?? ""), n: c.v })
          : copyText("rot.cap.fixedShort", { n: c.v });
  return copyText("rot.cap.rest", { what });
}

/** 名称那一格的错（核心写好的那一句）；那台拒了却没点名称这一格 ⇒「未保存」。规则列表改名 · 规则编辑器 · 新建规则同读这一处。 */
export function nameError(errors: CellError[]): string {
  return errors.find((x) => x.cell === "name")?.said ?? copyText("rot.save.failed");
}

/**
 * 封顶浮层（锚在号那一行的「封顶」按钮上）：`不设 | 固定 | 按时段` 三选（浮层里改草稿，分段不写盘）· 固定 `上限 [99] %` ·
 * 按时段每段一行 `[17:00]–[02:00] 上限 [0] ✕` ＋ `＋ 时段` ＋ 24h 色带。按「存」⇒ 先交后端逐格校验，没错才 `write`。
 */
export function openCapEditor(
  anchor: HTMLElement,
  origin: Origin,
  r: Rotation,
  account: string,
  w: string,
  write: (r: Rotation) => void,
): void {
  const cur = r.cap?.[account]?.[w];
  let mode: CapMode =
    cur === undefined ? "none" : typeof cur === "number" ? "fixed" : "slots";
  let fixed = typeof cur === "number" ? String(cur) : "99";
  let slots: Slot[] = Array.isArray(cur)
    ? cur.map((x) => ({
        from: x.at.slice(0, 5),
        to: x.at.slice(6),
        n: String(x.n),
      }))
    : [{ from: "17:00", to: "02:00", n: "0" }];
  let errors: CellError[] = [];
  // 按时段时色带下那一行「其余时段 ＝ …」：开浮层时问后端一次（这一格的下一层取到什么，`rotation-plan` 的 `effective`）。
  let rest: string | null = null;
  const root = el("div", (e) => (e.className = s.rotCap));
  root.dataset.rotCap = account;
  void readPlan(origin, { rotation: r }).then(
    (p) => {
      const c = p.effective[account]?.[w];
      if (!c) return;
      // 全部窗口那一格下面是两窗各一条线：其余时段照后端那一句两窗各取多少。
      rest =
        c.belowList !== undefined
          ? copyText("rot.cap.rest", { what: c.belowList })
          : restText(c.below);
      if (mode === "slots" && root.isConnected) paint();
    },
    (e: unknown) => console.warn("[rot] rotation-plan 失败：", e),
  );
  const draft = (): CapValue | undefined | null => {
    const num = (t: string): number | null =>
      /^\d{1,3}$/.test(t.trim()) ? Number(t.trim()) : null;
    if (mode === "none") return undefined;
    if (mode === "fixed") {
      const n = num(fixed);
      return n === null ? null : n;
    }
    // 写错的数原样交后端（它回 range）：这里只把字转成数，转不成的记成 -1 交出去也会被拒，故先挡在界面这一侧说「0–99」。
    const out = slots.map((x) => ({
      at: `${x.from.trim()}-${x.to.trim()}`,
      n: num(x.n) ?? -1,
    }));
    return out;
  };
  const paint = (): void => {
    root.replaceChildren();
    const head = el("div", (e) => (e.className = s.rotCapHead));
    head.appendChild(
      el(
        "span",
        (e) => (e.className = s.rotCapTitle),
        copyText("rot.cap.title", {
          acct: account === ALL ? copyText("acct.rot.trigger") : account,
          w: w === ALL ? copyText("rot.cap.colAll") : slotLabel(w),
        }),
      ),
    );
    head.appendChild(
      segmented<CapMode>({
        items: [
          { key: "none", label: copyText("rot.cap.none") },
          { key: "fixed", label: copyText("rot.cap.fixed") },
          { key: "slots", label: copyText("rot.cap.slots") },
        ],
        current: mode,
        label: copyText("rot.cap.label"),
        onChange: (k) => {
          mode = k;
          errors = [];
          paint();
        },
      }),
    );
    root.appendChild(head);
    const cellErr = (i: number | null): CellError[] =>
      errors.filter(
        (e) =>
          e.cell ===
          (i === null ? `cap.${account}.${w}` : `cap.${account}.${w}[${i}]`),
      );
    if (mode === "fixed") {
      const row = el("div", (e) => (e.className = s.rotCapRow));
      const n = numInput(Number(fixed) || 0, copyText("rot.cap.limit"));
      n.value = fixed;
      n.addEventListener("input", () => (fixed = n.value));
      row.append(
        el(
          "span",
          (e) => (e.className = s.rotLabel),
          copyText("rot.cap.limit"),
        ),
        n,
        el(
          "span",
          (e) => (e.className = s.rotUnit),
          copyText("acct.rot.pctUnit"),
        ),
        el("span", (e) => (e.className = s.rotHint), copyText("rot.cap.zero")),
      );
      root.appendChild(row);
      for (const e of cellErr(null)) {
        n.dataset.error = "true";
        root.appendChild(
          el("div", (e) => (e.className = s.rotErr), e.said),
        );
      }
    }
    if (mode === "slots") {
      slots.forEach((x, i) => {
        const row = el("div", (e) => (e.className = s.rotCapRow));
        row.dataset.rotSlot = String(i);
        const time = (
          v: string,
          set: (t: string) => void,
          label: string,
        ): HTMLInputElement => {
          const t = document.createElement("input");
          t.type = "text";
          t.className = s.rotTime;
          t.value = v;
          t.setAttribute("aria-label", label);
          t.addEventListener("input", () => {
            set(t.value);
            bandBox.replaceChildren(band(slots));
          });
          return t;
        };
        const f = time(x.from, (t) => (x.from = t), copyText("rot.cap.from"));
        const t = time(x.to, (v) => (x.to = v), copyText("rot.cap.to"));
        const n = numInput(Number(x.n) || 0, copyText("rot.cap.limit"));
        n.value = x.n;
        n.addEventListener("input", () => {
          x.n = n.value;
          bandBox.replaceChildren(band(slots));
        });
        const del = button({
          label: copyText("rot.cap.removeSlot"),
          icon: "close",
          kind: "icon",
          size: "compact",
          hint: copyText("rot.cap.removeSlot"),
          onClick: () => {
            slots = slots.filter((_, j) => j !== i);
            errors = [];
            paint();
          },
        });
        row.append(
          f,
          el(
            "span",
            (e) => (e.className = s.rotUnit),
            copyText("rot.cap.dash"),
          ),
          t,
          el(
            "span",
            (e) => (e.className = s.rotLabel),
            copyText("rot.cap.limit"),
          ),
          n,
          del,
        );
        root.appendChild(row);
        const es = cellErr(i);
        for (const e of es) {
          for (const box of e.code === "range"
            ? [n]
            : e.code === "overlap"
              ? [f, t]
              : [f, t])
            box.dataset.error = "true";
          root.appendChild(
            el("div", (e) => (e.className = s.rotErr), e.said),
          );
        }
      });
      const add = button({
        label: copyText("rot.cap.addSlot"),
        icon: "plus",
        kind: "ghost",
        size: "compact",
        onClick: () => {
          slots = [...slots, { from: "", to: "", n: "99" }];
          paint();
        },
      });
      root.appendChild(add);
      const bandBox = el("div", (e) => (e.className = s.rotBandBox));
      bandBox.appendChild(band(slots));
      root.appendChild(bandBox);
      if (rest !== null) {
        const line = el("div", (e) => (e.className = s.rotHint), rest);
        line.dataset.rotRest = "";
        root.appendChild(line);
      }
    }
    const foot = el("div", (e) => (e.className = s.rotCapFoot));
    const ok = button({
      label: copyText("rot.cap.ok"),
      kind: "primary",
      size: "compact",
      onClick: () => void commit(),
    });
    foot.append(
      button({
        label: copyText("rot.cap.cancel"),
        kind: "ghost",
        size: "compact",
        onClick: () => closePopover(),
      }),
      ok,
    );
    root.appendChild(foot);
  };
  const commit = async (): Promise<void> => {
    const v = draft();
    if (v === null) {
      // 框里填的不是数（还交不了核心）⇒ 就地标这一格，那一句与核心「越界」那一句同一条。
      errors = [{ cell: `cap.${account}.${w}`, code: "range", said: copyText("rot.capErr.range") }];
      paint();
      return;
    }
    const next = withCap(r, account, w, v);
    try {
      errors = await checkRotation(origin, next);
    } catch (e) {
      console.warn("[rot] rotation-plan 失败：", e);
      errors = [];
    }
    if (errors.length > 0) {
      paint();
      return;
    }
    closePopover();
    write(next);
  };
  root.addEventListener("keydown", (ev) => {
    if (
      ev.key === "Enter" &&
      !ev.isComposing &&
      (ev.target as HTMLElement).tagName === "INPUT"
    ) {
      ev.preventDefault();
      void commit();
    }
  });
  paint();
  openPopover(anchor, root, { label: copyText("rot.cap.label"), align: "end" });
}

/** 号那一行行尾的「封顶」小按钮（有设置时显示内容）：点开封顶浮层（编的是这号「全部窗口」那一格）。 */
export function capButton(
  origin: Origin,
  r: Rotation,
  account: string,
  write: (r: Rotation) => void,
): HTMLButtonElement {
  const v = r.cap?.[account]?.[ALL];
  const b = button({
    label: capShort(v),
    kind: "secondary",
    size: "compact",
    onClick: () => openCapEditor(b, origin, r, account, ALL, write),
  });
  b.dataset.rotCapBtn = account;
  if (v !== undefined) b.dataset.set = "true";
  return b;
}

// ─────────────────────────────── 来源下拉里的只读小卡

/**
 * 来源下拉里一条规则悬停 300ms 浮出的只读小卡：名字 · 顺序（起始账号占位 ＋ 勾上的号，行尾封顶标签与兜底，与面板列表同排法）·
 * 后端写好的那句说明（触发 · 换法 · 无号可换都在里面）。看完不用切。
 */
export function rulePeek(rule: RuleRow): HTMLElement {
  const box = el("div", (e) => (e.className = s.rotPeek));
  box.appendChild(el("div", (e) => (e.className = s.rotCapTitle), rule.name));
  const r = rule.rotation;
  for (const slot of r.order) {
    const row = el("div", (e) => (e.className = s.rotPeekRow));
    if (typeof slot !== "string") {
      row.dataset.peekRow = "start";
      row.appendChild(
        el(
          "span",
          (e) => (e.className = s.rotPeekStart),
          copyText("rot.ed.start"),
        ),
      );
    } else {
      if (!r.enabled.includes(slot)) continue;
      row.dataset.peekRow = slot;
      row.append(
        accountAvatarEl(slot, { size: 16 }),
        el("span", null, accountLabel(slot)),
      );
      for (const t of capTags(r, slot))
        row.appendChild(el("span", (e) => (e.className = s.rotPeekTag), t));
      if ((r.fallback ?? []).includes(slot)) row.appendChild(fallbackMark());
    }
    box.appendChild(row);
  }
  if (rule.explain)
    box.appendChild(el("div", (e) => (e.className = s.rotHint), rule.explain));
  return box;
}

// ─────────────────────────────── 存为规则…

/**
 * 「存为规则…」浮层（锚在来源下拉旁）：`名称 [ ]` · `☑ 本会话改用这条规则` · ［取消］［存］。名称对错后端判（空 · 重名 · 超长），照它红字。
 * 存成 ⇒ `onSaved(规则, 勾没勾)`。
 */
export function openSaveAsRule(
  anchor: HTMLElement,
  origin: Origin,
  rotation: Rotation,
  onSaved: (rule: RuleRow, link: boolean) => void,
): void {
  const root = el("div", (e) => (e.className = s.rotSave));
  root.dataset.rotSave = "true";
  root.appendChild(
    el("div", (e) => (e.className = s.rotCapTitle), copyText("rot.save.title")),
  );
  const name = field({ label: copyText("rot.save.name"), noteOnDemand: true });
  root.appendChild(name.root);
  const link = document.createElement("label");
  link.className = s.rotCheck;
  const c = document.createElement("input");
  c.type = "checkbox";
  c.checked = true;
  link.append(c, document.createTextNode(copyText("rot.save.link")));
  root.appendChild(link);
  const go = async (): Promise<void> => {
    name.setError(null);
    let got;
    try {
      got = await saveRule(origin, { name: name.input.value, rotation });
    } catch (e) {
      console.warn("[rot] rotation-rule-save 失败：", e);
      name.setError(copyText("rot.save.failed"));
      return;
    }
    if (got.state === "saved") {
      closePopover();
      onSaved(got.rule, c.checked);
      return;
    }
    if (got.state === "refused") {
      name.setError(nameError(got.errors));
      return;
    }
    name.setError(copyText("rot.save.failed"));
  };
  name.input.addEventListener("keydown", (e) => {
    const ev = e as KeyboardEvent;
    if (ev.key === "Enter" && !ev.isComposing) {
      ev.preventDefault();
      void go();
    }
  });
  const foot = el("div", (e) => (e.className = s.rotCapFoot));
  const ok = button({
    label: copyText("rot.save.ok"),
    kind: "primary",
    size: "compact",
    onClick: () => void go(),
  });
  ok.dataset.rotSaveOk = "true";
  foot.append(
    button({
      label: copyText("rot.cap.cancel"),
      kind: "ghost",
      size: "compact",
      onClick: () => closePopover(),
    }),
    ok,
  );
  root.appendChild(foot);
  openPopover(anchor, root, {
    label: copyText("rot.save.title"),
    align: "start",
  });
  name.input.focus();
}
