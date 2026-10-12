/**
 * 会话面板里的「轮换」：打开的配置 ＝ 一份副本；列表 ＝ 有效优先级（第一行在用）；号详情 · 参数… 两页。
 *
 * - 全照后端：行序 · 为什么 · 跳过 · 卡住条 · 策略出错 · 有新版本 · 描述都是 `rotation-session-read` 写好的；合图照 `rotation-plan`；
 *   表单照 `rotation-strategies-read` 的声明生成（`rot-form.ts`）。界面不跑策略、不判到线、不排先后。
 * - 改任何一格 ⇒ `rotation-session-edit`（写进副本，头上出 `已改`）；`保存` · `另存为…` · `还原` · 换配置（有没存的改动先问）
 *   ⇒ `rotation-session-save / -revert / -open`。拖一行 · 换到这个号 · `刷新策略` ⇒ `rotation-session-set`（手动调整，不算改配置、
 *   换完不出任何提示）。
 * 判据：`tests/frontend/ui/acct-panel.vitest.ts`。
 */
import { accountLabel } from "./acct-words";
import { acctAvatar } from "./acct-dom";
import { reasonLabel } from "./acct-view";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { detailOf, writtenSaid } from "./kit/detail";
import { button } from "./kit/button";
import { askText, choiceDialog, confirmDialog } from "./kit/dialog";
import { icon } from "./kit/icon";
import { openMenu, type MenuItem } from "./kit/menu";
import { select } from "./kit/select";
import { toast } from "./kit/toast";
import { combinedChart, type ChartRow, type ChartView } from "./rot-chart";
import { bulkBar, cellErrorsFor, paramForm } from "./rot-form";
import {
  editSession,
  openConfig,
  placeSession,
  readPlan,
  revertSession,
  saveSession,
  type CellSet,
  type ConfigsShown,
  type EditAsk,
} from "./rot-reads";
import type { CellError } from "./generated/CellError";
import type { ConfigDiff } from "./generated/ConfigDiff";
import type { DeclShown } from "./generated/DeclShown";
import type { PlanReply } from "./generated/PlanReply";
import type { QuotaShow } from "./generated/QuotaShow";
import type { RotationRow } from "./generated/RotationRow";
import type { SessionRotationState } from "./generated/SessionRotationState";
import type { StrategiesRead } from "./generated/StrategiesRead";
import type { StrategyItem } from "./generated/StrategyItem";
import s from "./rot-session.module.css";

export type Present = Extract<SessionRotationState, { state: "present" }>;

/** 面板此刻在哪一页。 */
export type RotView = { k: "list" } | { k: "detail"; account: string } | { k: "params" };

/** 跟着面板走的那几样（面板关了就没了）。 */
export interface RotState {
  view: RotView;
  /** 多选（Ctrl / Shift 点选）· 单击选中的那一个（Shift 的起点）。 */
  sel: string[];
  anchor: string | null;
  /** 多选那一条「已选 N 个号」填的格（空 ＝ 不改）。 */
  bulk: Record<string, unknown>;
  /** 最近一次改副本收不下的格（后端回的）。 */
  errors: CellError[];
  /** 最近一次写被那台整个拒了（例：轮换文件还是旧格式、常驻后端换形之前不写）：那一句 ＋ 复制详情；写成一次就撤。 */
  fail: { said: string; detail: string } | null;
  chart: { view: ChartView; reply: PlanReply | null; key: string; seq: number };
}

export function newRotState(): RotState {
  return { view: { k: "list" }, sel: [], anchor: null, bulk: {}, errors: [], fail: null, chart: { view: "12h", reply: null, key: "", seq: 0 } };
}

/** 面板交进来的几样。 */
export interface RotCtx {
  sid: string;
  origin: Origin;
  read: Present;
  configs: ConfigsShown | null;
  strategies: StrategiesRead | null | undefined;
  /** 这个号在额度账里的显示态（行头的用量 · 号详情的计量条）。 */
  quotaOf(account: string): QuotaShow | null;
  /** 号详情「此刻」那两条计量条（面板那一处的件）。 */
  meters(q: QuotaShow): HTMLElement[];
  /** 这台账号库里这一家的号（`＋ 加号…` 的候选）。 */
  library(): string[];
  st: RotState;
  /** 重画面板（数据没变、只换页 / 换选中）。 */
  render(): void;
  /** 写了之后：重问这个会话再画。 */
  reload(): Promise<void>;
  /** 拖 / 填的时候数据到了先不画。 */
  hold(on: boolean): void;
  sessionTitle: string;
  openSwitch(anchor: HTMLElement): void;
  openRotation(at?: { config: string } | { strategy: string }): void;
}

