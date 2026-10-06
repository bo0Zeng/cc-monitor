/**
 * 「账号」面板（右侧抽屉）：状态栏账号按钮 · 右键「账号…」· 会话头 ⋯ 三处开的同一个。四块 ＋ 底栏：
 * 当前 · 轮换（`默认 | 本会话`、触发 `满 / ≥N%`、无号可换 `继续跑 / 停`、勾与拖序、折着的时间轴）· 切换（选号、热切换 / 重启切换）· 记录 ·
 * 底栏 `新会话默认 [work ▾] … 管理账号…`。
 *
 * - 数据只经 `quota-read` · `rotation-read` · `rotation-session-read/-set` · `rotation-switch`（`quota-reads.ts`）与推送
 *   （`acct-center.ts` 收进 `appStore`）。界面不判「能不能换 / 该不该换」：热切换成不成立看 `inPlace`、下一个看 `next`、
 *   满没满看显示态，都是后端给的。新勾的按量号挪到末尾也是后端做的。
 * - 改即生效，不要保存；存失败列表退回原样、块顶一条 `未保存 · 轮换未变 [重试]`。
 * - 实时：数据变了原地重画；正在拖、正在填 `≥N%` 时不重画（松手 / 失焦再画）。
 */
import { ACTIVE_AGENT } from "./agent-profile";
import { appStore, type SessionRotationEntry } from "./app-store";
import { refreshQuota, refreshSessions } from "./acct-center";
import { acctAvatar } from "./acct-dom";
import { ledgerOf, localTzMin, machineLabel, reasonLabel, swappedFrom, usageOf, usageText, whyOf } from "./acct-view";
import { copyText } from "./copy-table";
import { askSessionInterrupts } from "./interrupt-reads";
import { commands } from "./ipc/commands";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button, setDisabled } from "./kit/button";
import { openDrawer, type DrawerHandle } from "./kit/drawer";
import { fold } from "./kit/fold";
import { icon } from "./kit/icon";
import { confirmInterrupts } from "./kit/interrupts";
import { closeMenu, openMenu, type MenuItem } from "./kit/menu";
import { meter, type MeterState } from "./kit/meter";
import { segmented } from "./kit/tabs";
import { toast } from "./kit/toast";
import { attachTooltip } from "./kit/tooltip";
import { ARRIVAL_BUDGET_MS } from "./launch-arrival";
import { accountLabel, fmtAt, fmtRel, slotLabel, type QuotaRead } from "./quota-lines";
import { switchHot, switchRestart, writeSessionRotation } from "./quota-reads";
import { runRemoteAttach } from "./remote-launch-run";
import { standingOf } from "./sessions-where";
import { startSettings } from "./tab-batch-run";
import type { AtLimit } from "./generated/AtLimit";
import type { QuotaShow } from "./generated/QuotaShow";
import type { Rotation } from "./generated/Rotation";
import type { RotationSlot } from "./generated/RotationSlot";
import type { SessionRotationState } from "./generated/SessionRotationState";
import s from "./acct.module.css";

/** 主窗口交进来的几样（都是回调，面板不认 tab 栏）。 */
export interface AcctPanelHost {
  /** 会话的标题（面板头 `orders · 本机`）。 */
  sessionTitle(sid: string): string;
  /** 会话的项目目录（重启切换要交那台）。 */
  cwdOf(sid: string): string;
  /** 设置窗那台的账号页（`管理账号…` · `设置…` · `接入…`）。 */
  openSettings(origin: Origin): void;
  /** 底栏「新会话默认」那一颗：开那台的默认账号下拉（与无会话时状态栏按钮的同一个）。 */
  openDefaultMenu(anchor: HTMLElement, origin: Origin): void;
  /** 新会话默认此刻是哪个号（底栏那颗按钮上的字）。 */
  defaultOf(origin: Origin): string | null;
  /** 开这条会话的恢复菜单（与右键「恢复 ▸」同一份）。 */
  openResume(sid: string): void;
}

type Present = Extract<SessionRotationState, { state: "present" }>;

interface Open {
  sid: string;
  origin: Origin;
  drawer: DrawerHandle;
  body: HTMLElement;
  /** 切换那一块选的号（`null` ＝ 按后端给的下一个）。 */
  pick: string | null;
  mode: "hot" | "restart";
  /** 切换那一块的错误条（没成的那一句）。 */
  switchError: string | null;
  /** 轮换那一块的存失败（重试那一下要写的那一份）。 */
  saveFailed: Rotation | null;
  timelineOpen: boolean;
  /** 本会话用自己的轮换时，在轮换块下面摊开一块只读的「默认轮换」（设置窗账号页那一行「默认轮换」点进来）。 */
  showDefault: boolean;
  /** 正在拖 / 正在填：数据到了先不重画。 */
  hold: boolean;
  pending: boolean;
  unsub: (() => void)[];
}

let open: Open | null = null;

/** 此刻开着的是不是这个会话的面板。 */
export function panelOpenFor(sid: string): boolean {
  return open?.sid === sid;
}

/** 开 / 关这个会话的面板（再点同一个入口 ＝ 关）。 */
export function toggleAccountPanel(sid: string, origin: Origin, host: AcctPanelHost): void {
  if (open?.sid === sid) {
    void open.drawer.close();
    return;
  }
  openAccountPanel(sid, origin, host);
}

/** 当前 tab 换了：面板开着就跟着换成那个会话（不关）。 */
export function followActive(a: { sid: string; origin: Origin } | null, host: AcctPanelHost): void {
  if (!open || a === null || open.sid === a.sid) return;
  openAccountPanel(a.sid, a.origin, host);
}

