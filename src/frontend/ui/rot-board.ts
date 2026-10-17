/**
 * 轮换（第五版）：第一行（选策略 · ⋯ · 编辑策略）＋ 说明 · 提示，下面会话面板是顺序（本会话的临时动作）· 模拟；
 * 设置窗「轮换」栏是轮换账号（号牌 ＋ 点开一个号）· 总体设置 · 模拟。目标不同（`{sid}` · `{strategy}`），第一行与模拟同一处。
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

/** 手势 ⇒ 命令（调用方发、发完重读重画）。会话才有的几样（现在用 · 临时动作 · 编辑策略）在设置页缺。 */
export interface BoardActs {
  /** 下拉选了一份（`null` ＝ 不轮换）：会话 ⇒ 这个会话用它；设置页 ⇒ 看它（不轮换 ⇒ 新会话不轮换）。 */
  pick(id: string | null): void;
  copy(id: string): void;
  rename(id: string, rev: string): void;
  forNew(id: string): void;
  remove(id: string): void;
  /** 新建策略（问名字，照编辑器那一份建）。 */
  create(): void;
  /** 从文件添加。 */
  add(): void;
  /** 改了一格 / 刷了一段 / 拖了号牌 / 移出 / 加号：一个意图（后端照声明改）。 */
  edit(e: RotEdit): void;
  useNow?: (account: string) => void;
  temp?: (op: RotTempOp) => void;
  /** 开设置窗那台的「轮换」栏、看这个会话用的那份（给了号 ⇒ 开那个号的编辑小窗）。 */
  editStrategy?: (account?: string) => void;
  hours(h: 12 | 24): void;
  open(account: string | null): void;
  /** 提示里的按钮：照「在文件窗口打开」那一路开一份文件（看代码 · 旧格式那几份）。 */
  showFile(path: string): void;
  /** 设时区（设成提示里后端给的那个）。 */
  setTz(zone: string): void;
  /** 策略改过了：会话 ⇒ 载入新版；设置页 ⇒ 在用旧版的会话都载入新版。 */
  loadNew(): void;
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
  const settings = spec.where === "settings";
  const current = view?.strategy?.id ?? "";
  const sel = select({
    label: copyText("rot.top.pick"),
    menuWidth: 340,
    options: [
      {
        value: "",
        label: copyText("rot.none.label"),
        note: list && list.forNew === "" ? copyText("rot.tag.forNew") : undefined,
        detail: copyText("rot.none.desc"),
      },
      ...strategies.map((st) => ({
        value: st.id,
        label: st.id,
        note:
          [st.forNew ? copyText("rot.tag.forNew") : "", st.hand ? copyText("rot.tag.hand") : ""]
            .filter((x) => x !== "")
            .join(copyText("kit.text.sep")) || undefined,
        detail: st.error ? copyText("rot.top.unreadable") : st.summary,
      })),
    ],
    value: current,
    decorate: settings
      ? (items) => [...items, { divider: true, label: "" }, { label: copyText("rot.more.create"), onClick: () => acts.create() }]
      : undefined,
    onChange: (v) => acts.pick(v === "" ? null : v),
  });
  sel.el.classList.add(s.rotPick);
  row.appendChild(sel.el);
  const item = strategies.find((x) => x.id === current);
  if (settings) {
    // 「⋯」：对策略的操作只在设置页。
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
        const busy = item.inUse.length;
        items.push(
          { label: copyText("rot.more.copy"), onClick: () => acts.copy(item.id) },
          { label: copyText("rot.more.rename"), onClick: () => acts.rename(item.id, item.rev) },
          { label: copyText("rot.more.forNew"), enabled: !item.forNew, onClick: () => acts.forNew(item.id) },
          {
            label: copyText("rot.more.delete"),
            danger: true,
            enabled: busy === 0,
            why: busy > 0 ? copyText("rot.more.inUse", { n: busy }) : undefined,
            title: busy > 0 ? item.inUse.join(copyText("kit.text.sep")) : undefined,
            onClick: () => acts.remove(item.id),
          },
          { divider: true, label: "" },
        );
      }
      items.push(
        { label: copyText("rot.more.create"), onClick: () => acts.create() },
        { label: copyText("rot.more.add"), onClick: () => acts.add() },
      );
      openMenu({ el: more }, items, { label: copyText("rot.more.label") });
    });
    row.appendChild(more);
    if (item?.forNew) row.appendChild(el("span", (e) => (e.className = s.rotBadge), copyText("rot.tag.forNewHere")));
  }
  row.appendChild(el("span", (e) => (e.className = s.rotSpacer)));
  if (acts.editStrategy && view?.strategy) {
    const go = acts.editStrategy;
    row.appendChild(
      button({ label: copyText("rot.top.edit"), kind: "secondary", size: "compact", onClick: () => go() }),
    );
  } else if (settings) {
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
    if (spec.where === "settings") {
      const go = spec.acts.create;
      box.append(" ", button({ label: copyText("rot.more.create"), kind: "secondary", size: "compact", onClick: () => go() }));
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
    // 设置页那一形「N 个会话还在用旧版」写在编辑小窗的底行。
    if (h.k === "older") return [];
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
      btns.push(hintButton(copyText("rot.hint.loadNew"), () => acts.loadNew(), { act: "loadNew" }));
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

/** 号名旁的记号（后端照声明给：字形 · 虚线框，悬停那一句）；虚线框记在 `holder` 上。 */
function markEls(row: RotRow | undefined, holder: HTMLElement): HTMLElement[] {
  const out: HTMLElement[] = [];
  for (const m of row?.marks ?? []) {
    if (m.dashed) {
      holder.dataset.dashed = "true";
      attachTooltip(holder, m.tip);
    }
    if (m.glyph === undefined) continue;
    const g = el("span", (e) => (e.className = s.rotMark), m.glyph);
    g.dataset.rotMark = "";
    attachTooltip(g, m.tip);
    out.push(g);
  }
  return out;
}

/** 视图里这个号那一行（号列表外 ⇒ `undefined`）。 */
function rowOf(view: RotationViewReply, account: string): RotRow | undefined {
  const i = view.accounts.findIndex((f) => f.account === account);
  return i >= 0 && i < view.listed ? view.board.rows[i] : undefined;
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
      ...markEls(view.board.rows[i], c),
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

/** 设置页点开一个号出的编辑小窗（贴着号牌、不挤开面板）：头一行 ✕ · `scope: each` 那几格 · 底行（改的是哪份 · 在用旧版的会话 · 移出）。 */
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
  pane.dataset.rotEditor = "";
  pane.setAttribute("role", "dialog");
  pane.setAttribute("aria-label", nameIn(view, account));
  const top = el("div", (e) => (e.className = s.rotPaneTop));
  top.append(
    avatar(view, account, spec.origin),
    el("b", (e) => (e.className = s.rotPaneName), nameIn(view, account)),
    el("span", (e) => (e.className = s.rotPaneLine), row?.line ?? ""),
    el("span", (e) => (e.className = s.rotSpacer)),
    button({ label: copyText("rot.pane.close"), kind: "icon", size: "compact", icon: "close", hint: copyText("rot.pane.close"), onClick: () => spec.acts.open(null) }),
  );
  pane.appendChild(top);
  const grid = el("div", (e) => (e.className = s.rotCells));
  for (const cell of row?.cells ?? []) grid.append(...cellRows(spec, cell, i));
  pane.appendChild(grid);
  const foot = el("div", (e) => (e.className = s.rotPaneFoot));
  const older = view.hints.find((n) => n.hint.k === "older");
  const parts = [copyText("rot.pane.foot", { name: view.strategy?.id ?? "" })];
  if (older) parts.push(older.text);
  else if ((view.strategy?.inUse.length ?? 0) === 0) parts.push(copyText("rot.pane.noUse"));
  foot.appendChild(el("span", (e) => (e.className = s.rotPaneLine), parts.join(copyText("kit.text.sep"))));
  if (older) {
    const b = hintButton(copyText("rot.hint.loadAll"), () => spec.acts.loadNew(), { ghost: true, act: "loadAll" });
    foot.appendChild(b);
  }
  foot.append(
    el("span", (e) => (e.className = s.rotSpacer)),
    button({ label: copyText("rot.acct.remove"), kind: "ghost", size: "compact", onClick: () => spec.acts.edit({ remove: { row: i } }) }),
  );
  pane.appendChild(foot);
  return pane;
}

// ─────────────────────────────── 会话的顺序

/** 这个号此刻怎样：策略交的理由；没有 ⇒ 用不了的原因（后端写好的字）。 */
function nowOf(view: RotationViewReply, account: string): string {
  const why = view.decision?.why.find((r) => r.account === account)?.why;
  return why ?? view.accounts.find((f) => f.account === account)?.blocked?.text ?? "";
}

/** 点一行出的小窗：只看（此刻 · 它在这份策略里的规则）＋ 现在用 · 编辑策略。 */
function orderPop(spec: BoardSpec, view: RotationViewReply, account: string, anchor: HTMLElement): void {
  const pop = el("div", (e) => (e.className = s.rotPane));
  pop.dataset.rotPop = "";
  const top = el("div", (e) => (e.className = s.rotPaneTop));
  top.append(avatar(view, account, spec.origin), el("b", (e) => (e.className = s.rotPaneName), nameIn(view, account)));
  pop.appendChild(top);
  const now = nowOf(view, account);
  if (now !== "") pop.appendChild(el("div", (e) => (e.className = s.rotPaneLine), copyText("rot.order.now", { why: now })));
  const row = rowOf(view, account);
  if (row) pop.appendChild(el("div", (e) => (e.className = s.rotPaneLine), row.line));
  for (const r of row?.rules ?? []) pop.appendChild(el("div", (e) => (e.className = s.rotPaneLine), r));
  const btns = el("div", (e) => (e.className = s.rotPaneTop));
  if (spec.acts.useNow && account !== view.current) {
    const use = spec.acts.useNow;
    btns.appendChild(
      button({
        label: copyText("rot.acct.useNow", { account: nameIn(view, account) }),
        kind: "secondary",
        size: "compact",
        onClick: () => {
          closePopover();
          use(account);
        },
      }),
    );
  }
  if (spec.acts.editStrategy) {
    const go = spec.acts.editStrategy;
    btns.appendChild(
      button({
        label: copyText("rot.top.edit"),
        kind: "ghost",
        size: "compact",
        onClick: () => {
          closePopover();
          go(account);
        },
      }),
    );
  }
  pop.appendChild(btns);
  openPopover(anchor, pop, { label: nameIn(view, account) });
}

/** 会话的顺序：第一格 ＝ 在用的号（后端排好）；每行一个勾（去掉 ＝ 本会话先不用）；拖一行改本会话先后，拖到第一格 ＝ 现在用；点一行出小窗。 */
function order(spec: BoardSpec, view: RotationViewReply): HTMLElement {
  const wrap = el("div", (e) => (e.className = s.rotCards));
  wrap.dataset.rotOrder = "";
  const line = view.lineup;
  if (spec.state.refused) {
    const why = el("div", (e) => (e.className = s.rotRefused), spec.state.refused.text);
    why.setAttribute("role", "status");
    wrap.appendChild(why);
  }
  line.forEach((account, i) => {
    const f = view.accounts.find((x) => x.account === account);
    const r = el("div", (e) => (e.className = s.rotCard));
    r.dataset.account = account;
    r.dataset.rotRow = "";
    if (i === 0) r.dataset.inUse = "true";
    if (f?.blocked) r.dataset.blocked = "true";
    const off = view.off.includes(account);
    if (off) r.dataset.leftOut = "true";
    const tick = document.createElement("input");
    tick.type = "checkbox";
    tick.checked = !off;
    tick.setAttribute("aria-label", copyText("rot.order.use", { account: nameIn(view, account) }));
    tick.addEventListener("pointerdown", (e) => e.stopPropagation());
    tick.addEventListener("change", () => spec.acts.temp?.({ off: { account, off: !tick.checked } }));
    r.append(
      tick,
      icon("drag", "compact"),
      el("span", (e) => (e.className = s.rotCardIx), String(i + 1)),
      avatar(view, account, spec.origin),
      el("span", (e) => (e.className = s.rotCardName), nameIn(view, account)),
      ...markEls(rowOf(view, account), r),
    );
    if (f) r.appendChild(meters(f, undefined));
    r.appendChild(el("span", (e) => (e.className = s.rotPaneLine), nowOf(view, account)));
    // 按下拖过 4px ⇒ 拖这一行（松在第一格 ⇒ 现在用；别的行 ⇒ 本会话先后照拖完的那一串）；没拖开 ⇒ 点：出小窗。
    r.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      const x0 = e.clientX;
      const y0 = e.clientY;
      let moving = false;
      const move = (m: PointerEvent): void => {
        if (!moving && Math.hypot(m.clientX - x0, m.clientY - y0) > 4) {
          moving = true;
          r.dataset.lifted = "true";
        }
      };
      const up = (u: PointerEvent): void => {
        document.removeEventListener("pointermove", move);
        document.removeEventListener("pointerup", up);
        delete r.dataset.lifted;
        if (!moving) return orderPop(spec, view, account, r);
        const hit = [...document.querySelectorAll<HTMLElement>("[data-rot-row]")].find((k) => {
          const b = k.getBoundingClientRect();
          return u.clientX >= b.left && u.clientX <= b.right && u.clientY >= b.top && u.clientY <= b.bottom;
        });
        const to = hit ? line.indexOf(hit.dataset.account ?? "") : -1;
        if (to < 0 || to === i) return;
        if (to === 0) return spec.acts.useNow?.(account);
        const next = line.filter((a) => a !== account);
        next.splice(to, 0, account);
        spec.acts.temp?.({ set: next.slice(1, Math.max(i, to) + 1) });
      };
      document.addEventListener("pointermove", move);
      document.addEventListener("pointerup", up);
    });
    wrap.appendChild(r);
  });
  return wrap;
}