/** 建一个元素：`cls` 里给它挂类（写成 `(e) => (e.className = s.x)`，叠没叠的量具认得出挂到哪）。 */
function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: (e: HTMLElementTagNameMap[K]) => void, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  cls(e);
  if (text !== undefined) e.textContent = text;
  return e;
}

function strategyOf(c: RotCtx): StrategyItem | null {
  return c.strategies?.strategies.find((x) => x.id === c.read.strategy.id) ?? null;
}

/** 一个号在副本里是哪一行：自己点名的那一行 · 起会话的号那一行 · 其余的订阅号那一行。 */
export function whoOf(read: Present, account: string): { who: string; params: Record<string, unknown>; index: number; fromRest: boolean } {
  const rows = read.values.accounts;
  let i = rows.findIndex((w) => w.who === account);
  if (i < 0 && account === read.start) i = rows.findIndex((w) => w.who === "@start");
  const fromRest = i < 0;
  if (i < 0) i = rows.findIndex((w) => w.who === "@rest");
  const w = rows[i];
  return { who: w?.who ?? account, params: w?.params ?? {}, index: i, fromRest };
}

/** 改副本那一格时写哪一行：起会话的号写 `@start`；其余的订阅号底下的号写它自己（后端给它单写一行）。 */
function editTarget(read: Present, account: string): string {
  const w = whoOf(read, account);
  return w.who === "@start" ? "@start" : account;
}

/** 副本跟原样差的那几格里落在这一行的：改过的键 · 原值。 */
function diffOf(read: Present, who: string | undefined): { changed: Set<string>; was: Record<string, unknown> } {
  const changed = new Set<string>();
  const was: Record<string, unknown> = {};
  for (const d of read.config.diff ?? []) {
    if (d.acct !== who) continue;
    changed.add(d.k);
    was[d.k] = d.from;
  }
  return { changed, was };
}

// ─────────────────────────────── 写

/** 一次写没成：那台写好了那一句（拒写的原因）⇒ 进面板那一条、不吞；说不出 ⇒ 一条 toast（`fallback`）。 */
function writeFailed(c: RotCtx, what: string, e: unknown, fallback: string): void {
  console.warn(`[rot] ${what} 失败：`, e);
  const said = writtenSaid(e);
  if (said !== null) c.st.fail = { said, detail: detailOf(e) };
  else toast(fallback, "", { level: "error" });
}

async function edit(c: RotCtx, ask: EditAsk): Promise<void> {
  try {
    const got = await editSession(c.origin, c.sid, ask);
    c.st.errors = got.errors;
    c.st.fail = null;
  } catch (e) {
    writeFailed(c, "rotation-session-edit", e, copyText("rot.doc.editFailed"));
  }
  // 那一条（拒写的那一句）先画出来 / 撤掉，再等重问回来。
  c.render();
  await c.reload();
}

function cfgName(c: RotCtx): string {
  return c.read.config.name;
}

/** `保存`：写回打开的那一份（带打开时的版本）；撞上别处存的新版 ⇒ 问 覆盖 · 另存为… · 取消。 */
export async function saveCopy(c: RotCtx, force = false): Promise<boolean> {
  try {
    const got = await saveSession(c.origin, c.sid, { ifRev: c.read.config.rev, ...(force ? { force: true } : {}) });
    c.st.fail = null;
    if (got.state === "saved") {
      toast(copyText("rot.doc.saved", { cfg: cfgName(c) }), "", { level: "success" });
      await c.reload();
      return true;
    }
    if (got.state === "refused") {
      toast(copyText("rot.doc.refused", { why: got.errors.map((e) => e.said).join(copyText("kit.text.sep")) }), "", { level: "error" });
      return false;
    }
    const at = c.read.config.newer?.text ?? "";
    const pick = await choiceDialog({
      title: copyText("rot.doc.clashTitle", { cfg: cfgName(c), at }),
      action: { key: "overwrite", label: copyText("rot.doc.overwrite") },
      others: [{ key: "saveAs", label: copyText("rot.doc.saveAs") }],
      blocks: [
        { lead: copyText("rot.doc.clashTheirs"), lines: got.theirs.map((d) => d.text) },
        { lead: copyText("rot.doc.clashMine"), lines: got.mine.map((d) => d.text) },
      ],
    });
    if (pick === "overwrite") return saveCopy(c, true);
    if (pick === "saveAs") return saveCopyAs(c);
    return false;
  } catch (e) {
    writeFailed(c, "rotation-session-save", e, copyText("rot.doc.failed", { cfg: cfgName(c) }));
    c.render();
    return false;
  }
}