export function openAccountPanel(sid: string, origin: Origin, host: AcctPanelHost): void {
  if (open) {
    const was = open;
    open = null;
    for (const u of was.unsub) u();
    void was.drawer.close();
  }
  const body = document.createElement("div");
  body.className = s.acctPanel;
  const statusBar = document.getElementById("status-bar");
  const drawer = openDrawer({
    title: copyText("acct.panel.title"),
    sub: copyText("acct.panel.sub", { session: host.sessionTitle(sid), machine: machineLabel(origin) }),
    body,
    width: 440,
    bottom: statusBar ? Math.round(statusBar.getBoundingClientRect().height) : undefined,
    onClose: () => {
      if (open?.drawer !== drawer) return;
      for (const u of open.unsub) u();
      open = null;
    },
  });
  const o: Open = {
    sid,
    origin,
    drawer,
    body,
    pick: null,
    mode: "hot",
    switchError: null,
    saveFailed: null,
    timelineOpen: false,
    showDefault: false,
    hold: false,
    pending: false,
    unsub: [],
  };
  open = o;
  const repaint = (): void => {
    if (open !== o) return;
    if (o.hold) {
      o.pending = true;
      return;
    }
    render(o, host);
  };
  o.unsub.push(appStore.quota.subscribe(repaint), appStore.sessionRotation.subscribe(repaint), appStore.rotationDefault.subscribe(repaint), appStore.accounts.subscribe(repaint));
  render(o, host);
  // 焦点落在「轮换」分段（规范：打开即可键盘改轮换）；没有那一块（中转没见过）⇒ 留在关闭钮上。
  body.querySelector<HTMLElement>('[role="radiogroup"] [aria-checked="true"]')?.focus();
  // 打开那一刻再问一次这个会话（推送丢过也能补上）；额度账同理。
  void refreshSessions(origin, [sid]);
  void refreshQuota(origin);
}

/** 面板里能直接滚到的两节：时间轴 · 默认轮换。 */
export type AcctPanelAnchor = "timeline" | "default-rotation";

/**
 * 打开这个会话的面板并滚到那一节（已开着就不关、原地滚）。只改面板自己的显示（时间轴展开 · 摊开默认轮换），
 * 不写任何东西：本会话跟不跟随默认照旧。
 */
export function openAccountPanelAt(sid: string, origin: Origin, host: AcctPanelHost, anchor: AcctPanelAnchor): void {
  if (open?.sid !== sid) openAccountPanel(sid, origin, host);
  const o = open;
  if (!o) return;
  if (anchor === "timeline") o.timelineOpen = true;
  else o.showDefault = true;
  render(o, host);
  o.body.querySelector<HTMLElement>(`[data-acct-anchor="${anchor}"]`)?.scrollIntoView({ block: "start" });
}

