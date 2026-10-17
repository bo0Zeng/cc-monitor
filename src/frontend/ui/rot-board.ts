/**
 * 轮换（第五版）的四块：第一行（选策略 · ⋯ · 没存的改动）＋ 说明 · 提示 · 轮换账号（号牌 ＋ 点开一个号）· 总体设置 · 模拟（时间轴）。
 * 会话面板与设置窗「轮换」栏画的是同一个，只是目标不同（`{sid}` · `{strategy, settings}`）。
 *
 * - 全照 `rotation-view` 与 `rotation-strategies` 的回包画：标题 ＝ 文件名、说明 ＝ 文件开头的注释、格照策略的 `form`
 *   （名字 `label`，悬停 `help`）、先后 · 换号时刻 · 停到几点 · 跳过哪段照 `decision` · `timeline`，时刻的字照回包里写好的 `…Text`。
 *   界面不判、不排、不算时刻、不拼句子；手势经 `BoardActs` 翻成 typed 命令（调用方发）。
 * - 设置那几格照回包的 `board` 画（此刻的值 · 从哪来 · 字都是后端解好的；跟总体的画虚线）；改一格 ⇒ 交一个意图（`acts.edit`），后端改完、校验、重读。
 * 判据：`tests/frontend/ui/rot-board.vitest.ts`。
 */
import { accountAvatarEl } from "./account-color";
import { clockFace } from "./clock-face";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import { openMenu, type MenuItem } from "./kit/menu";
import { closePopover, openPopover } from "./kit/popover";
import { hintText } from "./rot-words";
import { select } from "./kit/select";
import { toggleSwitch } from "./kit/switch";
import { segmented } from "./kit/tabs";
import { attachTooltip } from "./kit/tooltip";
import type { Fact } from "./generated/Fact";
import type { RotCell } from "./generated/RotCell";
import type { RotDaySeg } from "./generated/RotDaySeg";
import type { RotEdit } from "./generated/RotEdit";
import type { RotRow } from "./generated/RotRow";
import type { RotTempOp } from "./generated/RotTempOp";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import type { RotPlate } from "./generated/RotPlate";
import type { RotationViewReply } from "./generated/RotationViewReply";
import s from "./rot-board.module.css";