/** `另存为…`：存成新的一份（名字可不起 ⇒ `配置 N`），本会话改开它。 */
export async function saveCopyAs(c: RotCtx): Promise<boolean> {
  const name = await askText({ title: copyText("rot.doc.saveAsTitle"), action: copyText("rot.doc.saveAsGo"), label: copyText("rot.doc.saveAsName") });
  if (name === null) return false;
  try {
    const t = name.trim();
    const got = await saveSession(c.origin, c.sid, { ifRev: c.read.config.rev, as: t === "" ? {} : { name: t } });
    c.st.fail = null;
    if (got.state === "saved") {
      await c.reload();
      return true;
    }
    if (got.state === "refused") toast(copyText("rot.doc.refused", { why: got.errors.map((e) => e.said).join(copyText("kit.text.sep")) }), "", { level: "error" });
    return false;
  } catch (e) {
    writeFailed(c, "rotation-session-save", e, copyText("rot.doc.failed", { cfg: cfgName(c) }));
    c.render();
    return false;
  }
}

async function revert(c: RotCtx, toLatest = false): Promise<void> {
  try {
    await revertSession(c.origin, c.sid, toLatest);
    c.st.fail = null;
  } catch (e) {
    writeFailed(c, "rotation-session-revert", e, copyText("rot.doc.editFailed"));
  }
  c.render();
  await c.reload();
}

/** 换一份配置打开：有没存的改动 ⇒ 先问 保存 · 丢掉 · 取消。 */
export async function switchConfig(c: RotCtx, id: string): Promise<void> {
  const to = c.configs?.configs.find((x) => x.id === id);
  try {
    const got = await openConfig(c.origin, c.sid, id);
    if (got.state === "dirty") {
      // 下拉已经换了字：问的时候先画回打开着的那一份（选了「保存 / 丢掉」才真换）。
      c.render();
      const pick = await choiceDialog({
        title: copyText("rot.doc.askTitle", { name: to?.name ?? id }),
        action: { key: "keep", label: copyText("rot.doc.keep") },
        others: [{ key: "discard", label: copyText("rot.doc.discard") }],
        blocks: [{ lead: copyText("rot.doc.askLead", { cfg: cfgName(c) }), lines: got.diff.map((d: ConfigDiff) => d.text) }],
      });
      if (pick === null) {
        c.render();
        return;
      }
      if (pick === "keep" && !(await saveCopy(c))) return;
      await openConfig(c.origin, c.sid, id, true);
    }
    c.st.fail = null;
  } catch (e) {
    writeFailed(c, "rotation-session-open", e, copyText("rot.doc.editFailed"));
  }
  c.st.view = { k: "list" };
  c.render();
  await c.reload();
}

/** 手动调整：把一个号放到第 `at` 位（0 ＝ 现在就换过去，不出提示）· 刷新策略。没换成 ⇒ 一句为什么。 */
async function place(c: RotCtx, ask: { place: { account: string; at: number } } | { refresh: true }): Promise<void> {
  try {
    const got = await placeSession(c.origin, c.sid, ask);
    c.st.fail = null;
    if (got.state !== "done" && "place" in ask) {
      toast(copyText("rot.place.failed", { cur: accountLabel(c.read.current), reason: reasonLabel(got.why ?? "", { agent: c.read.agent, target: ask.place.account }) }), "", { level: "error" });
    }
  } catch (e) {
    writeFailed(c, "rotation-session-set", e, copyText("rot.doc.editFailed"));
  }
  c.render();
  await c.reload();
}

// ─────────────────────────────── 头 · 条