/** 拖完 / 填完：放开重画。 */
function release(o: Open, host: AcctPanelHost): void {
  o.hold = false;
  if (o.pending) {
    o.pending = false;
    render(o, host);
  }
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function section(title: string, right?: HTMLElement): { root: HTMLElement; content: HTMLElement } {
  const root = el("section", s.acctBlock);
  const head = el("div", s.acctBlockHead);
  head.appendChild(el("h3", s.acctBlockTitle, title));
  if (right) head.appendChild(right);
  const content = el("div", s.acctBlockBody);
  root.append(head, content);
  return { root, content };
}

function render(o: Open, host: AcctPanelHost): void {
  const entry = appStore.sessionRotation.get().get(o.sid);
  const quota = appStore.quota.get().get(o.origin) ?? null;
  const scroll = o.body.parentElement?.scrollTop ?? 0;
  o.body.replaceChildren();
  const read = entry?.read;
  o.body.append(nowBlock(o, entry, quota, host));
  if (read?.state === "present" && entry) {
    o.body.append(rotationBlock(o, entry, read, quota, host), switchBlock(o, entry, read, quota, host), historyBlock(entry, read));
  } else {
    o.body.append(switchBlock(o, entry, undefined, quota, host));
  }
  o.body.append(footer(o, host));
  if (o.body.parentElement) o.body.parentElement.scrollTop = scroll;
}

// ─────────────────────────────── 当前

function slotMeter(q: QuotaShow, slot: string, now: number, tz: number): HTMLElement {
  const x = q.slots.find((v) => v.slot === slot);
  const here = q.limiting === slot;
  let state: MeterState = "normal";
  if (!x || x.pct === undefined) state = "none";
  else if (q.stale) state = "stale";
  else if (here && q.state === "refused") state = "refused";
  else if (here && (q.state === "near" || q.state === "overageInUse")) state = "near";
  let value: string;
  if (!x || x.pct === undefined || (here && (q.state === "resetSinceSeen" || q.state === "unseen"))) value = copyText("acct.val.none");
  else if (here && q.state === "refused") value = copyText("acct.val.refused");
  else if (here && q.state === "overageInUse") value = copyText("acct.val.over");
  else value = copyText("acct.val.pct", { pct: x.pct });
  const at = x?.resetsAt;
  const rel = at === undefined ? null : fmtRel(at, now);
  const reset = at === undefined ? undefined : rel === null ? copyText("acct.reset.past", { at: fmtAt(at, now, tz) }) : copyText("acct.reset.at", { at: fmtAt(at, now, tz) });
  const row = el("div", s.acctMeterRow);
  row.appendChild(meter({ label: slotLabel(slot), ratio: (x?.pct ?? 0) / 100, state, value, reset }));
  row.appendChild(el("span", s.acctMeterRel, rel ?? ""));
  return row;
}

function tagEl(text: string, tone?: "cur" | "warn" | "error"): HTMLElement {
  const t = el("span", s.acctTag, text);
  if (tone) t.dataset.shade = tone;
  return t;
}

function kindTags(q: Pick<QuotaShow, "kind" | "login" | "state">): HTMLElement[] {
  const tags = [tagEl(q.kind === "api" ? copyText("acct.kind.api") : copyText("acct.kind.sub"))];
  if (q.login === "needsLogin") tags.push(tagEl(copyText("acct.tag.login"), "error"));
  if (q.login === "needsKey") tags.push(tagEl(copyText("acct.tag.key"), "error"));
  if (q.state === "overageInUse") tags.push(tagEl(copyText("acct.val.over"), "warn"));
  return tags;
}

function nowBlock(o: Open, entry: SessionRotationEntry | undefined, quota: QuotaRead | null, host: AcctPanelHost): HTMLElement {
  const read = entry?.read;
  if (!entry || !read || read.state === "absent") {
    const sec = section(copyText("acct.now.title"));
    const connect = button({ label: copyText("acct.now.connect"), kind: "ghost", size: "compact", onClick: () => host.openSettings(o.origin) });
    sec.content.appendChild(banner("warn", copyText("acct.now.noRelay"), [connect]));
    return sec.root;
  }
  const now = entry.now;
  const tz = localTzMin(now);
  const led = ledgerOf(quota, read.agent, read.account.current);
  const seen = led ? fmtAt(led.seenAt, now, tz) : null;
  const right =
    seen === null
      ? undefined
      : el("span", s.acctBlockNote, isLocalOrigin(o.origin) ? copyText("acct.now.seen", { at: seen }) : copyText("acct.now.seenOther", { machine: o.origin, at: seen }));
  const sec = section(copyText("acct.now.title"), right);
  if (quota === null) {
    const retry = button({ label: copyText("acct.now.retry"), kind: "ghost", size: "compact", onClick: () => void refreshQuota(o.origin) });
    sec.content.appendChild(banner("error", copyText("acct.now.readFail"), [retry]));
  }
  const who = el("div", s.acctWho);
  who.appendChild(acctAvatar(read.account.current, 20));
  who.appendChild(el("span", s.acctWhoName, accountLabel(read.account.current)));
  who.append(...kindTags(read.quota));
  const from = swappedFrom(read);
  if (from) who.appendChild(el("span", s.acctWhoFrom, copyText("acct.hover.from", { at: fmtAt(from.at, now, tz), name: accountLabel(from.from) })));
  sec.content.appendChild(who);
  if (read.quota.kind === "api") {
    const u = usageOf(read.quota, led?.reading?.resetsAt, now, tz);
    const line = el("div", s.acctApiLine, read.quota.state === "refused" ? u.value : copyText("acct.val.noLimit"));
    if (read.quota.state === "refused") line.dataset.shade = "refused";
    if (u.reset) line.appendChild(el("span", s.acctMeterRel, u.reset));
    sec.content.appendChild(line);
  } else {
    sec.content.append(slotMeter(read.quota, "5h", now, tz), slotMeter(read.quota, "7d", now, tz));
  }
  return sec.root;
}

// ─────────────────────────────── 轮换

const START: RotationSlot = { start: true };

/** 这一份轮换画成哪几行：顺序里的（占位换成起始号）＋ 这台账号库里其余的号（不勾，排后面）。 */
function rowsOf(r: Rotation, start: string, quota: QuotaRead | null, agent: string): { account: string; start: boolean; on: boolean }[] {
  const out: { account: string; start: boolean; on: boolean }[] = [];
  const seen = new Set<string>();
  for (const slot of r.order) {
    const a = typeof slot === "string" ? slot : start;
    if (seen.has(a)) continue;
    seen.add(a);
    out.push({ account: a, start: typeof slot !== "string", on: typeof slot !== "string" || r.enabled.includes(a) });
  }
  const others = [...(quota?.accounts ?? []), ...(quota?.unseen ?? [])].filter((x) => x.agent === agent && x.account !== "_").map((x) => x.account);
  for (const a of others) {
    if (seen.has(a)) continue;
    seen.add(a);
    out.push({ account: a, start: false, on: false });
  }
  return out;
}

/** 行的勾 / 序改了 ⇒ 一份新的轮换（占位留在原位；序里没有的号勾上时排到末尾）。 */
function withRows(r: Rotation, rows: { account: string; start: boolean; on: boolean }[]): Rotation {
  const order: RotationSlot[] = [];
  const enabled: string[] = [];
  for (const row of rows) {
    if (row.start) {
      order.push(START);
      continue;
    }
    if (!row.on && !r.order.includes(row.account)) continue;
    order.push(row.account);
    if (row.on) enabled.push(row.account);
  }
  if (!order.some((x) => typeof x !== "string")) order.unshift(START);
  return { ...r, order, enabled };
}

function quotaOf(quota: QuotaRead | null, agent: string, account: string): QuotaShow | null {
  const led = ledgerOf(quota, agent, account);
  if (led) return led;
  const u = quota?.unseen.find((x) => x.agent === agent && x.account === account);
  return u ? { kind: u.kind, state: "unseen", stale: false, slots: [], login: u.login } : null;
}

/** 一行右侧的用量：`5h ▮ 63% ↻18:30 · 7d ▮ 41%`（按量号 `无 5h / 7d`；被拒 `✕ ↻19:00`）。 */
function rowUsage(q: QuotaShow | null, now: number, tz: number, readingReset: number | undefined): HTMLElement {
  const box = el("span", s.acctRowUsage);
  if (!q) return box;
  if (q.kind === "api") {
    const u = usageOf(q, readingReset, now, tz);
    box.appendChild(el("span", s.acctRowSlotVal, q.state === "refused" ? u.value : copyText("acct.val.noLimit")));
    if (u.reset) box.appendChild(el("span", s.acctRowSlotReset, u.reset));
    if (q.state === "refused") box.dataset.shade = "refused";
    return box;
  }
  for (const slot of ["5h", "7d"]) {
    const x = q.slots.find((v) => v.slot === slot);
    const here = q.limiting === slot;
    const cell = el("span", s.acctRowSlot);
    cell.appendChild(el("span", s.acctRowSlotKey, slotLabel(slot)));
    const bar = el("span", s.acctMiniBar);
    const fill = el("span", s.acctMiniFill);
    fill.style.transform = `scaleX(${Math.max(0, Math.min(1, (x?.pct ?? 0) / 100))})`;
    bar.appendChild(fill);
    cell.appendChild(bar);
    let v: string;
    if (!x || x.pct === undefined || (here && (q.state === "resetSinceSeen" || q.state === "unseen"))) v = copyText("acct.val.none");
    else if (here && q.state === "refused") v = copyText("acct.val.refused");
    else if (here && q.state === "overageInUse") v = copyText("acct.val.over");
    else v = copyText("acct.val.pct", { pct: x.pct });
    cell.appendChild(el("span", s.acctRowSlotVal, v));
    if (here && x?.resetsAt !== undefined && slot === "5h") {
      const rel = fmtRel(x.resetsAt, now);
      cell.appendChild(el("span", s.acctRowSlotReset, rel === null ? copyText("acct.reset.past", { at: fmtAt(x.resetsAt, now, tz) }) : copyText("acct.reset.at", { at: fmtAt(x.resetsAt, now, tz) })));
    }
    if (here && q.state === "refused") cell.dataset.shade = "refused";
    else if (here && (q.state === "near" || q.state === "overageInUse")) cell.dataset.shade = "warn";
    if (q.stale) cell.dataset.stale = "true";
    box.appendChild(cell);
  }
  return box;
}

function rotationBlock(o: Open, entry: SessionRotationEntry, read: Present, quota: QuotaRead | null, host: AcctPanelHost): HTMLElement {
  const def = appStore.rotationDefault.get().get(o.origin) ?? null;
  const follow = read.follow;
  const r: Rotation | null = follow ? (def?.rotation ?? null) : (read.custom ?? def?.rotation ?? null);
  const sec = section(copyText("acct.rot.title"));
  const now = entry.now;
  const tz = localTzMin(now);

  const bar = el("div", s.acctRotBar);
  const seg = segmented<"follow" | "custom">({
    items: [
      { key: "follow", label: copyText("acct.rot.follow") },
      { key: "custom", label: copyText("acct.rot.custom") },
    ],
    current: follow ? "follow" : "custom",
    label: copyText("acct.rot.modeAria"),
    onChange: (k) => void save(o, host, k === "follow" ? "follow" : "custom"),
  });
  const info = el("span", s.acctInfo);
  info.tabIndex = 0;
  info.setAttribute("aria-label", copyText("acct.rot.infoAria"));
  info.appendChild(icon("info", "compact"));
  attachTooltip(info, copyText("acct.rot.rule"));
  bar.append(seg, info, el("span", s.acctSpacer));
  if (r) bar.appendChild(triggerControls(o, host, r, follow, read.atLimit));
  sec.content.appendChild(bar);

  if (o.saveFailed) {
    const retry = button({ label: copyText("acct.now.retry"), kind: "ghost", size: "compact", onClick: () => void save(o, host, { custom: o.saveFailed as Rotation }) });
    sec.content.appendChild(banner("error", copyText("acct.rot.saveFail"), [retry]));
  }
  if (follow) {
    const lead = el("div", s.acctLead, def?.state === "absent" || !def ? copyText("acct.rot.leadDefaultEmpty") : copyText("acct.rot.leadFollow"));
    lead.appendChild(button({ label: copyText("acct.rot.settings"), kind: "ghost", size: "compact", onClick: () => host.openSettings(o.origin) }));
    sec.content.appendChild(lead);
  }
  if (r) sec.content.appendChild(rotationList(o, host, read, r, quota, now, tz, !follow));
  if (o.showDefault && !follow && def?.state === "present") sec.content.appendChild(defaultPreview(o, host, read, def.rotation, quota, now, tz));
  const tl = timelineFold(o, read, r, quota, now, tz);
  tl.dataset.acctAnchor = "timeline";
  sec.content.appendChild(tl);
  // 本会话跟随默认 ⇒ 这一块画的就是默认那份，「默认轮换」滚到这里。
  if (follow) sec.root.dataset.acctAnchor = "default-rotation";
  return sec.root;
}

/** 本会话用自己的轮换时摊开的「默认轮换」：顺序 · 触发 · 封顶，全只读。 */
function defaultPreview(o: Open, host: AcctPanelHost, read: Present, r: Rotation, quota: QuotaRead | null, now: number, tz: number): HTMLElement {
  const box = el("div", s.acctDefPreview);
  box.dataset.acctAnchor = "default-rotation";
  box.appendChild(el("div", s.acctBlockTitle, copyText("acct.prev.title")));
  box.appendChild(el("div", s.acctBlockNote, copyText("acct.prev.lead")));
  // 「实际照什么办」那一句只说本会话；这一块是默认那份自己写的。
  box.appendChild(triggerControls(o, host, r, true, r.atLimit, "default"));
  const list = rotationList(o, host, read, r, quota, now, tz, false);
  for (const row of list.querySelectorAll<HTMLElement>("[data-acct-row]")) {
    const caps = capText(r, row.dataset.acctRow ?? "");
    // 封顶贴在名字那一串后面、用量之前。
    if (caps) row.insertBefore(tagEl(caps), row.lastElementChild);
  }
  box.appendChild(list);
  return box;
}

/** 一个号的封顶（每个窗口一段，按时段的写出时段）；没有 ⇒ `null`。 */
function capText(r: Rotation, account: string): string | null {
  const per = r.cap?.[account];
  if (!per) return null;
  const parts: string[] = [];
  for (const [win, v] of Object.entries(per)) {
    const pre = win === "*" ? "" : `${slotLabel(win)} `;
    if (typeof v === "number") parts.push(pre + copyText("acct.prev.cap", { n: v }));
    else for (const c of v) parts.push(pre + copyText("acct.prev.capAt", { at: c.at, n: c.n }));
  }
  return parts.length > 0 ? parts.join(copyText("kit.text.sep")) : null;
}

/** `触发 (•)满 ( )≥[90]%` ＋ `无号可换 [继续跑 | 停]`（后者只在 `≥N%` 时有效）。 */
function triggerControls(o: Open, host: AcctPanelHost, r: Rotation, readonly: boolean, actual: AtLimit, group = "own"): HTMLElement {
  const box = el("span", s.acctTrigger);
  box.appendChild(el("span", s.acctTriggerLabel, copyText("acct.rot.trigger")));
  // 同一份面板里可以有两组（本会话的 · 只读的默认轮换）：名字分开，免得勾一组顶掉另一组。
  const name = `acct-trigger-${o.sid}-${group}`;
  const pctMode = r.when !== "full";
  const n = r.when === "full" ? 90 : r.when.threshold.n;
  const radio = (on: boolean, label: string, pick: () => void): HTMLLabelElement => {
    const l = el("label", s.acctRadio);
    const i = document.createElement("input");
    i.type = "radio";
    i.name = name;
    i.checked = on;
    i.disabled = readonly;
    i.addEventListener("change", pick);
    l.append(i, document.createTextNode(label));
    return l;
  };
  const num = document.createElement("input");
  num.type = "text";
  num.inputMode = "numeric";
  num.className = s.acctPct;
  num.value = String(n);
  num.disabled = readonly;
  const commit = (): void => {
    const v = Number(num.value.trim());
    if (!Number.isInteger(v) || v < 50 || v > 99) {
      num.dataset.error = "true";
      num.title = copyText("acct.rot.pctRange");
      return;
    }
    delete num.dataset.error;
    if (pctMode && v === n) return;
    void save(o, host, { custom: { ...r, when: { threshold: { n: v } } } });
  };
  num.addEventListener("focus", () => {
    o.hold = true;
    num.select();
  });
  num.addEventListener("blur", () => {
    commit();
    release(o, host);
  });
  num.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !ev.isComposing) num.blur();
  });
  box.append(
    radio(!pctMode, copyText("acct.rot.trigFull"), () => void save(o, host, { custom: { ...r, when: "full" } })),
    radio(pctMode, copyText("acct.rot.trigPct"), () => void save(o, host, { custom: { ...r, when: { threshold: { n } } } })),
    num,
    el("span", s.acctTriggerUnit, copyText("acct.rot.pctUnit")),
  );
  // 无号可换时怎么办（稿里没画的那一格）：两态分段；「满」触发时没有 N%，灰着。
  const lim = el("span", s.acctLimit);
  const label = el("span", s.acctTriggerLabel, copyText("acct.lim.label"));
  attachTooltip(label, () => (pctMode ? copyText("acct.lim.labelHint", { n }) : copyText("acct.lim.fullOnly")));
  const seg = segmented<AtLimit>({
    items: [
      { key: "continue", label: copyText("acct.lim.go") },
      { key: "stop", label: copyText("acct.lim.stop") },
    ],
    current: r.atLimit,
    label: copyText("acct.lim.aria"),
    onChange: (k) => void save(o, host, { custom: { ...r, atLimit: k } }),
  });
  const [go, stop] = [...seg.querySelectorAll<HTMLButtonElement>("button")];
  if (go) attachTooltip(go, copyText("acct.lim.goHint"));
  if (stop) attachTooltip(stop, () => copyText("acct.lim.stopHint", { n }));
  if (readonly || !pctMode) {
    seg.dataset.disabled = "true";
    for (const b of [go, stop]) if (b) b.disabled = true;
  }
  lim.append(label, seg);
  if (r.atLimit === "stop" && actual === "continue" && pctMode) lim.appendChild(el("span", s.acctBlockNote, copyText("acct.lim.downgraded", { agent: ACTIVE_AGENT })));
  const wrap = el("span", s.acctTriggerWrap);
  wrap.append(box, lim);
  return wrap;
}

