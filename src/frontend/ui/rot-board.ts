/**
 * 轮换（第五版）的四块：第一行（选策略 · ⋯ · 没存的改动）＋ 说明 · 提示 · 轮换账号（号牌 ＋ 点开一个号）· 总体设置 · 模拟（时间轴）。
 * 会话面板与设置窗「轮换」栏画的是同一个（`轮换-v5.md`「布局」），只是目标不同（`{sid}` · `{strategy, settings}`）。
 *
 * - 全照 `rotation-view` 与 `rotation-strategies` 的回包画：标题 ＝ 文件名、说明 ＝ 文件开头的注释、格照策略的 `form`
 *   （名字 `label`，悬停 `help`）、先后 · 换号时刻 · 停到几点 · 跳过哪段照 `decision` · `timeline`，时刻的字照回包里写好的 `…Text`。
 *   界面不判、不排、不算时刻、不拼句子；手势经 `BoardActs` 翻成 typed 命令（调用方发）。
 * - 设置是策略自己的形状（`settings`），这里只照声明取、放：`scope: all` ⇒ `settings[key]`；`scope: each` ⇒ `settings.accounts[i][key]`
 *   （`inherit` 且号没另设 ⇒ 跟总体，画虚线）。改一格 ⇒ 整份新设置交 `acts.settings`（后端照声明校验）。
 * 判据：`tests/frontend/ui/rot-board.vitest.ts`。
 */
import { accountLabel } from "./acct-words";
import { accountAvatarEl, accountColorOf } from "./account-color";
import { clockFace } from "./clock-face";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import { openMenu, type MenuItem } from "./kit/menu";
import { closePopover, openPopover } from "./kit/popover";
import { firstLine } from "./rot-reads";
import { isObj } from "./ipc/decode";
import { hintText } from "./rot-words";
import { select } from "./kit/select";
import { toggleSwitch } from "./kit/switch";
import { segmented } from "./kit/tabs";
import { attachTooltip } from "./kit/tooltip";
import type { Fact } from "./generated/Fact";
import type { RotFormCell } from "./generated/RotFormCell";
import type { RotSeg } from "./generated/RotSeg";
import type { RotTempOp } from "./generated/RotTempOp";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import type { RotationViewReply } from "./generated/RotationViewReply";
import type { StrategyText } from "./generated/StrategyText";
import s from "./rot-board.module.css";

type Obj = Record<string, unknown>;

/** 画的时候界面手上的那几样（不进后端）：推几小时 · 点开了哪个号 · 「现在用」被拦下的那一句。 */
export interface BoardState {
  hours: 12 | 24;
  open: string | null;
  refused: { account: string; text: string } | null;
}

/** 手势 ⇒ 命令（调用方发、发完重读重画）。会话才有的几样（现在用 · 时间轴拖动 · 还原）在设置页缺。 */
export interface BoardActs {
  /** 下拉选了一份（`null` ＝ 不轮换）。 */
  pick(id: string | null): void;
  copy(id: string): void;
  rename(id: string, rev: string): void;
  forNew(id: string): void;
  remove(id: string): void;
  add(): void;
  /** 改了一格 / 拖了号牌 / 移出 / 加号：整份新设置。 */
  settings(next: Obj): void;
  save(): void;
  revert(): void;
  /** 24 小时条上刷一段（`v: null` ＝ 删这段）⇒ 刷完的那一组（后端算）。 */
  dayPaint(
    segs: RotSeg[],
    from: number,
    to: number,
    v: number | null,
  ): Promise<RotSeg[]>;
  useNow?: (account: string) => void;
  temp?: (op: RotTempOp) => void;
  hours(h: 12 | 24): void;
  open(account: string | null): void;
}

export interface BoardSpec {
  view: RotationViewReply | null;
  list: RotationStrategiesReply | null;
  state: BoardState;
  acts: BoardActs;
  origin: Origin;
  /** `session` ＝ 会话面板；`settings` ＝ 设置窗（第一行右边写「设置 · 那台」，选中的 ＝ 新会话用的）。 */
  where: "session" | "settings";
  /** 设置窗第一行右边那台的名字。 */
  machine?: string;
}