/** `轮换 [配置 ▾] 已改 … 保存 另存为… 还原`。 */
function cfgLine(c: RotCtx, withPicker: boolean): HTMLElement {
  const line = el("div", (e) => (e.className = s.rsCfgLine));
  if (withPicker) {
    line.appendChild(el("span", (e) => (e.className = s.rsTitle), copyText("rot.panel.title")));
    const cs = c.configs?.configs ?? [];
    const sel = select({
      label: copyText("rot.cfg.label"),
      options: (cs.length > 0 ? cs : [{ id: c.read.config.id, name: c.read.config.name, text: "", inUse: [] as string[], isDefault: false }]).map((x) => ({
        value: x.id,
        label: x.name,
        detail: x.inUse.length > 0 ? copyText("rot.cfg.inUse", { n: x.inUse.length }) : undefined,
        note: x.isDefault ? copyText("rot.cfg.tagDefault") : undefined,
        shownNote: "",
        peek: x.text === "" ? undefined : () => el("div", (e) => (e.className = s.rsPeek), x.text),
      })),
      value: c.read.config.id,
      closedKeys: "open",
      menuWidth: 340,
      decorate: (items) => [
        ...items,
        { label: "", divider: true },
        { id: "saveAs", label: copyText("rot.doc.saveAs"), onClick: () => void saveCopyAs(c) },
        { id: "manage", label: copyText("rot.cfg.manage"), onClick: () => c.openRotation({ config: c.read.config.id }) },
      ],
      onChange: (id) => void switchConfig(c, id),
    });
    sel.el.dataset.rotCfg = "true";
    const pickBox = el("span", (e) => (e.className = s.rsCfgPick));
    pickBox.appendChild(sel.el);
    line.appendChild(pickBox);
  } else {
    line.appendChild(el("span", (e) => (e.className = s.rsCfgName), c.read.config.name));
  }
  if (c.read.config.dirty) {
    const tag = el("span", (e) => (e.className = s.rsDirty), copyText("rot.doc.dirty"));
    tag.dataset.rotDirty = "true";
    line.appendChild(tag);
  }
  line.appendChild(el("span", (e) => (e.className = s.rsSpacer)));
  if (c.read.config.dirty) {
    const save = button({ label: copyText("rot.doc.save"), kind: "primary", size: "compact", onClick: () => void saveCopy(c) });
    save.dataset.rotSave = "true";
    const as = button({ label: copyText("rot.doc.saveAs"), kind: "secondary", size: "compact", onClick: () => void saveCopyAs(c) });
    as.dataset.rotSaveAs = "true";
    const back = button({ label: copyText("rot.doc.revert"), kind: "ghost", size: "compact", onClick: () => void revert(c) });
    back.dataset.rotRevert = "true";
    const acts = el("span", (e) => (e.className = s.rsActs));
    acts.append(save, as, back);
    // 窄窗：三个动作收进 ⋯（`rot-session.module.css` 按面板宽切）。
    const more = el("button", (e) => (e.className = s.rsMore));
    more.type = "button";
    more.setAttribute("aria-label", copyText("rot.doc.more"));
    more.appendChild(icon("more", "compact"));
    more.addEventListener("click", () =>
      openMenu({ el: more, align: "end" }, [
        { label: copyText("rot.doc.save"), onClick: () => void saveCopy(c) },
        { label: copyText("rot.doc.saveAs"), onClick: () => void saveCopyAs(c) },
        { label: copyText("rot.doc.revert"), onClick: () => void revert(c) },
      ]),
    );
    line.append(acts, more);
  }
  return line;
}

/** 信息条：读不懂（需手动）· 卡住 · 策略出错 · 有新版本（手动热换不出条）。 */
function bars(c: RotCtx): HTMLElement[] {
  const r = c.read;
  const out: HTMLElement[] = [];
  // 轮换文件读不懂 · 策略出错：需手动 ＋ 后端写好的那一句（＋ 复制详情）。
  if (r.needs) {
    const b = banner("warn", copyText("rot.bar.needs", { said: r.needs.reason }), [], r.needs.detail);
    b.dataset.rotNeeds = r.needs.kind;
    out.push(b);
  }
  if (c.st.fail) {
    const b = banner("warn", copyText("rot.bar.needs", { said: c.st.fail.said }), [], c.st.fail.detail);
    b.dataset.rotWriteFail = "true";
    out.push(b);
  }
  if (r.blocked) {
    const b = banner("warn", r.blocked.text);
    b.dataset.rotBlocked = "true";
    out.push(b);
  }
  if (r.strategyError) {
    const b = banner("error", copyText("rot.bar.needs", { said: r.strategyError.text }), [], r.strategyError.msg);
    b.dataset.rotStrategyError = "true";
    out.push(b);
  }
  if (r.config.newer) {
    const load = button({
      label: copyText("rot.doc.loadNewer"),
      kind: "ghost",
      size: "compact",
      onClick: () =>
        void (async () => {
          const go = await confirmDialog({ title: copyText("rot.doc.loadTitle", { cfg: cfgName(c) }), action: copyText("rot.doc.loadGo"), body: copyText("rot.doc.loadAsk") });
          if (go) await revert(c, true);
        })(),
    });
    load.dataset.rotLoadNewer = "true";
    const b = banner("info", r.config.newer.text, [load]);
    b.dataset.rotNewer = "true";
    out.push(b);
  }
  return out;
}

// ─────────────────────────────── 列表 ＋ 合图

/** 号名那一格（读屏 · 判据认它：`data-rot-name`）。 */
function nameEl(account: string): HTMLElement {
  const n = el("span", (e) => (e.className = s.rsName), accountLabel(account));
  n.dataset.rotName = account;
  return n;
}