function rotationList(o: Open, host: AcctPanelHost, read: Present, r: Rotation, quota: QuotaRead | null, now: number, tz: number, editable: boolean): HTMLElement {
  const rows = rowsOf(r, read.account.start, quota, read.agent);
  const list = el("div", s.acctList);
  list.setAttribute("role", "list");
  const cur = read.account.current;
  const elems: HTMLElement[] = [];
  rows.forEach((row, i) => {
    const q = quotaOf(quota, read.agent, row.account);
    const line = el("div", s.acctRow);
    line.setAttribute("role", "listitem");
    line.dataset.acctRow = row.account;
    line.tabIndex = editable ? 0 : -1;
    if (row.account === cur) line.dataset.inuse = "true";
    if (q && (q.state === "refused" || q.login !== "ok")) line.dataset.dim = "true";
    const handle = el("span", s.acctHandle);
    handle.appendChild(icon("drag", "compact"));
    handle.setAttribute("aria-label", copyText("acct.row.dragAria", { name: accountLabel(row.account) }));
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = row.on;
    box.setAttribute("aria-label", copyText("acct.row.checkAria", { name: accountLabel(row.account) }));
    const locked = row.start || row.account === cur;
    box.disabled = !editable || locked || q?.login === "needsLogin" || q?.login === "needsKey";
    if (row.account === cur && editable) attachTooltip(line, copyText("acct.rot.lockHint"));
    box.addEventListener("change", () => {
      const next = rows.map((x, k) => (k === i ? { ...x, on: box.checked } : x));
      void save(o, host, { custom: withRows(r, next) });
    });
    if (editable) line.appendChild(handle);
    line.append(box, acctAvatar(row.account), el("span", s.acctRowName, accountLabel(row.account)));
    if (row.start) line.appendChild(tagEl(copyText("acct.tag.start")));
    if (row.account === cur) line.appendChild(tagEl(copyText("acct.tag.cur"), "cur"));
    if (q) {
      if (q.kind === "api") line.appendChild(tagEl(copyText("acct.kind.api")));
      if (q.login === "needsLogin") line.appendChild(tagEl(copyText("acct.tag.login"), "error"));
      if (q.login === "needsKey") line.appendChild(tagEl(copyText("acct.tag.key"), "error"));
      if (q.state === "overageInUse") line.appendChild(tagEl(copyText("acct.val.over"), "warn"));
    }
    line.appendChild(rowUsage(q, now, tz, ledgerOf(quota, read.agent, row.account)?.reading?.resetsAt));
    if (editable) {
      // 键盘：Alt+↑ / Alt+↓ 移位、空格勾（复选框自己管）。
      line.addEventListener("keydown", (ev) => {
        if (ev.isComposing || !ev.altKey || (ev.key !== "ArrowUp" && ev.key !== "ArrowDown")) return;
        ev.preventDefault();
        const to = ev.key === "ArrowUp" ? i - 1 : i + 1;
        if (to < 0 || to >= rows.length) return;
        void save(o, host, { custom: withRows(r, move(rows, i, to)) });
      });
      handle.addEventListener("pointerdown", (ev) => startDrag(ev, o, host, list, elems, i, (to) => void save(o, host, { custom: withRows(r, move(rows, i, to)) })));
    }
    elems.push(line);
    list.appendChild(line);
  });
  return list;
}