// ─────────────────────────────── 小件

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  cls: (e: HTMLElementTagNameMap[K]) => void,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  cls(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

/** 摆到轴上：左起几成 · 宽几成（0–1；CSS 里乘 100% —— 排版几何交样式表，不在这里拼带单位的串）。 */
function at(e: HTMLElement, x: number, w?: number): void {
  e.style.setProperty("--rot-x", String(x));
  if (w !== undefined) e.style.setProperty("--rot-w", String(w));
}

/** 号的头像（照它那台账号库的号色）。 */
function avatar(account: string, origin: Origin): HTMLElement {
  return accountAvatarEl(account, { size: 16, origin });
}

/** 策略写的字（一句，或分语言）。 */
export function textOf(t: StrategyText | undefined): string {
  if (t === undefined) return "";
  return typeof t === "string" ? t : t.zh;
}


/** 设置里号列表那一排（`settings.accounts`）。 */
function rowsOf(settings: unknown): Obj[] {
  const a = isObj(settings) ? settings.accounts : undefined;
  return Array.isArray(a) ? a.filter(isObj) : [];
}

/** 换掉号列表那一排，别的原样（整份新的）。 */
function withRows(settings: unknown, rows: Obj[]): Obj {
  return { ...(isObj(settings) ? settings : {}), accounts: rows };
}

/** 一天里的分钟 ⇒ `HH:MM`（24:00 写成 24:00）。 */
function dayClock(min: number): string {
  return min >= 1440 ? copyText("rot.day.end") : clockFace(min * 60_000, 0);
}

// ─────────────────────────────── 第一行

function firstRow(spec: BoardSpec): HTMLElement {
  const { view, list, acts } = spec;
  const row = el("div", (e) => (e.className = s.rotTop));
  const strategies = list?.strategies ?? [];
  const current =
    spec.where === "settings"
      ? (list?.forNew ?? "")
      : (view?.strategy?.id ?? "");
  const sel = select({
    label: copyText("rot.top.pick"),
    menuWidth: 340,
    options: [
      {
        value: "",
        label: copyText("rot.none.label"),
        detail: copyText("rot.none.desc"),
      },
      ...strategies.map((st) => ({
        value: st.id,
        label: st.id,
        note:
          st.forNew && spec.where === "session"
            ? copyText("rot.tag.forNew")
            : undefined,
        detail: st.error
          ? copyText("rot.top.unreadable")
          : firstLine(st.description),
      })),
    ],
    value: current,
    onChange: (v) => acts.pick(v === "" ? null : v),
  });
  sel.el.classList.add(s.rotPick);
  row.appendChild(sel.el);
  const item = strategies.find((x) => x.id === current);
  const more = button({
    label: copyText("rot.more.label"),
    kind: "icon",
    size: "compact",
    icon: "more",
    hint: copyText("rot.more.label"),
  });
  more.addEventListener("click", () => {
    const items: MenuItem[] = [];
    if (item) {
      items.push(
        { label: copyText("rot.more.copy"), onClick: () => acts.copy(item.id) },
        {
          label: copyText("rot.more.rename"),
          onClick: () => acts.rename(item.id, item.rev),
        },
      );
      if (spec.where === "session")
        items.push({
          label: copyText("rot.more.forNew"),
          enabled: !item.forNew,
          onClick: () => acts.forNew(item.id),
        });
      const busy = item.inUse.length;
      items.push({
        label: copyText("rot.more.delete"),
        danger: true,
        enabled: busy === 0,
        why: busy > 0 ? copyText("rot.more.inUse", { n: busy }) : undefined,
        title: busy > 0 ? item.inUse.join(copyText("kit.text.sep")) : undefined,
        onClick: () => acts.remove(item.id),
      });
      items.push({ divider: true, label: "" });
    }
    items.push({ label: copyText("rot.more.add"), onClick: () => acts.add() });
    openMenu({ el: more }, items, { label: copyText("rot.more.label") });
  });
  row.appendChild(more);
  row.appendChild(el("span", (e) => (e.className = s.rotSpacer)));
  if (view?.dirty) {
    row.append(
      el("span", (e) => (e.className = s.rotDirty), copyText("rot.top.dirty")),
      button({
        label: copyText("rot.top.revert"),
        kind: "ghost",
        size: "compact",
        onClick: () => acts.revert(),
      }),
      button({
        label: copyText("rot.top.save"),
        kind: "primary",
        size: "compact",
        onClick: () => acts.save(),
      }),
    );
  } else if (spec.where === "settings") {
    row.appendChild(
      el(
        "span",
        (e) => (e.className = s.rotWhere),
        copyText("rot.top.here", { machine: spec.machine ?? "" }),
      ),
    );
  }
  return row;
}

/** 第一行下面的说明（选中那份的开头注释；不轮换 ⇒ 只用开会话的号）。 */
function description(spec: BoardSpec): HTMLElement {
  const box = el("div", (e) => (e.className = s.rotAbout));
  const v = spec.view;
  if (!v?.strategy) {
    box.append(copyText("rot.none.desc"));
    if (v && spec.where === "session") {
      box.append(
        " ",
        avatar(v.start, spec.origin),
        " ",
        accountLabel(v.start),
      );
    }
    return box;
  }
  for (const para of v.strategy.description.split("\n\n")) {
    if (para.trim() === "") continue;
    box.appendChild(el("p", (e) => (e.className = s.rotAboutPara), para));
  }
  return box;
}

// ─────────────────────────────── 提示

function hints(spec: BoardSpec): HTMLElement[] {
  const v = spec.view;
  if (!v) return [];
  return v.hints.map((h) => {
    const t = hintText(h, v);
    const b = banner(t.tone, t.text);
    b.dataset.rotHint = h.hint.k;
    return b;
  });
}

// ─────────────────────────────── 号牌

/** 号牌上的两条小计量（上 5h、下 7d）。 */
function meters(f: Fact): HTMLElement {
  const m = el("span", (e) => (e.className = s.rotMeters));
  for (const [w, wf] of [
    ["5h", f.w5h],
    ["7d", f.w7d],
  ] as const) {
    const r = el("span", (e) => (e.className = s.rotMeterRow));
    const bar = el("span", (e) => (e.className = s.rotMeter));
    const fill = el("i", (e) => (e.className = s.rotMeterFill));
    at(fill, 0, Math.max(0, Math.min(100, wf.used)) / 100);
    if (wf.used >= 100) fill.dataset.full = "true";
    bar.appendChild(fill);
    r.append(
      el(
        "em",
        (e) => (e.className = s.rotMeterWin),
        w === "5h" ? copyText("rot.card.w5h") : copyText("rot.card.w7d"),
      ),
      bar,
    );
    m.appendChild(r);
  }
  attachTooltip(
    m,
    copyText("rot.card.hover", {
      u5: Math.round(f.w5h.used),
      u7: Math.round(f.w7d.used),
    }),
  );
  return m;
}

function cards(spec: BoardSpec, view: RotationViewReply): HTMLElement {
  const wrap = el("div", (e) => (e.className = s.rotCards));
  const listed = view.accounts.slice(0, view.listed);
  const rows = rowsOf(view.settings);
  listed.forEach((f, i) => {
    const c = el("button", (e) => (e.className = s.rotCard));
    c.type = "button";
    c.dataset.account = f.account;
    c.dataset.rotCard = "";
    if (f.account === view.current && spec.where === "session")
      c.dataset.inUse = "true";
    if (f.blocked) c.dataset.blocked = "true";
    c.setAttribute("aria-expanded", String(spec.state.open === f.account));
    c.append(
      icon("drag", "compact"),
      el("span", (e) => (e.className = s.rotCardIx), String(i + 1)),
      avatar(f.account, spec.origin),
      el("span", (e) => (e.className = s.rotCardName), accountLabel(f.account)),
      meters(f),
    );
    // 按下拖过 4px ⇒ 拖号牌改默认优先级（松在哪张号牌上 ⇒ 排到它前头）；没拖开 ⇒ 点：展开 / 收起这个号。
    c.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      const x0 = e.clientX;
      const y0 = e.clientY;
      let moving = false;
      const move = (m: PointerEvent): void => {
        if (!moving && Math.hypot(m.clientX - x0, m.clientY - y0) > 4) {
          moving = true;
          c.dataset.lifted = "true";
        }
      };
      const up = (u: PointerEvent): void => {
        document.removeEventListener("pointermove", move);
        document.removeEventListener("pointerup", up);
        delete c.dataset.lifted;
        if (!moving) {
          spec.acts.open(spec.state.open === f.account ? null : f.account);
          return;
        }
        // 拖的时候面板可能已经照推送重画过（这一排换了一份新的）⇒ 照此刻页上的号牌找落点，按号认第几张。
        const hit = [...document.querySelectorAll<HTMLElement>("button[data-rot-card]")].find((k) => {
          const r = k.getBoundingClientRect();
          return u.clientX >= r.left && u.clientX <= r.right && u.clientY >= r.top && u.clientY <= r.bottom;
        });
        const to = hit ? listed.findIndex((x) => x.account === hit.dataset.account) : -1;
        if (to < 0 || to === i || i >= rows.length) return;
        const next = rows.slice();
        const [moved] = next.splice(i, 1);
        next.splice(to > i ? to - 1 : to, 0, moved);
        spec.acts.settings(withRows(view.settings, next));
      };
      document.addEventListener("pointermove", move);
      document.addEventListener("pointerup", up);
    });
    c.addEventListener("keydown", (e) => {
      if (e.key !== "Enter" && e.key !== " ") return;
      e.preventDefault();
      spec.acts.open(spec.state.open === f.account ? null : f.account);
    });
    wrap.appendChild(c);
  });
  const rest = view.accounts.slice(view.listed);
  const add = button({
    label: copyText("rot.card.add"),
    kind: "secondary",
    size: "compact",
    icon: "plus",
  });
  add.classList.add(s.rotCardAdd);
  if (rest.length === 0) add.disabled = true;
  add.addEventListener("click", () => {
    openMenu(
      { el: add },
      rest.map((f) => ({
        label: accountLabel(f.account),
        avatar: avatar(f.account, spec.origin),
        onClick: () =>
          spec.acts.settings(
            withRows(view.settings, [...rows, { id: f.account }]),
          ),
      })),
      { label: copyText("rot.card.add") },
    );
  });
  wrap.appendChild(add);
  return wrap;
}