/** 行头：把手 · 头像 · 号名 · 用法标（不是缺省的才标）；下一行 在用 · 用量。 */
function rowHead(c: RotCtx, row: RotationRow, i: number, modeDecl: DeclShown | null): HTMLElement {
  const box = el("div", (e) => (e.className = s.rsHead));
  const l1 = el("div", (e) => (e.className = s.rsL1));
  const handle = el("span", (e) => (e.className = s.rsHandle));
  handle.dataset.rotHandle = row.account;
  handle.setAttribute("aria-label", copyText("rot.row.dragAria", { name: accountLabel(row.account) }));
  handle.appendChild(icon("drag", "compact"));
  l1.append(handle, acctAvatar(row.account, 16), nameEl(row.account));
  const mode = modeDecl ? whoOf(c.read, row.account).params[modeDecl.k] : undefined;
  if (modeDecl && mode !== undefined && mode !== modeDecl.init) {
    const opt = modeDecl.options?.find((o) => o.v === mode);
    if (opt) {
      const tag = el("span", (e) => (e.className = s.rsTag), opt.label);
      tag.dataset.mode = String(mode);
      l1.appendChild(tag);
    }
  }
  const l2 = el("div", (e) => (e.className = s.rsL2));
  if (i === 0 && row.account === c.read.current) l2.appendChild(el("span", (e) => (e.className = s.rsCur), copyText("acct.tag.cur")));
  const q = c.quotaOf(row.account);
  if (q) {
    const slot = q.usage.slot ? q.slots.find((x) => x.slot === q.usage.slot) : undefined;
    if (slot?.pct !== undefined) {
      const bar = el("span", (e) => (e.className = s.rsMini));
      const fill = el("span", (e) => (e.className = s.rsMiniFill));
      fill.style.transform = `scaleX(${Math.max(0, Math.min(1, slot.pct / 100))})`;
      bar.appendChild(fill);
      l2.appendChild(bar);
    }
    const t = el("span", (e) => (e.className = s.rsUsage), q.usage.text);
    if (q.usage.tone !== "plain") t.dataset.tone = q.usage.tone;
    l2.appendChild(t);
  }
  box.append(l1, l2);
  return box;
}

/** 那份策略声明的「用法」那一格（约定名 `mode`，`choice`）：右键菜单 · 行上的标照它。 */
function modeDeclOf(c: RotCtx): DeclShown | null {
  return strategyOf(c)?.acctParams.find((d) => d.k === "mode" && d.type === "choice") ?? null;
}

function rowMenu(c: RotCtx, account: string, ev: MouseEvent): void {
  ev.preventDefault();
  const items: MenuItem[] = [];
  items.push({ label: copyText("rot.menu.switchTo"), enabled: account !== c.read.current, onClick: () => void place(c, { place: { account, at: 0 } }) });
  const md = modeDeclOf(c);
  if (md) {
    const cur = whoOf(c.read, account).params[md.k];
    items.push({
      label: md.label,
      submenu: (md.options ?? []).map((o) => ({
        label: o.label,
        detail: o.hint,
        checked: cur === o.v,
        onClick: () => void edit(c, { set: [{ acct: editTarget(c.read, account), k: md.k, v: o.v }] }),
      })),
    });
  }
  items.push(
    { label: copyText("rot.row.leave"), enabled: account !== c.read.current, onClick: () => void edit(c, { drop: [account] }) },
    { label: copyText("rot.menu.detail"), onClick: () => openDetail(c, account) },
  );
  openMenu({ x: ev.clientX, y: ev.clientY }, items);
}

function openDetail(c: RotCtx, account: string): void {
  c.st.view = { k: "detail", account };
  c.render();
}

/** 点选：单击选一个 · Ctrl 加减 · Shift 连选（照此刻的行序）。 */
function pick(c: RotCtx, account: string, ev: MouseEvent | KeyboardEvent, order: string[]): void {
  const st = c.st;
  if (ev.shiftKey && st.anchor !== null) {
    const a = order.indexOf(st.anchor);
    const b = order.indexOf(account);
    st.sel = order.slice(Math.min(a, b), Math.max(a, b) + 1);
  } else if (ev.ctrlKey || ev.metaKey) {
    st.sel = st.sel.includes(account) ? st.sel.filter((x) => x !== account) : [...st.sel, account];
    st.anchor = account;
  } else {
    st.sel = [account];
    st.anchor = account;
  }
  if (st.sel.length < 2) st.bulk = {};
  c.render();
}