function move<T>(xs: T[], from: number, to: number): T[] {
  const out = [...xs];
  const [x] = out.splice(from, 1);
  out.splice(to, 0, x);
  return out;
}

/** 只从把手拖：拖起的行浮起、落点顶边一条插入线、原位留影子；松手即存；拖出列表 ＝ 不动；Esc 取消。 */
function startDrag(ev: PointerEvent, o: Open, host: AcctPanelHost, list: HTMLElement, rows: HTMLElement[], from: number, done: (to: number) => void): void {
  ev.preventDefault();
  const row = rows[from];
  o.hold = true;
  row.dataset.lifted = "true";
  let to = from;
  const line = el("div", s.acctDropLine);
  const place = (y: number): void => {
    const box = list.getBoundingClientRect();
    if (y < box.top - 8 || y > box.bottom + 8) {
      to = from;
      line.remove();
      return;
    }
    let k = rows.findIndex((r) => y < r.getBoundingClientRect().top + r.getBoundingClientRect().height / 2);
    if (k < 0) k = rows.length;
    to = k > from ? k - 1 : k;
    const anchor = rows[k] ?? null;
    list.insertBefore(line, anchor);
  };
  const moveFn = (e: PointerEvent): void => place(e.clientY);
  const end = (commit: boolean): void => {
    document.removeEventListener("pointermove", moveFn);
    document.removeEventListener("pointerup", up);
    document.removeEventListener("keydown", esc, true);
    line.remove();
    delete row.dataset.lifted;
    if (commit && to !== from) done(to);
    release(o, host);
  };
  const up = (): void => end(true);
  const esc = (e: KeyboardEvent): void => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    e.preventDefault();
    end(false);
  };
  document.addEventListener("pointermove", moveFn);
  document.addEventListener("pointerup", up);
  document.addEventListener("keydown", esc, true);
}