/** 点开的那个号：头一行（名 · 此刻用量 · 重置）＋ 现在用 · 移出 ＋ `scope: each` 那几格。 */
function accountPane(
  spec: BoardSpec,
  view: RotationViewReply,
  account: string,
): HTMLElement | null {
  const i = view.accounts.findIndex((f) => f.account === account);
  if (i < 0 || i >= view.listed) return null;
  const f = view.accounts[i];
  const pane = el("div", (e) => (e.className = s.rotPane));
  pane.dataset.account = account;
  const top = el("div", (e) => (e.className = s.rotPaneTop));
  top.append(
    avatar(account, spec.origin),
    el("b", (e) => (e.className = s.rotPaneName), accountLabel(account)),
  );
  const reset = f.w5h.resetsText
    ? copyText("rot.acct.reset", { at: f.w5h.resetsText })
    : "";
  top.appendChild(
    el(
      "span",
      (e) => (e.className = s.rotPaneLine),
      [
        copyText("rot.acct.line", {
          u5: Math.round(f.w5h.used),
          u7: Math.round(f.w7d.used),
        }),
        reset,
      ]
        .filter((x) => x !== "")
        .join(copyText("kit.text.sep")),
    ),
  );
  top.appendChild(el("span", (e) => (e.className = s.rotSpacer)));
  if (spec.acts.useNow && account !== view.current) {
    const use = spec.acts.useNow;
    top.appendChild(
      button({
        label: copyText("rot.acct.useNow", { account: accountLabel(account) }),
        kind: "secondary",
        size: "compact",
        onClick: () => use(account),
      }),
    );
  }
  const rows = rowsOf(view.settings);
  top.appendChild(
    button({
      label: copyText("rot.acct.remove"),
      kind: "ghost",
      size: "compact",
      onClick: () =>
        spec.acts.settings(
          withRows(
            view.settings,
            rows.filter((_, k) => k !== i),
          ),
        ),
    }),
  );
  pane.appendChild(top);
  if (spec.state.refused?.account === account) {
    const why = el("div", (e) => (e.className = s.rotRefused), spec.state.refused.text);
    why.setAttribute("role", "status");
    pane.appendChild(why);
  }
  const grid = el("div", (e) => (e.className = s.rotCells));
  for (const cell of view.form.filter((c) => c.scope === "each"))
    grid.append(...cellRows(spec, view, cell, i));
  pane.appendChild(grid);
  return pane;
}