/** 只从把手拖：拖起的行淡下去、落点那一行顶边一道线；松手 ⇒ 放到那一位（到第一行 ＝ 现在就换过去）；拖出列表 · Esc ＝ 不动。 */
function startDrag(c: RotCtx, ev: PointerEvent, rows: HTMLElement[], from: number, account: string, list: HTMLElement): void {
  ev.preventDefault();
  const row = rows[from];
  c.hold(true);
  row.dataset.lifted = "true";
  let to = from;
  const clear = (): void => {
    for (const r of rows) delete r.dataset.drop;
  };
  const move = (e: PointerEvent): void => {
    const box = list.getBoundingClientRect();
    clear();
    if (e.clientY < box.top - 8 || e.clientY > box.bottom + 8) {
      to = from;
      return;
    }
    let k = rows.findIndex((r) => e.clientY < r.getBoundingClientRect().top + r.getBoundingClientRect().height / 2);
    if (k < 0) k = rows.length;
    to = k > from ? k - 1 : k;
    if (k < rows.length) rows[k].dataset.drop = "before";
    if (to === 0 && from !== 0) row.title = copyText("rot.row.dragTo", { name: accountLabel(account) });
  };
  const end = (commit: boolean): void => {
    document.removeEventListener("pointermove", move);
    document.removeEventListener("pointerup", up);
    document.removeEventListener("keydown", esc, true);
    clear();
    delete row.dataset.lifted;
    row.removeAttribute("title");
    c.hold(false);
    if (commit && to !== from) void place(c, { place: { account, at: to } });
  };
  const up = (): void => end(true);
  const esc = (e: KeyboardEvent): void => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    end(false);
  };
  document.addEventListener("pointermove", move);
  document.addEventListener("pointerup", up);
  document.addEventListener("keydown", esc, true);
}

/** 合图的推演：按「这一刻的会话轮换 ＋ 视窗」记一把钥匙，钥匙变了才重问。 */
function askPlan(c: RotCtx): void {
  const st = c.st.chart;
  const key = `${st.view}|${JSON.stringify(c.read)}`;
  if (st.key === key) return;
  st.key = key;
  const seq = ++st.seq;
  readPlan(c.origin, { sid: c.sid, view: st.view }).then(
    (p) => {
      if (st.seq !== seq) return;
      st.reply = p;
      c.render();
    },
    (e: unknown) => {
      if (st.seq !== seq) return;
      console.warn("[rot] rotation-plan 失败：", e);
    },
  );
}

/** ★ 列表那一页：头一行 · 描述 · 信息条 · 列表头一排 · 合图（＋ 多选那一条）。 */
export function rotationList(c: RotCtx): HTMLElement {
  const r = c.read;
  const root = el("section", (e) => (e.className = s.rsBlock));
  root.dataset.rotList = "true";
  root.append(cfgLine(c, true), el("div", (e) => (e.className = s.rsDesc), r.text), ...bars(c));
  const tbar = el("div", (e) => (e.className = s.rsTbar));
  tbar.appendChild(el("span", (e) => (e.className = s.rsSpacer)));
  const par = button({ label: copyText("rot.tbar.params"), kind: "ghost", size: "compact", onClick: () => ((c.st.view = { k: "params" }), c.render()) });
  par.dataset.rotParams = "true";
  const refresh = button({ label: copyText("rot.manual.refresh"), kind: "ghost", size: "compact", onClick: () => void place(c, { refresh: true }) });
  refresh.dataset.rotRefresh = "true";
  const sw = button({ label: copyText("rot.main.switch"), kind: "ghost", size: "compact", onClick: () => c.openSwitch(sw) });
  sw.dataset.rotSwitch = "true";
  tbar.append(par, refresh, sw);
  root.appendChild(tbar);

  askPlan(c);
  const modeDecl = modeDeclOf(c);
  const order = r.rows.map((x) => x.account);
  c.st.sel = c.st.sel.filter((a) => order.includes(a));
  const rows: ChartRow[] = r.rows.map((row, i) => ({
    account: row.account,
    head: rowHead(c, row, i, modeDecl),
    why: row.why,
    skip: row.skip === true,
    inuse: i === 0 && row.account === r.current,
    alsoUsed: row.alsoUsed,
    selected: c.st.sel.includes(row.account),
  }));
  const added = new Set(order);
  const candidates = c.library().filter((a) => !added.has(a));
  const chart = combinedChart({
    plan: c.st.chart.reply,
    view: c.st.chart.view,
    onView: (v) => {
      c.st.chart.view = v;
      c.render();
    },
    session: { title: copyText("rot.chart.session"), sub: copyText("rot.chart.sessionSub", { session: c.sessionTitle, acct: accountLabel(r.start) }) },
    rows,
    onOpen: (a) => openDetail(c, a),
    onAdd: () => {
      const items: MenuItem[] =
        candidates.length === 0
          ? [{ label: copyText("rot.add.none"), enabled: false }]
          : candidates.map((a) => ({ label: accountLabel(a), avatar: acctAvatar(a), onClick: () => void edit(c, { add: [a] }) }));
      const anchor = root.querySelector<HTMLElement>("[data-rc-add]");
      if (anchor) openMenu({ el: anchor, align: "start" }, items);
    },
  });
  const rowEls = r.rows.map((x) => chart.rowEls.get(x.account)!);
  const list = rowEls[0]?.parentElement ?? chart.root;
  r.rows.forEach((row, i) => {
    const e = rowEls[i];
    e.tabIndex = 0;
    e.addEventListener("click", (ev) => {
      if ((ev.target as HTMLElement).closest("button")) return;
      pick(c, row.account, ev, order);
    });
    e.addEventListener("dblclick", () => openDetail(c, row.account));
    e.addEventListener("contextmenu", (ev) => rowMenu(c, row.account, ev));
    e.addEventListener("keydown", (ev) => {
      if (ev.isComposing) return;
      if (ev.key === "Enter") {
        ev.preventDefault();
        openDetail(c, row.account);
      } else if (ev.altKey && (ev.key === "ArrowUp" || ev.key === "ArrowDown")) {
        ev.preventDefault();
        const to = ev.key === "ArrowUp" ? i - 1 : i + 1;
        if (to >= 0 && to < order.length) void place(c, { place: { account: row.account, at: to } });
      } else if (ev.key === "ArrowUp" || ev.key === "ArrowDown") {
        ev.preventDefault();
        rowEls[ev.key === "ArrowUp" ? Math.max(0, i - 1) : Math.min(rowEls.length - 1, i + 1)]?.focus();
      }
    });
    e.querySelector<HTMLElement>("[data-rot-handle]")?.addEventListener("pointerdown", (ev) => startDrag(c, ev, rowEls, i, row.account, list));
  });
  root.appendChild(chart.root);
  if (c.st.sel.length >= 2) {
    const decls = strategyOf(c)?.acctParams ?? [];
    const bar = bulkBar(
      decls,
      c.st.sel.length,
      c.st.bulk,
      (k, v) => {
        if (v === undefined) delete c.st.bulk[k];
        else c.st.bulk[k] = v;
      },
      () => {
        const set: CellSet[] = [];
        for (const a of c.st.sel) for (const [k, v] of Object.entries(c.st.bulk)) set.push({ acct: editTarget(r, a), k, v });
        c.st.bulk = {};
        if (set.length > 0) void edit(c, { set });
      },
    );
    chart.root.insertBefore(bar, chart.root.lastChild);
  }
  return root;
}