/** 写这个会话的轮换：成 ⇒ 重问这个会话；败 ⇒ 列表退回原样、块顶一条错误条（重试写同一份）。 */
async function save(o: Open, host: AcctPanelHost, w: "follow" | "custom" | { custom: Rotation }): Promise<void> {
  try {
    const got = await writeSessionRotation(o.origin, [o.sid], w);
    const one = got[o.sid];
    if (one && one.state !== "done") throw new Error(one.code);
    o.saveFailed = null;
  } catch (e) {
    console.warn("[acct] rotation-session-set 失败：", e);
    o.saveFailed = typeof w === "object" ? w.custom : null;
  }
  await refreshSessions(o.origin, [o.sid]);
  if (open === o) render(o, host);
}

// ─────────────────────────────── 时间轴（折在轮换下）

function timelineFold(o: Open, read: Present, r: Rotation | null, quota: QuotaRead | null, now: number, tz: number): HTMLElement {
  const pool = r ? rowsOf(r, read.account.start, quota, read.agent).filter((x) => x.on) : [{ account: read.account.current, start: true, on: true }];
  const usable = (quota?.usableNow ?? []).filter((a) => pool.some((p) => p.account === a));
  const back = quota?.earliestReturn ?? null;
  const parts: string[] = [];
  if (usable.length > 0) parts.push(copyText("acct.tl.usable", { list: usable.map(accountLabel).join(", ") }));
  if (back && pool.some((p) => p.account === back.account)) parts.push(copyText("acct.tl.nextBack", { name: accountLabel(back.account), at: fmtAt(back.at, now, tz) }));
  const summary = parts.length > 0 ? parts.join(copyText("kit.text.sep")) : copyText("acct.tl.allOk");
  const body = el("div", s.acctTimeline);
  const span = 24 * 3600;
  for (const p of pool) {
    const q = quotaOf(quota, read.agent, p.account);
    const line = el("div", s.acctTlRow);
    if (p.account === read.account.current) line.dataset.inuse = "true";
    line.append(acctAvatar(p.account), el("span", s.acctTlName, accountLabel(p.account)));
    const track = el("span", s.acctTlTrack);
    const marks: string[] = [];
    for (const x of q?.slots ?? []) {
      if (x.resetsAt === undefined) continue;
      const d = x.resetsAt - now;
      if (d <= 0 || d > span) continue;
      const m = el("span", x.slot === "7d" ? s.acctTlMark7 : s.acctTlMark5);
      m.style.left = `${(d / span) * 100}%`;
      if (q?.limiting === x.slot && q.state === "refused") {
        const seg = el("span", s.acctTlRefused);
        seg.style.width = `${(d / span) * 100}%`;
        track.appendChild(seg);
      }
      track.appendChild(m);
      marks.push(copyText("acct.tl.marks", { w: slotLabel(x.slot), at: fmtAt(x.resetsAt, now, tz) }));
    }
    line.append(track, el("span", s.acctTlLabel, marks.join(copyText("kit.text.sep"))));
    body.appendChild(line);
  }
  body.appendChild(el("div", s.acctBlockNote, copyText("acct.tl.legend")));
  const refusedHere = read.quota.state === "refused";
  return fold({
    title: copyText("acct.tl.title"),
    summary,
    open: o.timelineOpen || refusedHere,
    body,
    onToggle: (v) => {
      o.timelineOpen = v;
    },
  });
}

