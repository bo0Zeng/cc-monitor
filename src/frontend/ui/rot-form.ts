/**
 * 照策略声明生成的表单：界面不为哪一份策略写专门的表单 —— 每项声明一格，类型定格的样子，
 * 字（参数名 · 灰字 · 选项）全取后端照请求语言写好的那几样（`rotation-strategies-read` 的 `DeclShown`）。
 *
 * - 改一格只回 `onSet(k, v)`；值原样交后端（`rotation-session-edit` · 配置编辑器的草稿）逐格校验，界面不判范围；
 *   收不下的那一格由调用方把后端回的错（`cellErrorsFor`）交回来，画在那一格下。
 * - 号详情 · `参数…` · 配置编辑器三处同一个件。
 * 判据：`tests/frontend/ui/rot-form.vitest.ts`。
 */
import { copyText } from "./copy-table";
import { slotLabel } from "./acct-words";
import { icon } from "./kit/icon";
import { checkbox, radio } from "./kit/switch";
import { button } from "./kit/button";
import { select } from "./kit/select";
import type { CellError } from "./generated/CellError";
import type { DeclShown } from "./generated/DeclShown";
import s from "./rot-form.module.css";

/** 一格的错（后端写好的那一句）。 */
export interface FieldError {
  k: string;
  said: string;
}

export interface FormSpec {
  decls: readonly DeclShown[];
  values: Readonly<Record<string, unknown>>;
  /** 单选组名的前缀（同一页里几张表单不串组）。 */
  name: string;
  onSet: (k: string, v: unknown) => void;
  /** 跟原样比改过的格（左侧一道琥珀线）。 */
  changed?: ReadonlySet<string>;
  /** 原样那一格的值（`line` 旁写「原 99%」）。 */
  was?: Readonly<Record<string, unknown>>;
  errors?: readonly FieldError[];
  /** 按量号：`line` · `slots` 两格不出（没有窗口）。 */
  metered?: boolean;
  /** 时段那几个钟点按哪个时区（配置的 `tz`）。 */
  tz?: string;
  /** 不画的格（「新加的号缺省」不含 `slots`）。 */
  skip?: readonly string[];
  /** 窄（手机 · 号表一格）：灰字收起。 */
  compact?: boolean;
}

/** 选项多过这么多 ⇒ 下拉；两个以内横排。 */
const RADIO_MAX = 4;
const INLINE_MAX = 2;

/** 新加一段时段的样子（稿：加了再改起止与线）。 */
const NEW_SLOT = { at: "00:00-06:00", line: 80 };