// ─────────────────────────────── 格


/** 一格此刻的值：号另设的 · 跟总体的 · 声明里的默认。 */
function valueOf(
  view: RotationViewReply,
  cell: RotFormCell,
  row: number | null,
): { v: unknown; inherited: boolean } {
  const settings = isObj(view.settings) ? view.settings : {};
  if (row !== null) {
    const r = rowsOf(view.settings)[row] ?? {};
    if (r[cell.key] !== undefined) return { v: r[cell.key], inherited: false };
    if (cell.inherit)
      return { v: (settings[cell.key] !== undefined ? settings[cell.key] : cell.default), inherited: true };
    return { v: cell.default, inherited: false };
  }
  return { v: (settings[cell.key] !== undefined ? settings[cell.key] : cell.default), inherited: false };
}

/** 放回一格（整份新设置）。 */
function put(
  view: RotationViewReply,
  cell: RotFormCell,
  row: number | null,
  v: unknown,
): Obj {
  const settings: Obj = isObj(view.settings) ? { ...view.settings } : {};
  if (row === null) {
    settings[cell.key] = v;
    return settings;
  }
  const rows = rowsOf(view.settings).map((r) => ({ ...r }));
  if (!rows[row]) return settings;
  if (v === undefined) delete rows[row][cell.key];
  else rows[row][cell.key] = v;
  return withRows(settings, rows);
}