// ─────────────────────────────── 切换

function switchBlock(o: Open, entry: SessionRotationEntry | undefined, read: Present | undefined, quota: QuotaRead | null, host: AcctPanelHost): HTMLElement {
  const sec = section(copyText("acct.sw.title"));
  // 中转没见过这个会话 ⇒ 不知道它的路由名，取这台额度账里那一家（今天只有一家）。
  const agent = read?.agent ?? quota?.accounts[0]?.agent ?? quota?.unseen[0]?.agent ?? ACTIVE_AGENT;
  const cur = read?.account.current ?? null;
  const now = entry?.now ?? Math.floor(Date.now() / 1000);
  const tz = localTzMin(now);
  const inPlace = read ? read.account.inPlace : entry?.read.state === "absent" ? entry.read.inPlace : "noRelay";
  const candidates = [...(quota?.accounts ?? []), ...(quota?.unseen ?? [])].filter((x) => x.agent === agent && x.account !== "_").map((x) => x.account);
  // 缺省选后端给的下一个；没给（中转没见过）⇒ 此刻发得出去的头一个（`usableNow`，后端判的）。
  const pick = o.pick ?? read?.next ?? candidates.find((a) => a !== cur && (quota?.usableNow ?? []).includes(a)) ?? candidates.find((a) => a !== cur) ?? null;
  const pickBtn = el("button", s.acctPick);
  pickBtn.type = "button";
  pickBtn.setAttribute("aria-label", copyText("acct.sw.pickAria"));
  if (pick) {
    pickBtn.append(acctAvatar(pick), el("span", s.acctPickName, accountLabel(pick)));
    const q = quotaOf(quota, agent, pick);
    if (q) {
      const u = usageOf(q, ledgerOf(quota, agent, pick)?.reading?.resetsAt, now, tz);
      pickBtn.appendChild(el("span", s.acctPickUsage, usageText(u)));
    }
  } else {
    pickBtn.appendChild(el("span", s.acctPickName, copyText("acct.sw.optNone")));
  }
  pickBtn.appendChild(icon("caretDown", "compact"));
  pickBtn.addEventListener("click", () => {
    const items: MenuItem[] = candidates.map((a) => {
      const q = quotaOf(quota, agent, a);
      const u = q ? usageOf(q, ledgerOf(quota, agent, a)?.reading?.resetsAt, now, tz) : null;
      const usage = u ? (usageText(u)) : undefined;
      const blocked = a === cur || q?.login === "needsLogin" || q?.login === "needsKey";
      return {
        label: accountLabel(a),
        avatar: acctAvatar(a),
        checked: a === pick,
        detail: a === cur ? copyText("acct.tag.cur") : q?.login === "needsLogin" ? copyText("acct.tag.login") : q?.login === "needsKey" ? copyText("acct.tag.key") : usage,
        detailTone: u?.tone === "refused" || u?.tone === "near" ? "warn" : undefined,
        enabled: !blocked,
        onClick: () => {
          o.pick = a;
          o.switchError = null;
          render(o, host);
        },
      };
    });
    openMenu({ el: pickBtn, align: "start" }, items, { label: copyText("acct.sw.pickAria") });
  });
  const hotOk = inPlace === "ok";
  const mode = hotOk ? o.mode : "restart";
  const go = button({
    label: mode === "restart" ? copyText("acct.sw.restart") : copyText("acct.sw.go"),
    kind: "primary",
    onClick: () => void doSwitch(o, host, pick, cur, read, go),
  });
  const row = el("div", s.acctSwitchRow);
  row.append(pickBtn, go);
  sec.content.appendChild(row);

  const name = `acct-mode-${o.sid}`;
  const modes = el("div", s.acctModes);
  const radio = (m: "hot" | "restart", label: string, disabled: boolean): HTMLLabelElement => {
    const l = el("label", s.acctRadio);
    const i = document.createElement("input");
    i.type = "radio";
    i.name = name;
    i.checked = mode === m;
    i.disabled = disabled;
    i.addEventListener("change", () => {
      o.mode = m;
      render(o, host);
    });
    l.append(i, document.createTextNode(label));
    return l;
  };
  modes.append(radio("hot", copyText("acct.sw.hot"), !hotOk));
  if (!hotOk) modes.appendChild(el("span", s.acctReason, reasonLabel(inPlace, { agent, target: pick ?? "" })));
  modes.append(radio("restart", copyText("acct.sw.restart"), false));
  sec.content.appendChild(modes);
  if (read) sec.content.appendChild(el("div", s.acctHint, copyText("acct.sw.hotHint", { name: accountLabel(read.account.start) })));
  sec.content.appendChild(el("div", s.acctHint, copyText("acct.sw.restartHint")));
  if (!pick || pick === cur) setDisabled(go, copyText("acct.sw.optNone"));
  if (o.switchError) sec.content.appendChild(banner("error", o.switchError));
  return sec.root;
}