/** 画的时候界面手上的那几样（不进后端）：推几小时 · 点开了哪个号 · 「现在用」被拦下的那一句 · 旧格式那几份展开没有。 */
export interface BoardState {
  hours: 12 | 24;
  open: string | null;
  refused: { account: string; text: string } | null;
  oldOpen?: boolean;
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
  /** 改了一格 / 刷了一段 / 拖了号牌 / 移出 / 加号：一个意图（后端照声明改）。 */
  edit(e: RotEdit): void;
  save(): void;
  revert(): void;
  useNow?: (account: string) => void;
  temp?: (op: RotTempOp) => void;
  hours(h: 12 | 24): void;
  open(account: string | null): void;
  /** 提示里的按钮：照「在文件窗口打开」那一路开一份文件（看代码 · 旧格式那几份）。 */
  showFile(path: string): void;
  /** 设时区（设成提示里后端给的那个）。 */
  setTz(zone: string): void;
  /** 别处改了：载入新的 · 留着这边的。 */
  loadNew(): void;
  keepMine(): void;
  /** 旧格式那几份展开 / 收起。 */
  toggleOld(): void;
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

/** 视图里这个号的号牌（后端带好号色 · 号名 · 号牌字）；视图里没有 ⇒ `undefined`。 */
function acctIn(view: RotationViewReply | null | undefined, account: string): RotPlate | undefined {
  return view?.accounts.find((a) => a.account === account)?.plate;
}

/** 号名：照视图里那一格；视图里没有的号照原名。 */
function nameIn(view: RotationViewReply | null | undefined, account: string): string {
  return acctIn(view, account)?.label ?? account;
}

/** 号的头像（号色照视图里那一格；视图里没有 ⇒ 中性的头像）。 */
function avatar(view: RotationViewReply | null | undefined, account: string, origin: Origin): HTMLElement {
  const plate = acctIn(view, account);
  return accountAvatarEl(account, { size: 16, origin, color: plate?.color ?? null, plate: plate?.text });
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
          : st.summary,
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
        avatar(spec.view, v.start, spec.origin),
        " ",
        nameIn(spec.view, v.start),
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

/** 提示里的一颗按钮。 */
function hintButton(label: string, onClick: () => void, opt: { ghost?: boolean; hint?: string; act: string }): HTMLElement {
  const b = button({ label, kind: opt.ghost ? "ghost" : "secondary", size: "compact", hint: opt.hint, onClick });
  b.dataset.rotAct = opt.act;
  return b;
}

/** 提示：一条一句（后端写好的），按钮写动作，要的东西（文件在哪 · 设成哪个时区 · 哪几份 · 复制详情）都是后端给的。 */
function hints(spec: BoardSpec): HTMLElement[] {
  const v = spec.view;
  if (!v) return [];
  const { acts } = spec;
  return v.hints.flatMap((n) => {
    const t = hintText(n, v);
    const h = n.hint;
    const btns: HTMLElement[] = [];
    let below: HTMLElement | null = null;
    if (h.k === "strategyError" && h.path !== undefined) {
      const path = h.path;
      btns.push(hintButton(copyText("rot.hint.code"), () => acts.showFile(path), { act: "code" }));
    } else if (h.k === "tzUnknown") {
      const zone = h.zone;
      btns.push(hintButton(copyText("rot.hint.setTz"), () => acts.setTz(zone), { act: "tz", hint: copyText("rot.hint.setTzTip", { zone }) }));
    } else if (h.k === "oldScripts") {
      btns.push(hintButton(copyText("rot.hint.which"), () => acts.toggleOld(), { act: "which" }));
      if (spec.state.oldOpen) {
        below = el("div", (e) => (e.className = s.rotOldList));
        for (const f of h.files) {
          const b = hintButton(f.name, () => acts.showFile(f.path), { ghost: true, act: "old", hint: f.path });
          below.appendChild(b);
        }
      }
    } else if (h.k === "newer") {
      btns.push(
        hintButton(copyText("rot.hint.loadNew"), () => acts.loadNew(), { act: "loadNew" }),
        hintButton(copyText("rot.hint.keepMine"), () => acts.keepMine(), { ghost: true, act: "keepMine" }),
      );
    }
    const b = banner(t.tone, t.text, btns, n.detail ?? "");
    b.dataset.rotHint = h.k;
    return below ? [b, below] : [b];
  });
}

// ─────────────────────────────── 号牌

/** 号牌上的两条小计量（上 5h、下 7d）；悬停那一句后端写好。 */
function meters(f: Fact, row: RotRow | undefined): HTMLElement {
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
  if (row) attachTooltip(m, row.hover);
  return m;
}

function cards(spec: BoardSpec, view: RotationViewReply): HTMLElement {
  const wrap = el("div", (e) => (e.className = s.rotCards));
  const listed = view.accounts.slice(0, view.listed);
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
      avatar(view, f.account, spec.origin),
      el("span", (e) => (e.className = s.rotCardName), nameIn(view, f.account)),
      meters(f, view.board.rows[i]),
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
        if (to >= 0 && to !== i) spec.acts.edit({ move: { from: i, to } });
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
        label: nameIn(view, f.account),
        avatar: avatar(view, f.account, spec.origin),
        onClick: () => spec.acts.edit({ add: { account: f.account } }),
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
  const row = view.board.rows[i];
  const pane = el("div", (e) => (e.className = s.rotPane));
  pane.dataset.account = account;
  const top = el("div", (e) => (e.className = s.rotPaneTop));
  top.append(
    avatar(view, account, spec.origin),
    el("b", (e) => (e.className = s.rotPaneName), nameIn(view, account)),
  );
  top.appendChild(el("span", (e) => (e.className = s.rotPaneLine), row?.line ?? ""));
  top.appendChild(el("span", (e) => (e.className = s.rotSpacer)));
  if (spec.acts.useNow && account !== view.current) {
    const use = spec.acts.useNow;
    top.appendChild(
      button({
        label: copyText("rot.acct.useNow", { account: nameIn(view, account) }),
        kind: "secondary",
        size: "compact",
        onClick: () => use(account),
      }),
    );
  }
  top.appendChild(
    button({
      label: copyText("rot.acct.remove"),
      kind: "ghost",
      size: "compact",
      onClick: () => spec.acts.edit({ remove: { row: i } }),
    }),
  );
  pane.appendChild(top);
  if (spec.state.refused?.account === account) {
    const why = el("div", (e) => (e.className = s.rotRefused), spec.state.refused.text);
    why.setAttribute("role", "status");
    pane.appendChild(why);
  }
  const grid = el("div", (e) => (e.className = s.rotCells));
  for (const cell of row?.cells ?? []) grid.append(...cellRows(spec, cell, i));
  pane.appendChild(grid);
  return pane;
}

// ─────────────────────────────── 格


function label(cell: RotCell): HTMLElement {
  const k = el("span", (e) => (e.className = s.rotCellKey), cell.label);
  if (cell.from === "all") k.dataset.inherited = "true";
  if (cell.help !== undefined) attachTooltip(k, cell.help);
  return k;
}

function numberBox(
  value: number,
  cell: RotCell,
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
  input.setAttribute("aria-label", cell.label);
  input.addEventListener("change", () => {
    const n = Number(input.value);
    if (input.value.trim() !== "" && Number.isFinite(n)) onSet(n);
  });
  box.append(input, el("span", (e) => (e.className = s.rotNumUnit), unit));
  return box;
}

/** 数格（百分数 · 分钟）：可空的带一颗空值说法的按钮；空着时数画虚线。 */
function numberCell(cell: RotCell, unit: string, set: (next: unknown) => void): HTMLElement[] {
  const ctl = el("span", (e) => (e.className = s.rotCtl));
  const num = cell.num ?? 0;
  const inherited = cell.from === "all";
  if (cell.nullable !== undefined) {
    const isNull = cell.v === null;
    const nul = button({
      label: cell.nullable,
      kind: "secondary",
      size: "compact",
      onClick: () => set(isNull ? num : null),
    });
    nul.classList.add(s.rotNull);
    nul.setAttribute("aria-pressed", String(isNull));
    ctl.append(nul, numberBox(num, cell, unit, isNull || inherited, (n) => set(n)));
  } else {
    ctl.append(numberBox(num, cell, unit, inherited, (n) => set(n)));
  }
  return [label(cell), ctl];
}

/** 一格 ⇒ `[标签, 控件]`（排进格子）；`row` ＝ 号列表第几行（总体那几格 ⇒ `null`）。 */
function cellRows(spec: BoardSpec, cell: RotCell, row: number | null): HTMLElement[] {
  const inRow = row === null ? {} : { row };
  const set = (v: unknown): void => spec.acts.edit({ set: { key: cell.key, v, ...inRow } });
  switch (cell.kind) {
    case "toggle": {
      const sw = toggleSwitch({ label: cell.label, on: cell.v === true, onChange: (on) => set(on) });
      sw.root.classList.add(s.rotSwitch);
      return [label(cell), sw.root];
    }
    case "percent":
      return numberCell(cell, copyText("rot.unit.percent"), set);
    case "minutes":
      return numberCell(cell, copyText("rot.unit.minutes"), set);
    case "choice": {
      const choices = cell.choices ?? [];
      const cur = typeof cell.v === "string" ? cell.v : "";
      if (choices.length <= 3)
        return [
          label(cell),
          segmented({
            label: cell.label,
            items: choices.map((c) => ({ key: c.v, label: c.label })),
            current: cur,
            onChange: (k) => set(k),
          }),
        ];
      const sel = select({
        label: cell.label,
        options: choices.map((c) => ({ value: c.v, label: c.label })),
        value: cur,
        onChange: (k) => set(k),
      });
      return [label(cell), sel.el];
    }
    case "daySegments": {
      const win = cell.window ?? "";
      return [
        label(cell),
        dayBar(cell.segs ?? [], cell.from === "all", (from, to, v) =>
          spec.acts.edit({ day: { key: cell.key, window: win, from, to, v, ...inRow } }),
        ),
      ];
    }
  }
}

// ─────────────────────────────── 24 小时条

const DAY = 1440;
/** 拖的时候按半小时吸附。 */
const SNAP = 30;

/** 24 小时条：段与字照回包画；拖出一截、选个值 ⇒ `paint(from, to, v)`（`v: null` ＝ 删这截）。 */
function dayBar(
  segs: readonly RotDaySeg[],
  inherited: boolean,
  paint: (from: number, to: number, v: number | null) => void,
): HTMLElement {
  const wrap = el("div", (e) => (e.className = s.rotDay));
  const bar = el("div", (e) => (e.className = s.rotDayBar));
  bar.dataset.dayBar = "";
  if (inherited) bar.dataset.inherited = "true";
  for (const g of segs) {
    const from = Number(g.from);
    const to = Number(g.to);
    const seg = el("span", (e) => (e.className = s.rotDaySeg), g.text);
    if (g.v === 0) seg.dataset.zero = "true";
    at(seg, from / DAY, (to - from) / DAY);
    attachTooltip(seg, g.tip);
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
    paintPop(bar, from, to, paint, () => ghost?.remove());
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
  anchor: HTMLElement,
  from: number,
  to: number,
  onPaint: (from: number, to: number, v: number | null) => void,
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
    onPaint(from % DAY, to % DAY, v);
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
  // 左端：过去那一截的开头（会话才有；没有 ⇒ 此刻）。过去的段实心，推演的段浅色虚边，此刻一条竖线。
  const past = view.past;
  const a = past?.from ?? view.now;
  const b = view.now + spec.state.hours * 3600;
  const X = (t: number): number => (Math.max(a, Math.min(b, t)) - a) / (b - a);
  const nowX = X(view.now);
  const nowLine = (track: HTMLElement): void => {
    if (!past) return;
    const l = el("span", (e) => (e.className = s.rotSimNow));
    l.dataset.rotNow = "true";
    at(l, nowX);
    track.appendChild(l);
  };
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
  if (nowX === 0) now.dataset.edge = "true";
  at(now, nowX);
  axis.appendChild(now);
  for (const t of tl.ticks) {
    // 挨着「此刻」那一格的刻度让开（字会叠在一起）；贴着左端的也让开（半个字会出界）。
    const x = X(t.at);
    if (t.at >= b || t.at <= a || Math.abs(x - nowX) < (nowX === 0 ? 0.14 : 0.08) || x < 0.04) continue;
    const k = el("span", (e) => (e.className = s.rotSimTick), t.text);
    at(k, X(t.at));
    axis.appendChild(k);
  }
  box.appendChild(grid(null, axis));
  // 将来每次换号：时刻 ＋ 换到谁。
  const sw = el("div", (e) => (e.className = s.rotSimSwitches));
  // 两排轮着放：同一排里前后两个至少隔 18%，放不下就放另一排；两排都放不下就省掉（时刻在最上那条的段里看得出）。
  const lanes = [-1, -1];
  for (const w of tl.switches) {
    if (w.at <= view.now || w.at >= b) continue;
    const x = X(w.at);
    const lane = x - lanes[0] >= 0.18 ? 0 : x - lanes[1] >= 0.18 ? 1 : -1;
    if (lane < 0) continue;
    lanes[lane] = x;
    const k = el(
      "span",
      (e) => (e.className = s.rotSimSwitch),
      copyText("rot.sim.switch", {
        at: w.atText,
        account: nameIn(view, w.account),
      }),
    );
    at(k, x);
    if (lane === 1) k.dataset.lane = "1";
    sw.appendChild(k);
  }
  box.appendChild(grid(null, sw));
  // 最上面那条：照推演每段用哪个号（停发 ⇒ 停下）；拖出来的那几段白描边加圆点；轮到时用不了的画「跳过」。
  const top = el("div", (e) => (e.className = s.rotSimTop));
  const useColor = (seg: HTMLElement, account: string | undefined | null): void => {
    if (account) {
      const slot = acctIn(view, account)?.color;
      if (slot !== undefined) seg.dataset.c = String(slot);
    } else seg.dataset.hold = "true";
  };
  // 接着此刻、跟推演第一段同一个号的那一段不再写号名（同一段只写一次，写在推演那一截）。
  const first = tl.uses[0];
  for (const u of past?.uses ?? []) {
    const joins = u.to === view.now && first !== undefined && first.from === view.now && (first.account ?? null) === (u.account ?? null);
    const seg = el("span", (e) => (e.className = s.rotSimUse), joins ? "" : u.account ? nameIn(view, u.account) : copyText("rot.sim.hold"));
    place(seg, u.from, u.to);
    useColor(seg, u.account);
    seg.dataset.past = "true";
    top.appendChild(seg);
  }
  for (const u of tl.uses) {
    if (u.to <= a || u.from >= b) continue;
    const seg = el(
      "span",
      (e) => (e.className = s.rotSimUse),
      u.account ? nameIn(view, u.account) : copyText("rot.sim.hold"),
    );
    place(seg, u.from, u.to);
    useColor(seg, u.account);
    if (u.fromTemp) seg.dataset.temp = "true";
    if (u.from > a) seg.dataset.future = "true";
    top.appendChild(seg);
    for (const sk of u.skipped) {
      const k = el(
        "span",
        (e) => (e.className = s.rotSimSkip),
        copyText("rot.sim.skip", { account: nameIn(view, sk) }),
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
    const at = tl.uses.filter((u) => u.from > view.now && u.from < t).length + 1;
    spec.acts.temp({ place: { account, at } });
  };
  nowLine(top);
  box.appendChild(grid(null, top, true));
  // 每个号一行：只画这个号自己的额度；字只有两种（此刻那段 剩 N% · 重置后那段 ↻HH:MM），发不出去的段靠斜纹认。
  for (const lane of tl.lanes) {
    const head = el("span", (e) => (e.className = s.rotSimHead));
    head.append(
      avatar(view, lane.account, spec.origin),
      el("span", (e) => (e.className = s.rotSimName), nameIn(view, lane.account)),
    );
    attachTooltip(head, nameIn(view, lane.account));
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
    // 过去那一截：只有说得出的几段（说不出的空着）；字只有重置那段的 ↻HH:MM。
    for (const g of past?.lanes.find((l) => l.account === lane.account)?.segs ?? []) {
      const seg = el("span", (e) => (e.className = s.rotSimSeg), g.text);
      place(seg, g.from, g.to);
      seg.dataset.state = g.sendable ? "ok" : "out";
      seg.dataset.past = "true";
      track.appendChild(seg);
    }
    lane.segs.forEach((g) => {
      if (g.to <= a || g.from >= b) return;
      const seg = el("span", (e) => (e.className = s.rotSimSeg), g.text);
      place(seg, g.from, g.to);
      seg.dataset.state = g.sendable ? "ok" : "out";
      if (g.from > a) seg.dataset.future = "true";
      track.appendChild(seg);
    });
    nowLine(track);
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
  const all = view.board.all;
  if (all.length > 0) {
    root.appendChild(header(copyText("rot.h.all")));
    const grid = el("div", (e) => (e.className = s.rotCells));
    for (const cell of all) grid.append(...cellRows(spec, cell, null));
    root.appendChild(grid);
  }
  if (view.timeline) {
    root.appendChild(simHeader(spec, view));
    root.appendChild(simulation(spec, view));
  }
  return root;
}