function label(
  cell: RotFormCell,
  inherited: boolean,
  extra?: string,
): HTMLElement {
  const k = el(
    "span",
    (e) => (e.className = s.rotCellKey),
    extra
      ? copyText("rot.cell.win", { label: textOf(cell.label), win: extra })
      : textOf(cell.label),
  );
  if (inherited) k.dataset.inherited = "true";
  const help = textOf(cell.help);
  if (help !== "") attachTooltip(k, help);
  return k;
}

function numberBox(
  value: number,
  cell: RotFormCell,
  unit: string,
  inherited: boolean,
  onSet: (n: number) => void,
): HTMLElement {
  const box = el("label", (e) => (e.className = s.rotNum));
  if (inherited) box.dataset.inherited = "true";
  const input = document.createElement("input");
  input.type = "number";
  input.className = s.rotNumInput;
  if (cell.min !== undefined) input.min = String(cell.min);
  if (cell.max !== undefined) input.max = String(cell.max);
  input.value = String(value);
  input.setAttribute("aria-label", textOf(cell.label));
  input.addEventListener("change", () => {
    const n = Number(input.value);
    if (input.value.trim() !== "" && Number.isFinite(n)) onSet(n);
  });
  box.append(input, el("span", (e) => (e.className = s.rotNumUnit), unit));
  return box;
}

/** 数格（百分数 · 分钟）：可空的带一颗空值说法的按钮；空着时数画虚线。 */
function numberCell(cell: RotFormCell, v: unknown, inherited: boolean, unit: string, set: (next: unknown) => void): HTMLElement[] {
  const ctl = el("span", (e) => (e.className = s.rotCtl));
  const num =
    typeof v === "number"
      ? v
      : typeof cell.default === "number"
        ? cell.default
        : (cell.min ?? 0);
  if (cell.nullable !== undefined) {
    const isNull = v === null || v === undefined;
    const nul = button({
      label: textOf(cell.nullable),
      kind: "secondary",
      size: "compact",
      onClick: () => set(isNull ? num : null),
    });
    nul.classList.add(s.rotNull);
    nul.setAttribute("aria-pressed", String(isNull));
    ctl.append(
      nul,
      numberBox(num, cell, unit, isNull || inherited, (n) => set(n)),
    );
  } else {
    ctl.append(numberBox(num, cell, unit, inherited, (n) => set(n)));
  }
  return [label(cell, inherited), ctl];
}

/** 一格 ⇒ 一行或几行（`[标签, 控件]` 两两一组排进格子）。 */
function cellRows(
  spec: BoardSpec,
  view: RotationViewReply,
  cell: RotFormCell,
  row: number | null,
): HTMLElement[] {
  const { v, inherited } = valueOf(view, cell, row);
  const set = (next: unknown): void =>
    spec.acts.settings(put(view, cell, row, next));
  switch (cell.kind) {
    case "toggle": {
      const sw = toggleSwitch({
        label: textOf(cell.label),
        on: v === true,
        onChange: (on) => set(on),
      });
      sw.root.classList.add(s.rotSwitch);
      return [label(cell, inherited), sw.root];
    }
    case "percent":
      return numberCell(cell, v, inherited, copyText("rot.unit.percent"), set);
    case "minutes":
      return numberCell(cell, v, inherited, copyText("rot.unit.minutes"), set);
    case "choice": {
      const choices = cell.choices ?? [];
      const cur = typeof v === "string" ? v : "";
      if (choices.length <= 3) {
        return [
          label(cell, inherited),
          segmented({
            label: textOf(cell.label),
            items: choices.map((c) => ({ key: c.v, label: textOf(c.label) })),
            current: cur,
            onChange: (k) => set(k),
          }),
        ];
      }
      const sel = select({
        label: textOf(cell.label),
        options: choices.map((c) => ({ value: c.v, label: textOf(c.label) })),
        value: cur,
        onChange: (k) => set(k),
      });
      return [label(cell, inherited), sel.el];
    }
    case "daySegments": {
      // 每个窗口一条：号另设了这一窗 ⇒ 用它的；否则跟总体（虚线）。
      const out: HTMLElement[] = [];
      const settings = isObj(view.settings) ? view.settings : {};
      const total = isObj(settings[cell.key])
        ? (settings[cell.key] as Obj)
        : {};
      const ownRow =
        row !== null ? rowsOf(view.settings)[row]?.[cell.key] : undefined;
      const own = isObj(ownRow) ? ownRow : {};
      for (const w of cell.windows ?? []) {
        const mine = row === null ? total[w] : own[w];
        const follows =
          row !== null && cell.inherit === true && !Array.isArray(mine);
        const segs = (
          Array.isArray(mine)
            ? mine
            : follows && Array.isArray(total[w])
              ? total[w]
              : []
        ) as RotSeg[];
        const winText =
          w === "5h"
            ? copyText("rot.win.fiveHour")
            : w === "7d"
              ? copyText("rot.win.sevenDay")
              : copyText("rot.win.other", { w });
        out.push(
          label(cell, follows, winText),
          dayBar(spec, segs, follows, (next) => {
            const base = { ...(row === null ? total : own) };
            if (next.length === 0) delete base[w];
            else base[w] = next;
            set(
              row !== null && Object.keys(base).length === 0 ? undefined : base,
            );
          }),
        );
      }
      return out;
    }
  }
}