async function doSwitch(o: Open, host: AcctPanelHost, target: string | null, cur: string | null, read: Present | undefined, go: HTMLButtonElement): Promise<void> {
  if (!target) return;
  const agent = read?.agent ?? ACTIVE_AGENT;
  const hot = (read?.account.inPlace ?? "noRelay") === "ok" && o.mode === "hot";
  setDisabled(go, copyText("acct.sw.busy"));
  try {
    if (hot) {
      const got = (await switchHot(o.origin, [o.sid], target))[o.sid];
      if (!got || got.state !== "done") {
        o.switchError = copyText("acct.sw.failHot", { cur: accountLabel(cur ?? "_"), reason: reasonLabel(got ? got.code : "", { agent, target }) });
      } else {
        o.switchError = null;
        o.pick = null;
        toast(copyText("acct.sw.doneHot", { name: accountLabel(target) }), "", { level: "info" });
      }
      return;
    }
    await restartSwitch(o, host, target);
  } catch (e) {
    console.warn("[acct] rotation-switch 失败：", e);
    const reason = copyText("acct.reason.unknown");
    o.switchError = hot ? copyText("acct.sw.failHot", { cur: accountLabel(cur ?? "_"), reason }) : copyText("acct.sw.failRestart", { reason });
  } finally {
    setDisabled(go, null);
    await refreshSessions(o.origin, [o.sid]);
    if (open === o) render(o, host);
  }
}

/** 等那台定位 · 停旧 ＋ 起新那几步的底数（等报出之外）。 */
const RESTART_BASE_MS = 60_000;

/** 重启切换：先问那台会打断什么（有才问）、再交那台 `rotation-switch`（重启那一形，逐个交给 `session-restart`）、成了开终端接上。 */
async function restartSwitch(o: Open, host: AcctPanelHost, target: string): Promise<void> {
  const standing = await standingOf(o.origin, o.sid);
  if (standing?.kind !== "running") {
    o.switchError = copyText("acct.sw.failRestart", { reason: reasonLabel("not_in_terminal", { agent: ACTIVE_AGENT, target }) });
    return;
  }
  const go = await confirmInterrupts({
    title: copyText("acct.sw.confirmTitle", { name: accountLabel(target) }),
    action: copyText("acct.sw.confirmGo"),
    keep: [copyText("acct.sw.confirmKeep")],
    ask: () => askSessionInterrupts(o.origin, o.sid),
  });
  if (!go) return;
  const args = {
    sid: o.sid,
    cwd: host.cwdOf(o.sid),
    compact_first: false,
    compact_within_ms: 0,
    arrive_within_ms: ARRIVAL_BUDGET_MS,
    local: isLocalOrigin(o.origin),
    ...(await startSettings(o.origin)),
  };
  const got = (await switchRestart(o.origin, [args], target, ARRIVAL_BUDGET_MS + RESTART_BASE_MS))[o.sid];
  o.switchError = null;
  if (!got || got.state !== "done") {
    const reason = reasonLabel(got ? got.code : "", { agent: ACTIVE_AGENT, target });
    const log = { label: copyText("acct.sw.log"), run: () => void commands.open_log_file().catch((e) => console.warn("open_log_file failed:", e)) };
    if (got?.old === "ended") {
      toast(copyText("acct.sw.failRestartEnded", { ended: copyText("sessionState.ended.name"), reason }), "", {
        level: "error",
        action: [{ label: copyText("acct.sw.resume"), run: () => host.openResume(o.sid) }, log],
      });
    } else {
      toast(copyText("acct.sw.failRestart", { reason }), "", { level: "error", action: [log] });
    }
    return;
  }
  o.pick = null;
  toast(copyText("acct.sw.doneRestart", { name: accountLabel(target), session: host.sessionTitle(o.sid) }), "", { level: "info" });
  await runRemoteAttach(o.origin, ACTIVE_AGENT, got.terminal, { quiet: true });
}

// ─────────────────────────────── 记录 · 底栏

const HISTORY_SHOWN = 5;

function historyBlock(entry: SessionRotationEntry, read: Present): HTMLElement {
  const sec = section(copyText("acct.hist.title"));
  const now = entry.now;
  const tz = localTzMin(now);
  const line = (at: string, parts: HTMLElement[]): HTMLElement => {
    const r = el("div", s.acctHistRow);
    r.append(el("span", s.acctHistAt, at), ...parts);
    return r;
  };
  const rows = [...read.account.history].reverse().map((h) => {
    const w = whyOf(h, now, tz);
    const move = el("span", s.acctHistMove);
    move.append(acctAvatar(h.from), document.createTextNode(accountLabel(h.from)));
    if (h.to !== h.from) move.append(el("span", s.acctHistArrow, copyText("acct.strip.arrow")), acctAvatar(h.to), document.createTextNode(accountLabel(h.to)));
    const why = el("span", s.acctHistWhy, w.why);
    if (w.reset) why.appendChild(el("span", s.acctHistReset, w.reset));
    return line(fmtAt(h.at, now, tz), [move, why]);
  });
  const startRow = el("span", s.acctHistMove, copyText("acct.hist.start", { name: accountLabel(read.account.start) }));
  rows.push(line(read.account.history.length === 0 ? fmtAt(read.account.since, now, tz) : "", [startRow]));
  sec.content.append(...rows.slice(0, HISTORY_SHOWN));
  if (rows.length > HISTORY_SHOWN) {
    const more = el("div", s.acctHistMore);
    more.append(...rows.slice(HISTORY_SHOWN));
    sec.content.appendChild(fold({ title: copyText("acct.hist.more", { n: rows.length - HISTORY_SHOWN }), open: false, body: more }));
  }
  return sec.root;
}

function footer(o: Open, host: AcctPanelHost): HTMLElement {
  const f = el("div", s.acctFoot);
  f.appendChild(el("span", s.acctFootLabel, copyText("acct.foot.default")));
  const def = host.defaultOf(o.origin);
  const b = el("button", s.acctPick);
  b.type = "button";
  if (def) b.append(acctAvatar(def), el("span", s.acctPickName, accountLabel(def)));
  b.appendChild(icon("caretDown", "compact"));
  b.addEventListener("click", () => {
    closeMenu();
    host.openDefaultMenu(b, o.origin);
  });
  f.append(b, el("span", s.acctSpacer), button({ label: copyText("acct.foot.manage"), kind: "ghost", size: "compact", onClick: () => host.openSettings(o.origin) }));
  return f;
}