/** 后端回的错里落在 `prefix` 那一行的（`params` · `acctInit` · `accounts[i]`）：`prefix.k…` ⇒ `{k, said}`。 */
export function cellErrorsFor(errors: readonly CellError[], prefix: string): FieldError[] {
  const out: FieldError[] = [];
  for (const e of errors) {
    if (!e.cell.startsWith(`${prefix}.`)) continue;
    const k = /^[^.[]+/.exec(e.cell.slice(prefix.length + 1))?.[0];
    if (k) out.push({ k, said: e.said });
  }
  return out;
}

/** 建一个元素：`cls` 里给它挂类（写成 `(e) => (e.className = s.x)`，叠没叠的量具认得出挂到哪）。 */
function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: (e: HTMLElementTagNameMap[K]) => void, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  cls(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

/** 一句带一个 `{n}` 的字切成前后两截（中间放输入格）。 */
function around(text: (n: string) => string): [string, string] {
  const MARK = "\u0001";
  const [a, b = ""] = text(MARK).split(MARK);
  return [a.trimEnd(), b.trimStart()];
}

/** 数字格：填的字照原样转（能成数 ⇒ 数；空 ⇒ `null`；别的原样交后端判）。 */
function numberBox(value: unknown, onCommit: (v: unknown) => void, aria: string, d?: DeclShown): HTMLInputElement {
  const i = el("input", (e) => (e.className = s.rfNum));
  i.type = "text";
  i.inputMode = "numeric";
  i.value = typeof value === "number" ? String(value) : "";
  i.placeholder = copyText("rot.form.empty");
  i.setAttribute("aria-label", aria);
  if (d?.min !== undefined && d.max !== undefined) i.title = copyText("rot.form.outOfRange", { min: d.min, max: d.max });
  i.addEventListener("change", () => {
    const t = i.value.trim();
    onCommit(t === "" ? null : Number.isFinite(Number(t)) ? Number(t) : t);
  });
  return i;
}

function clockBox(value: string, onCommit: (v: string) => void, aria: string): HTMLInputElement {
  const i = el("input", (e) => (e.className = s.rfClock));
  i.type = "text";
  i.value = value;
  i.setAttribute("aria-label", aria);
  i.addEventListener("change", () => onCommit(i.value.trim()));
  return i;
}

function hintEl(text: string | undefined, compact: boolean): HTMLElement | null {
  return text && !compact ? el("div", (e) => (e.className = s.rfHint), text) : null;
}

function boolField(d: DeclShown, v: unknown, o: FormSpec): HTMLElement[] {
  const box = checkbox(d.label, v === true, (on) => o.onSet(d.k, on));
  return [box, ...[hintEl(d.hint, o.compact === true)].filter((x): x is HTMLElement => x !== null)];
}

function numField(d: DeclShown, v: unknown, o: FormSpec, f: HTMLElement): HTMLElement[] {
  f.dataset.row = "true";
  const label = el("span", (e) => (e.className = s.rfLabel), d.label);
  const out: HTMLElement[] = [label, numberBox(v, (x) => o.onSet(d.k, x), d.label, d)];
  const h = hintEl(d.hint, o.compact === true);
  if (h) {
    h.dataset.wide = "true";
    out.push(h);
  }
  return out;
}

function choiceField(d: DeclShown, v: unknown, o: FormSpec, f: HTMLElement): HTMLElement[] {
  const opts: { v: string; label: string; hint?: string }[] = d.options ?? (d.of ?? []).map((x) => ({ v: x, label: x }));
  const out: HTMLElement[] = [el("div", (e) => (e.className = s.rfLabel), d.label)];
  if (opts.length > RADIO_MAX) {
    const sel = select({
      label: d.label,
      options: opts.map((x) => ({ value: x.v, label: x.label, note: o.compact ? undefined : x.hint })),
      value: typeof v === "string" ? v : undefined,
      onChange: (x) => o.onSet(d.k, x),
    });
    out.push(sel.el);
    return out;
  }
  if (opts.length <= INLINE_MAX) f.dataset.inline = "true";
  const g = el("div", (e) => (e.className = s.rfOpts));
  g.setAttribute("role", "radiogroup");
  g.setAttribute("aria-label", d.label);
  for (const x of opts) {
    const r = radio(`${o.name}-${d.k}`, x.label, v === x.v, () => o.onSet(d.k, x.v));
    if (x.hint && !o.compact) r.appendChild(el("span", (e) => (e.className = s.rfOptHint), x.hint));
    g.appendChild(r);
  }
  out.push(g);
  return out;
}

type LineV = number | { "5h"?: number | null; "7d"?: number | null } | null;

function lineField(d: DeclShown, v: unknown, o: FormSpec): HTMLElement[] {
  const head = el("div", (e) => (e.className = s.rfLabel), d.label);
  const was = o.was?.[d.k];
  if (o.changed?.has(d.k) && was !== undefined) {
    head.appendChild(
      el("span", (e) => (e.className = s.rfWas), typeof was === "number" ? copyText("rot.form.was", { n: was }) : copyText("rot.form.wasText", { v: was === null ? copyText("rot.form.full") : lineText(was as LineV) })),
    );
  }
  const line = v as LineV;
  const split = line !== null && typeof line === "object";
  const g = el("div", (e) => (e.className = s.rfOpts));
  g.dataset.inline = "true";
  const name = `${o.name}-${d.k}`;
  const at = radio(name, "", line !== null, () => o.onSet(d.k, typeof d.init === "number" ? d.init : 90));
  at.lastChild?.remove();
  if (split) {
    for (const w of ["5h", "7d"] as const) {
      const [a, b] = around((n) => copyText("rot.form.lineAt", { n }));
      const piece = el("span", (e) => (e.className = s.rfInline));
      const box = numberBox((line as Record<string, unknown>)[w], (x) => o.onSet(d.k, { ...(line as object), [w]: x }), `${d.label} ${slotLabel(w)}`);
      box.dataset.line = w;
      piece.append(el("span", (e) => (e.className = s.rfW), slotLabel(w)), document.createTextNode(a), box, document.createTextNode(b));
      at.appendChild(piece);
    }
  } else {
    const [a, b] = around((n) => copyText("rot.form.lineAt", { n }));
    const box = numberBox(typeof line === "number" ? line : null, (x) => o.onSet(d.k, x), d.label);
    box.dataset.line = "all";
    at.append(document.createTextNode(a), box, document.createTextNode(b));
  }
  const full = radio(name, copyText("rot.form.full"), line === null, () => o.onSet(d.k, null));
  g.append(at, full);
  if (!o.compact) {
    const sw = el("button", (e) => (e.className = s.rfLink), split ? copyText("rot.form.joined") : copyText("rot.form.split"));
    sw.type = "button";
    sw.dataset.lineSplit = "true";
    sw.addEventListener("click", () => {
      if (split) {
        const l = line as Record<string, number | null | undefined>;
        o.onSet(d.k, l["5h"] ?? l["7d"] ?? null);
      } else {
        const n = typeof line === "number" ? line : typeof d.init === "number" ? d.init : 90;
        o.onSet(d.k, { "5h": n, "7d": n });
      }
    });
    g.appendChild(sw);
  }
  return [head, g];
}

function lineText(l: LineV): string {
  if (l === null) return copyText("rot.form.full");
  if (typeof l === "number") return copyText("rot.form.was", { n: l }).replace(/^\S+\s*/, "");
  return ["5h", "7d"].map((w) => `${slotLabel(w)} ${(l as Record<string, unknown>)[w] ?? copyText("rot.form.empty")}`).join(copyText("kit.text.sep"));
}

interface Slot {
  at: string;
  line: unknown;
}

function slotsField(d: DeclShown, v: unknown, o: FormSpec): HTMLElement[] {
  const slots: Slot[] = Array.isArray(v) ? (v as Slot[]).map((x) => ({ at: String(x.at ?? ""), line: x.line })) : [];
  const head = el("div", (e) => (e.className = s.rfLabel), d.label);
  if (o.tz) head.appendChild(el("span", (e) => (e.className = s.rfTz), copyText("rot.form.tz", { tz: o.tz })));
  const out: HTMLElement[] = [head];
  const put = (next: Slot[]): void => o.onSet(d.k, next);
  if (slots.length === 0) out.push(el("div", (e) => (e.className = s.rfSlotNone), copyText("rot.form.slotNone")));
  slots.forEach((sl, i) => {
    const row = el("div", (e) => (e.className = s.rfSlot));
    row.dataset.slot = String(i);
    const [from, to = ""] = sl.at.split("-");
    const set = (patch: Partial<Slot>): void => put(slots.map((x, j) => (j === i ? { ...x, ...patch } : x)));
    const [a, b] = around((n) => copyText("rot.form.slotLine", { n }));
    const drop = el("button", (e) => (e.className = s.rfDrop));
    drop.type = "button";
    drop.appendChild(icon("close", "compact"));
    drop.dataset.slotDrop = "true";
    drop.setAttribute("aria-label", copyText("rot.form.slotDrop"));
    drop.addEventListener("click", () => put(slots.filter((_, j) => j !== i)));
    row.append(
      clockBox(from, (x) => set({ at: `${x}-${to}` }), copyText("rot.form.slotFrom")),
      el("span", (e) => (e.className = s.rfDash), copyText("rot.form.dash")),
      clockBox(to, (x) => set({ at: `${from}-${x}` }), copyText("rot.form.slotTo")),
      document.createTextNode(a),
      numberBox(sl.line, (x) => set({ line: x }), d.label),
      document.createTextNode(b),
      drop,
    );
    out.push(row);
  });
  const add = el("button", (e) => (e.className = s.rfLink), copyText("rot.form.slotAdd"));
  add.type = "button";
  add.dataset.slotAdd = "true";
  add.addEventListener("click", () => put([...slots, { ...NEW_SLOT }]));
  out.push(add);
  if (slots.length > 0) {
    const band = dayBand(slots, o.values.line);
    if (band) out.push(band);
    const h = hintEl(copyText("rot.form.slotRest"), o.compact === true);
    if (h) out.push(h);
  }
  return out;
}

/** 24h 色带：一天里哪几截按时段另算（写那一段的线；0 ＝ 这段不用），其余照这个号的线。 */
function dayBand(slots: readonly Slot[], line: unknown): HTMLElement | null {
  const mins = (t: string): number | null => {
    const m = /^(\d{2}):(\d{2})$/.exec(t);
    return m ? Number(m[1]) * 60 + Number(m[2]) : null;
  };
  const band = el("div", (e) => (e.className = s.rfBand));
  band.setAttribute("aria-hidden", "true");
  const base = el("i", (e) => (e.className = s.rfBandBase), typeof line === "number" ? String(line) : "");
  band.appendChild(base);
  const piece = (f: number, t: number, text: string, off: boolean): void => {
    const p = el("i", (e) => (e.className = s.rfBandSlot), text);
    p.style.left = `${(f / 1440) * 100}%`;
    p.style.width = `${((t - f) / 1440) * 100}%`;
    if (off) p.dataset.unused = "true";
    band.appendChild(p);
  };
  for (const sl of slots) {
    const [a, b] = sl.at.split("-");
    const f = mins(a ?? "");
    const t = mins(b ?? "");
    if (f === null || t === null || f === t) continue;
    const text = typeof sl.line === "number" ? String(sl.line) : "";
    const off = sl.line === 0;
    if (f < t) piece(f, t, text, off);
    else {
      piece(f, 1440, text, off);
      piece(0, t, text, off);
    }
  }
  const wrap = el("div", (e) => (e.className = s.rfBandWrap));
  const axis = el("div", (e) => (e.className = s.rfBandAxis));
  for (const h of [0, 6, 12, 18, 24]) {
    const t = el("span", (e) => (e.className = s.rfBandTick), String(h).padStart(2, "0"));
    t.style.left = `${(h / 24) * 100}%`;
    axis.appendChild(t);
  }
  wrap.append(band, axis);
  return wrap;
}

/** ★ 一张表单：声明的先后一项一格。 */
export function paramForm(o: FormSpec): HTMLElement {
  const root = el("div", (e) => (e.className = s.rfForm));
  root.dataset.rotForm = o.name;
  let meteredSaid = false;
  for (const d of o.decls) {
    if (o.skip?.includes(d.k)) continue;
    const v = o.values[d.k] === undefined ? d.init : o.values[d.k];
    const f = el("div", (e) => (e.className = s.rfField));
    f.dataset.rotField = d.k;
    f.dataset.type = d.type;
    if (o.changed?.has(d.k)) f.dataset.changed = "true";
    if (o.metered && (d.type === "line" || d.type === "slots")) {
      // 按量号没有窗口：两格合成一句（只写一次）。
      if (d.type === "slots" || meteredSaid) continue;
      meteredSaid = true;
      f.append(el("div", (e) => (e.className = s.rfLabel), d.label), el("div", (e) => (e.className = s.rfHint), copyText("rot.form.metered")));
      root.appendChild(f);
      continue;
    }
    const parts =
      d.type === "bool"
        ? boolField(d, v, o)
        : d.type === "minutes" || d.type === "points" || d.type === "percent"
          ? numField(d, v, o, f)
          : d.type === "choice"
            ? choiceField(d, v, o, f)
            : d.type === "line"
              ? lineField(d, v, o)
              : slotsField(d, v, o);
    f.append(...parts);
    const errs = (o.errors ?? []).filter((e) => e.k === d.k);
    if (errs.length > 0) {
      f.dataset.error = "true";
      for (const e of errs) {
        const line = el("div", (e) => (e.className = s.rfError), e.said);
        line.setAttribute("role", "alert");
        f.appendChild(line);
      }
    }
    root.appendChild(f);
  }
  return root;
}

/** 多选那一条：每个每号参数一格（空着 ＝ 不改）＋ `全部改成`（同配置编辑器）。 */
export function bulkBar(decls: readonly DeclShown[], n: number, values: Record<string, unknown>, set: (k: string, v: unknown) => void, apply: () => void): HTMLElement {
  const bar = el("div", (e) => (e.className = s.rfBulk));
  bar.dataset.rotBulk = String(n);
  bar.appendChild(el("span", (e) => (e.className = s.rfBulkN), copyText("rot.bulk.picked", { n })));
  for (const d of decls) {
    if (d.type === "slots" || d.type === "bool") continue;
    const cell = el("label", (e) => (e.className = s.rfBulkCell));
    cell.appendChild(el("span", (e) => (e.className = s.rfBulkLabel), d.label));
    if (d.type === "choice") {
      const sel = select({
        label: d.label,
        options: [{ value: "", label: copyText("rot.bulk.noChange") }, ...(d.options ?? []).map((o) => ({ value: o.v, label: o.label }))],
        value: typeof values[d.k] === "string" ? (values[d.k] as string) : "",
        onChange: (v) => set(d.k, v === "" ? undefined : v),
      });
      cell.appendChild(sel.el);
    } else {
      const i = el("input", (e) => (e.className = s.rfBulkNum));
      i.type = "text";
      i.inputMode = "numeric";
      i.placeholder = copyText("rot.form.empty");
      i.dataset.bulk = d.k;
      i.value = typeof values[d.k] === "number" ? String(values[d.k]) : "";
      i.addEventListener("change", () => {
        const t = i.value.trim();
        set(d.k, t === "" ? undefined : Number.isFinite(Number(t)) ? Number(t) : t);
      });
      cell.appendChild(i);
    }
    bar.appendChild(cell);
  }
  bar.appendChild(el("span", (e) => (e.className = s.rfSpacer)));
  const go = button({ label: copyText("rot.bulk.apply"), kind: "primary", size: "compact", onClick: apply });
  go.dataset.rotBulkApply = "true";
  bar.appendChild(go);
  return bar;
}