// ─────────────────────────────── 号详情

function backLine(c: RotCtx, title: HTMLElement[], nav: HTMLElement[] = []): HTMLElement {
  const line = el("div", (e) => (e.className = s.rsBackLine));
  const back = el("button", (e) => (e.className = s.rsBack));
  back.type = "button";
  back.append(icon("caretLeft", "compact"), copyText("rot.det.back"));
  back.dataset.rotBack = "true";
  back.addEventListener("click", () => {
    c.st.view = { k: "list" };
    c.render();
  });
  line.append(back, ...title, el("span", (e) => (e.className = s.rsSpacer)), ...nav);
  return line;
}

/** ★ 号详情（图 07 A · B）：头 · 配置名 ＋ 已改 ＋ 保存三件 ·（从其余的订阅号来的）一句 · 此刻 · 每号参数（照声明生成）。 */
export function detailView(c: RotCtx, account: string): HTMLElement {
  const r = c.read;
  const root = el("section", (e) => (e.className = s.rsBlock));
  root.dataset.rotDetail = account;
  const order = r.rows.map((x) => x.account);
  const i = order.indexOf(account);
  const row = r.rows[i];
  const md = modeDeclOf(c);
  const w = whoOf(r, account);
  const titleBits: HTMLElement[] = [acctAvatar(account, 18), el("span", (e) => (e.className = s.rsDetName), accountLabel(account))];
  if (md) {
    const v = w.params[md.k];
    const opt = md.options?.find((o) => o.v === v);
    if (opt && v !== md.init) titleBits.push(el("span", (e) => (e.className = s.rsTag), opt.label));
  }
  const nav = (dir: -1 | 1, aria: string, glyph: "caretLeft" | "caretRight"): HTMLElement => {
    const b = el("button", (e) => (e.className = s.rsNav));
    b.appendChild(icon(glyph, "compact"));
    b.type = "button";
    b.setAttribute("aria-label", aria);
    b.dataset.rotNav = String(dir);
    const to = order[i + dir];
    if (to === undefined) b.disabled = true;
    else
      b.addEventListener("click", () => {
        c.st.view = { k: "detail", account: to };
        c.render();
      });
    return b;
  };
  root.appendChild(backLine(c, titleBits, [nav(-1, copyText("rot.det.prev"), "caretLeft"), nav(1, copyText("rot.det.next"), "caretRight")]));
  root.appendChild(cfgLine(c, false));
  if (w.fromRest) root.appendChild(el("div", (e) => (e.className = s.rsNote), copyText("rot.det.fromRest")));
  // 此刻：排第几与为什么（引擎写的有效位置）· 换到它 · 计量条。
  const now = el("div", (e) => (e.className = s.rsNow));
  const head = el("div", (e) => (e.className = s.rsNowHead));
  head.appendChild(el("span", (e) => (e.className = s.rsSub), copyText("rot.det.now")));
  if (row) {
    const why = row.skip ? copyText("rot.row.skip", { why: row.why }) : row.why;
    const rank = el("span", (e) => (e.className = s.rsRank), copyText("rot.det.rank", { n: i + 1, why }));
    rank.dataset.rotRank = String(i + 1);
    head.appendChild(rank);
  }
  head.appendChild(el("span", (e) => (e.className = s.rsSpacer)));
  const go = button({ label: copyText("rot.det.switchTo"), kind: "secondary", size: "compact", onClick: () => void place(c, { place: { account, at: 0 } }) });
  go.dataset.rotSwitchTo = "true";
  if (account === r.current) go.disabled = true;
  head.appendChild(go);
  now.appendChild(head);
  const q = c.quotaOf(account);
  if (q) {
    if (q.kind === "api") now.appendChild(el("div", (e) => (e.className = s.rsApi), q.usage.text));
    else now.append(...c.meters(q));
  }
  root.appendChild(now);
  // 每号参数：照这份策略的声明生成；改过的格一道琥珀线、line 旁写原值。
  const st = strategyOf(c);
  root.appendChild(el("h4", (e) => (e.className = s.rsSub), copyText("rot.det.acctParams", { strategy: r.strategy.name })));
  if (st) {
    const d = diffOf(r, w.fromRest ? "@rest" : w.who);
    root.appendChild(
      paramForm({
        decls: st.acctParams,
        values: w.params,
        name: `rot-det-${c.sid}-${account}`,
        onSet: (k, v) => void edit(c, { set: [{ acct: editTarget(r, account), k, v }] }),
        changed: d.changed,
        was: d.was,
        errors: cellErrorsFor(c.st.errors, `accounts[${w.index}]`),
        metered: (q?.kind ?? row?.facts.kind) === "api",
      }),
    );
  }
  return root;
}