// ─────────────────────────────── 24 小时条

const DAY = 1440;
/** 拖的时候按半小时吸附。 */
const SNAP = 30;

function dayBar(
  spec: BoardSpec,
  segs: readonly RotSeg[],
  inherited: boolean,
  onSegs: (next: RotSeg[]) => void,
): HTMLElement {
  const wrap = el("div", (e) => (e.className = s.rotDay));
  const bar = el("div", (e) => (e.className = s.rotDayBar));
  bar.dataset.dayBar = "";
  if (inherited) bar.dataset.inherited = "true";
  for (const g of segs) {
    const from = Number(g.from);
    const to = Number(g.to);
    const seg = el(
      "span",
      (e) => (e.className = s.rotDaySeg),
      g.v === 0 ? copyText("rot.day.zero") : copyText("rot.day.v", { v: g.v }),
    );
    if (g.v === 0) seg.dataset.zero = "true";
    at(seg, from / DAY, (to - from) / DAY);
    attachTooltip(
      seg,
      copyText("rot.day.seg", {
        from: dayClock(from),
        to: dayClock(to),
        v: g.v,
      }),
    );
    bar.appendChild(seg);
  }
  const minAt = (x: number): number => {
    const r = bar.getBoundingClientRect();
    const f = r.width > 0 ? (x - r.left) / r.width : 0;
    return Math.max(0, Math.min(DAY, Math.round((f * DAY) / SNAP) * SNAP));
  };
  let start: number | null = null;
  let ghost: HTMLElement | null = null;
  bar.addEventListener("pointerdown", (e) => {
    start = minAt(e.clientX);
    ghost = el("span", (e) => (e.className = s.rotDayGhost));
    bar.appendChild(ghost);
    try {
      bar.setPointerCapture(e.pointerId);
    } catch {
      // 不是真指针（派出来的事件）⇒ 没得捕获，照样画。
    }
  });
  bar.addEventListener("pointermove", (e) => {
    if (start === null || !ghost) return;
    const m = minAt(e.clientX);
    at(ghost, Math.min(start, m) / DAY, Math.abs(m - start) / DAY);
  });
  bar.addEventListener("pointerup", (e) => {
    if (start === null) return;
    const m = minAt(e.clientX);
    let from = Math.min(start, m);
    let to = Math.max(start, m);
    start = null;
    if (to - from < SNAP) {
      // 点一下 ⇒ 点中的那一段整段。
      const hit = segs.find(
        (g) => Number(g.from) <= from && from < Number(g.to),
      );
      from = hit ? Number(hit.from) : 0;
      to = hit ? Number(hit.to) : DAY;
    }
    paintPop(spec, bar, segs, from, to, onSegs, () => ghost?.remove());
  });
  wrap.appendChild(bar);
  const axis = el("div", (e) => (e.className = s.rotDayAxis));
  for (const h of [0, 6, 12, 18, 24]) {
    const t = el("span", (e) => (e.className = s.rotDayTick), dayClock(h * 60).slice(0, 2));
    at(t, h / 24);
    axis.appendChild(t);
  }
  wrap.appendChild(axis);
  return wrap;
}