// ─────────────────────────────── 格

function label(cell: RotCell): HTMLElement {
  const k = el("span", (e) => (e.className = s.rotCellKey), cell.label);
  if (cell.from === "all") k.dataset.inherited = "true";
  if (cell.under !== undefined) k.dataset.under = cell.under;
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

/** 数格（百分数 · 分钟）：前面那半句（声明给的）· 数 · 可空的再带「或 ［空值说法］」；空着时数画虚线。 */
function numberCell(cell: RotCell, unit: string, set: (next: unknown) => void): HTMLElement[] {
  const ctl = el("span", (e) => (e.className = s.rotCtl));
  const num = cell.num ?? 0;
  const isNull = cell.nullable !== undefined && cell.v === null;
  if (cell.lead !== undefined) ctl.appendChild(el("span", (e) => (e.className = s.rotLead), cell.lead));
  ctl.appendChild(numberBox(num, cell, unit, isNull || cell.from === "all", (n) => set(n)));
  if (cell.nullable !== undefined) {
    const nul = button({
      label: cell.nullable,
      kind: "secondary",
      size: "compact",
      onClick: () => set(isNull ? num : null),
    });
    nul.classList.add(s.rotNull);
    nul.setAttribute("aria-pressed", String(isNull));
    ctl.append(el("span", (e) => (e.className = s.rotNumUnit), copyText("rot.cell.or")), nul);
  }
  return [label(cell), ctl];
}

/** 一格 ⇒ `[标签, 控件]`（排进格子）；`row` ＝ 号列表第几行（总体那几格 ⇒ `null`，说明写在控件旁边）。 */
function cellRows(spec: BoardSpec, cell: RotCell, row: number | null): HTMLElement[] {
  const inRow = row === null ? {} : { row };
  const set = (v: unknown): void => spec.acts.edit({ set: { key: cell.key, v, ...inRow } });
  switch (cell.kind) {
    case "toggle": {
      const sw = toggleSwitch({ label: cell.label, on: cell.v === true, onChange: (on) => set(on) });
      sw.root.classList.add(s.rotSwitch);
      if (row !== null || cell.help === undefined) return [label(cell), sw.root];
      const ctl = el("span", (e) => (e.className = s.rotCtl));
      ctl.append(sw.root, el("span", (e) => (e.className = s.rotNumUnit), cell.help));
      return [el("span", (e) => (e.className = s.rotCellKey), cell.label), ctl];
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
    case "daySegments":
      return [label(cell), segments(spec, cell, inRow)];
  }
}

// ─────────────────────────────── 分段

const DAY = 1440;

/** `HH:MM` ⇒ 一天里的分钟（24:00 ⇒ 1440）；写得不对 ⇒ `null`。 */
function minutesOf(text: string): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!m) return null;
  const v = Number(m[1]) * 60 + Number(m[2]);
  return Number(m[2]) < 60 && v <= DAY ? v : null;
}