// ─────────────────────────────── 参数…

/** ★ `参数…`（图 07 C）：策略下拉（换策略：旧参数收起、同名照搬）· 整份参数 · 新加的号缺省（不含时段）。 */
export function paramsView(c: RotCtx): HTMLElement {
  const r = c.read;
  const root = el("section", (e) => (e.className = s.rsBlock));
  root.dataset.rotParamsView = "true";
  root.appendChild(backLine(c, [el("span", (e) => (e.className = s.rsDetName), copyText("rot.par.title"))]));
  root.appendChild(cfgLine(c, false));
  root.appendChild(el("h4", (e) => (e.className = s.rsSub), copyText("rot.par.strategy")));
  const line = el("div", (e) => (e.className = s.rsStratLine));
  const list = (c.strategies?.strategies ?? []).filter((x) => x.error === undefined);
  const sel = select({
    label: copyText("rot.par.strategy"),
    options: (list.length > 0 ? list : [{ id: r.strategy.id, name: r.strategy.name, text: "" }]).map((x) => ({ value: x.id, label: x.name, detail: x.text })),
    value: r.strategy.id,
    menuWidth: 320,
    onChange: (id) => void edit(c, { strategy: id }),
  });
  sel.el.dataset.rotStrategy = "true";
  const src = button({ label: copyText("rot.par.source"), kind: "ghost", size: "compact", onClick: () => c.openRotation({ strategy: r.strategy.id }) });
  line.append(sel.el, src);
  root.appendChild(line);
  const st = strategyOf(c);
  if (st?.text) root.appendChild(el("div", (e) => (e.className = s.rsDesc), st.text));
  if (!st) return root;
  root.appendChild(el("h4", (e) => (e.className = s.rsSub), copyText("rot.par.whole", { strategy: r.strategy.name })));
  const whole = diffOf(r, undefined);
  root.appendChild(
    paramForm({
      decls: st.params,
      values: r.values.params,
      name: `rot-par-${c.sid}`,
      onSet: (k, v) => void edit(c, { set: [{ k, v }] }),
      changed: whole.changed,
      was: whole.was,
      errors: cellErrorsFor(c.st.errors, "params"),
    }),
  );
  const ih = el("div", (e) => (e.className = s.rsSubLine));
  ih.append(el("h4", (e) => (e.className = s.rsSub), copyText("rot.par.init")), el("span", (e) => (e.className = s.rsNote), copyText("rot.par.initHint")));
  root.appendChild(ih);
  const init = diffOf(r, "@init");
  root.appendChild(
    paramForm({
      decls: st.acctParams,
      values: r.values.acctInit,
      name: `rot-init-${c.sid}`,
      onSet: (k, v) => void edit(c, { set: [{ acct: "@init", k, v }] }),
      changed: init.changed,
      was: init.was,
      errors: cellErrorsFor(c.st.errors, "acctInit"),
      skip: ["slots"],
      compact: true,
    }),
  );
  return root;
}