/** 松手弹出的那一张：`20:00–23:00 · 上限 [0 %]` · 0% · 50% · 100% · 删这段。 */
function paintPop(
  spec: BoardSpec,
  anchor: HTMLElement,
  segs: readonly RotSeg[],
  from: number,
  to: number,
  onSegs: (next: RotSeg[]) => void,
  done: () => void,
): void {
  const pop = el("div", (e) => (e.className = s.rotDayPop));
  pop.appendChild(
    el(
      "span",
      (e) => (e.className = s.rotDayPopText),
      copyText("rot.day.pop", { from: dayClock(from), to: dayClock(to) }),
    ),
  );
  const paint = (v: number | null): void => {
    closePopover();
    // 一整天（0–24）交「相等」，到 24:00 为止交「到 0 点」（后端的写法：`to ≤ from` 跨午夜）。
    void spec.acts
      .dayPaint(segs.slice(), from % DAY, to % DAY, v)
      .then(onSegs, () => undefined);
  };
  const input = document.createElement("input");
  input.type = "number";
  input.min = "0";
  input.max = "100";
  input.className = s.rotNumInput;
  input.setAttribute(
    "aria-label",
    copyText("rot.day.pop", { from: dayClock(from), to: dayClock(to) }),
  );
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.isComposing && input.value.trim() !== "")
      paint(Number(input.value));
  });
  pop.append(input, el("span", (e) => (e.className = s.rotNumUnit), copyText("rot.unit.percent")));
  for (const v of [0, 50, 100])
    pop.appendChild(
      button({
        label: copyText("rot.day.v", { v }),
        kind: "secondary",
        size: "compact",
        onClick: () => paint(v),
      }),
    );
  pop.appendChild(
    button({
      label: copyText("rot.day.del"),
      kind: "ghost",
      size: "compact",
      onClick: () => paint(null),
    }),
  );
  openPopover(anchor, pop, {
    label: copyText("rot.day.popLabel"),
    onClose: done,
  });
  input.focus();
}

// ─────────────────────────────── 模拟（时间轴）

function simulation(spec: BoardSpec, view: RotationViewReply): HTMLElement {
  const tl = view.timeline;
  const box = el("div", (e) => (e.className = s.rotSim));
  if (!tl) return box;
  const a = view.now;
  const b = view.now + spec.state.hours * 3600;
  const X = (t: number): number => (Math.max(a, Math.min(b, t)) - a) / (b - a);
  const place = (e: HTMLElement, from: number, to: number): void => {
    at(e, X(from), Math.max(0, X(to) - X(from)));
  };
  const grid = (
    head: HTMLElement | null,
    body: HTMLElement,
    top = false,
  ): HTMLElement => {
    const r = el("div", (e) => (e.className = top ? s.rotSimTopRow : s.rotSimRow));
    r.append(head ?? el("span", (e) => (e.className = s.rotSimHead)), body);
    return r;
  };
  // 轴：此刻 ＋ 整点刻度（字都是回包写好的）。
  const axis = el("div", (e) => (e.className = s.rotSimAxis));
  const now = el("span", (e) => (e.className = s.rotSimTick), view.nowText);
  now.dataset.now = "true";
  at(now, 0);
  axis.appendChild(now);
  for (const t of tl.ticks) {
    // 挨着「此刻」那一格的刻度让开（字会叠在一起）。
    if (t.at >= b || X(t.at) < 0.08) continue;
    const k = el("span", (e) => (e.className = s.rotSimTick), t.text);
    at(k, X(t.at));
    axis.appendChild(k);
  }
  box.appendChild(grid(null, axis));
  // 将来每次换号：时刻 ＋ 换到谁。
  const sw = el("div", (e) => (e.className = s.rotSimSwitches));
  for (const w of tl.switches) {
    if (w.at <= a || w.at >= b) continue;
    const k = el(
      "span",
      (e) => (e.className = s.rotSimSwitch),
      copyText("rot.sim.switch", {
        at: w.atText,
        account: accountLabel(w.account),
      }),
    );
    at(k, X(w.at));
    sw.appendChild(k);
  }
  box.appendChild(grid(null, sw));
  // 最上面那条：照推演每段用哪个号（停发 ⇒ 停下）；拖出来的那几段白描边加圆点；轮到时用不了的画「跳过」。
  const top = el("div", (e) => (e.className = s.rotSimTop));
  for (const u of tl.uses) {
    if (u.to <= a || u.from >= b) continue;
    const seg = el(
      "span",
      (e) => (e.className = s.rotSimUse),
      u.account ? accountLabel(u.account) : copyText("rot.sim.hold"),
    );
    place(seg, u.from, u.to);
    if (u.account) {
      const slot = accountColorOf(u.account, spec.origin);
      if (slot !== null) seg.dataset.c = String(slot);
    } else seg.dataset.hold = "true";
    if (u.fromTemp) seg.dataset.temp = "true";
    if (u.from > a) seg.dataset.future = "true";
    top.appendChild(seg);
    for (const sk of u.skipped) {
      const k = el(
        "span",
        (e) => (e.className = s.rotSimSkip),
        copyText("rot.sim.skip", { account: accountLabel(sk) }),
      );
      at(k, X(u.from));
      top.appendChild(k);
    }
  }
  // 拖号名到最上面那条：松在哪一刻 ⇒ 排到那一刻之后那一段（此刻那段是第 0 位，在用的号排第一，之后每段换号往后数一位）。
  const dropOnTop = (account: string, x: number, y: number): void => {
    const r = top.getBoundingClientRect();
    if (!spec.acts.temp || y < r.top - 8 || y > r.bottom + 8 || x < r.left || x > r.right) return;
    const t = a + ((x - r.left) / Math.max(1, r.width)) * (b - a);
    const at = tl.uses.filter((u) => u.from > a && u.from < t).length + 1;
    spec.acts.temp({ place: { account, at } });
  };
  box.appendChild(grid(null, top, true));
  // 每个号一行：只画这个号自己的额度；字只有两种（此刻那段 剩 N% · 重置后那段 ↻HH:MM），发不出去的段靠斜纹认。
  for (const lane of tl.lanes) {
    const head = el("span", (e) => (e.className = s.rotSimHead));
    head.append(
      avatar(lane.account, spec.origin),
      el("span", (e) => (e.className = s.rotSimName), accountLabel(lane.account)),
    );
    attachTooltip(head, accountLabel(lane.account));
    if (spec.acts.temp) {
      head.dataset.drag = "true";
      head.addEventListener("pointerdown", (e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        const up = (u: PointerEvent): void => {
          document.removeEventListener("pointerup", up);
          dropOnTop(lane.account, u.clientX, u.clientY);
        };
        document.addEventListener("pointerup", up);
      });
    }
    const track = el("div", (e) => (e.className = s.rotSimTrack));
    lane.segs.forEach((g, k) => {
      if (g.to <= a || g.from >= b) return;
      const text = !g.sendable
        ? ""
        : g.reset
          ? copyText("rot.sim.reset", { at: g.fromText })
          : k === 0
            ? copyText("rot.sim.left", { n: Math.round(g.left) })
            : "";
      const seg = el("span", (e) => (e.className = s.rotSimSeg), text);
      place(seg, g.from, g.to);
      seg.dataset.state = g.sendable ? "ok" : "out";
      if (g.from > a) seg.dataset.future = "true";
      track.appendChild(seg);
    });
    box.appendChild(grid(head, track));
  }
  return box;
}