/** 分段一窗：后面那几窗先是［同第一窗 · 另设］；自己设的 ⇒ 上面一条只画的条 ＋ 一段一行（从 · 到 · 上限 · ✕）＋「＋ 加一段」。 */
function segments(spec: BoardSpec, cell: RotCell, inRow: { row?: number }): HTMLElement {
  const box = el("div", (e) => (e.className = s.rotSegs));
  const win = cell.window ?? "";
  const edit = (at: number | undefined, seg: { from: number; to: number; v: number } | undefined): void =>
    spec.acts.edit({ seg: { key: cell.key, window: win, ...inRow, ...(at === undefined ? {} : { at }), ...(seg ? { seg } : {}) } });
  if (cell.same !== undefined) {
    const same = cell.from === "same";
    box.appendChild(
      segmented({
        label: cell.label,
        items: [
          { key: "same", label: cell.same },
          { key: "own", label: copyText("rot.cell.own") },
        ],
        current: same ? "same" : "own",
        onChange: (k) => spec.acts.edit({ same: { key: cell.key, window: win, ...inRow, same: k === "same" } }),
      }),
    );
    if (same) return box;
  }
  const segs = cell.segs ?? [];
  box.appendChild(dayBar(segs, cell.from === "all"));
  /** 一行：给了 `g` ⇒ 第 `at` 段（改哪格都交整段）；没给 ⇒ 新加的那一行，三格都填好才交。 */
  const line = (g: RotDaySeg | null, at: number | undefined): HTMLElement => {
    const r = el("div", (e) => (e.className = s.rotSegRow));
    r.dataset.rotSeg = at === undefined ? "new" : String(at);
    const time = (min: number | null, name: string): HTMLInputElement => {
      const t = document.createElement("input");
      t.type = "text";
      t.inputMode = "numeric";
      t.className = s.rotSegTime;
      t.value = min === null ? "" : dayClock(min);
      t.placeholder = copyText("rot.seg.placeholder");
      t.setAttribute("aria-label", name);
      return t;
    };
    const from = time(g ? Number(g.from) : null, copyText("rot.seg.from"));
    const to = time(g ? Number(g.to) : null, copyText("rot.seg.to"));
    const v = document.createElement("input");
    v.type = "number";
    v.className = s.rotNumInput;
    v.min = String(cell.min ?? 0);
    v.max = String(cell.max ?? 100);
    v.value = g ? String(g.v) : "";
    v.setAttribute("aria-label", copyText("rot.seg.v"));
    const commit = (): void => {
      const a = minutesOf(from.value);
      const b = minutesOf(to.value);
      const n = v.value.trim() === "" ? NaN : Number(v.value);
      if (a === null || b === null || !Number.isFinite(n)) return;
      edit(at, { from: a, to: b, v: n });
    };
    for (const x of [from, to, v]) x.addEventListener("change", commit);
    const num = el("label", (e) => (e.className = s.rotNum));
    num.append(v, el("span", (e) => (e.className = s.rotNumUnit), copyText("rot.unit.percent")));
    r.append(from, el("span", (e) => (e.className = s.rotNumUnit), copyText("rot.seg.dash")), to, num);
    if (at !== undefined)
      r.appendChild(
        button({ label: copyText("rot.seg.del"), kind: "icon", size: "compact", icon: "close", hint: copyText("rot.seg.del"), onClick: () => edit(at, undefined) }),
      );
    return r;
  };
  segs.forEach((g, i) => box.appendChild(line(g, i)));
  const add = button({ label: copyText("rot.seg.add"), kind: "ghost", size: "compact", icon: "plus" });
  add.addEventListener("click", () => {
    const r = line(null, undefined);
    add.before(r);
    r.querySelector("input")?.focus();
  });
  box.appendChild(add);
  return box;
}

/** 分段那一条：只画（段与字照回包）；0% 的段斜纹。 */
function dayBar(segs: readonly RotDaySeg[], inherited: boolean): HTMLElement {
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
  return bar;
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
  if (spec.where === "session") {
    root.appendChild(header(copyText("rot.h.order"), copyText("rot.h.orderHint")));
    root.appendChild(order(spec, view));
    if (view.timeline) {
      root.appendChild(simHeader(spec, view));
      root.appendChild(simulation(spec, view));
    }
    return root;
  }
  root.appendChild(
    header(copyText("rot.h.cards"), copyText("rot.h.cardsHint")),
  );
  // 编辑小窗浮在号牌下面（不挤开面板）：号牌与它同在一格里，小窗照那一格摆。
  const deck = el("div", (e) => (e.className = s.rotDeck));
  deck.appendChild(cards(spec, view));
  const pane = spec.state.open === null ? null : accountPane(spec, view, spec.state.open);
  if (pane) deck.appendChild(pane);
  root.appendChild(deck);
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