function simHeader(spec: BoardSpec, view: RotationViewReply): HTMLElement {
  const h = el("div", (e) => (e.className = s.rotHdr));
  h.appendChild(el("span", (e) => (e.className = s.rotHdrText), copyText("rot.h.sim")));
  h.appendChild(el("span", (e) => (e.className = s.rotSpacer)));
  const temp = view.decision?.temp ?? [];
  if (temp.length > 0 && spec.acts.temp) {
    const clear = spec.acts.temp;
    h.append(
      el("span", (e) => (e.className = s.rotHdrNote), copyText("rot.sim.manual", { n: temp.length })),
      button({
        label: copyText("rot.sim.clear"),
        kind: "ghost",
        size: "compact",
        onClick: () => clear("clear"),
      }),
    );
  }
  h.appendChild(
    segmented({
      label: copyText("rot.sim.range"),
      items: [
        { key: "12", label: copyText("rot.sim.h12") },
        { key: "24", label: copyText("rot.sim.h24") },
      ],
      current: String(spec.state.hours) as "12" | "24",
      onChange: (k) => spec.acts.hours(k === "24" ? 24 : 12),
    }),
  );
  return h;
}

function header(text: string, note?: string): HTMLElement {
  const h = el("div", (e) => (e.className = s.rotHdr));
  h.appendChild(el("span", (e) => (e.className = s.rotHdrText), text));
  if (note) h.append(el("span", (e) => (e.className = s.rotSpacer)), el("span", (e) => (e.className = s.rotHdrNote), note));
  return h;
}

// ─────────────────────────────── 整块

/** ★ 四块：照回包画；`view` 还没到 ⇒ 只画第一行。 */
export function board(spec: BoardSpec): HTMLElement {
  const root = el("div", (e) => (e.className = s.rotBoard));
  root.appendChild(firstRow(spec));
  root.appendChild(description(spec));
  root.append(...hints(spec));
  const view = spec.view;
  if (!view?.strategy) return root;
  root.appendChild(
    header(copyText("rot.h.cards"), copyText("rot.h.cardsHint")),
  );
  root.appendChild(cards(spec, view));
  if (spec.state.open !== null) {
    const pane = accountPane(spec, view, spec.state.open);
    if (pane) root.appendChild(pane);
  }
  const all = view.form.filter((c) => c.scope === "all");
  if (all.length > 0) {
    root.appendChild(header(copyText("rot.h.all")));
    const grid = el("div", (e) => (e.className = s.rotCells));
    for (const cell of all) grid.append(...cellRows(spec, view, cell, null));
    root.appendChild(grid);
  }
  if (view.timeline) {
    root.appendChild(simHeader(spec, view));
    root.appendChild(simulation(spec, view));
  }
  return root;
}
