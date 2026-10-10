/**
 * 「账号」面板（右侧抽屉）：状态栏账号按钮 · 右键「账号…」· 会话头 ⋯ 三处开的同一个。四块 ＋ 底栏：
 * 当前 · 轮换（来源下拉 `跟随默认 · 本会话`、触发 `满 / ≥N%`、无号可换 `继续跑 / 停`、勾与拖序、折着的时间轴）· 切换（选号、热切换 / 重启切换）· 记录 ·
 * 底栏 `新会话默认 [work ▾] … 管理账号…`。
 *
 * - 数据只经 `quota-read` · `rotation-rules-read` · `rotation-session-read/-set` · `rotation-switch`（`quota-reads.ts`）与推送
 *   （`acct-center.ts` 收进 `appStore`）。界面不判「能不能换 / 该不该换」：热切换成不成立看 `inPlace`、下一个看 `next`、
 *   满没满看显示态，都是后端给的。新勾的按量号挪到末尾也是后端做的。
 * - 改即生效，不要保存；存失败列表退回原样、块顶一条 `未保存 · 轮换未变 [重试]`。
 * - 实时：数据变了原地重画；正在拖、正在填 `≥N%` 时不重画（松手 / 失焦再画）。
 */
import { displayNameOf, lookupAgentProfile } from "./agent-profile";
import { appStore, type SessionRotationEntry } from "./app-store";
import { refreshQuota, refreshSessions } from "./acct-center";
import { acctAvatar } from "./acct-dom";
import {
  ledgerOf,
  machineLabel,
  reasonLabel,
  swappedFrom,
  usageOf,
  usageText,
  whyOf,
} from "./acct-view";
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
import { select } from "./kit/select";
import {
  capButton,
  capTags,
  fallbackMark,
  fallbackToggle,
  howControl,
  moved,
  openSaveAsRule,
  rowsOf,
  rulePeek,
  toggled,
  waitControl,
} from "./rot-editor";
import { toast } from "./kit/toast";
import { attachTooltip } from "./kit/tooltip";
import { ARRIVAL_BUDGET_MS } from "./launch-arrival";
import {
  accountLabel,
  slotLabel,
  slotText,
  type QuotaRead,
  type QuotaReadAccount,
} from "./acct-words";
import {
  readPlan,
  type PlanRead,
  defaultRuleOf,
  switchHot,
  switchRestart,
  writeSessionRotation,
  type SessionRotationWrite,
} from "./quota-reads";
import { runRemoteAttach } from "./remote-launch-run";
import { headLine, timelineAxis, viewSwitch, type TlView } from "./rot-timeline";
import { standingOf } from "./sessions-where";
import { startSettings } from "./tab-batch-run";
import type { AtLimit } from "./generated/AtLimit";
import type { QuotaShow } from "./generated/QuotaShow";
import type { Rotation } from "./generated/Rotation";
import type { RotationSource } from "./generated/RotationSource";
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
  /** 设置窗那台的「轮换」栏（`管理规则…`）；给了规则 ⇒ 滚到那一条（`编辑规则…`）。 */
  openRules(origin: Origin, rule?: string): void;
  /** 底栏「新会话默认」那一颗：开那台的默认账号下拉（与无会话时状态栏按钮的同一个）。 */
  openDefaultMenu(anchor: HTMLElement, origin: Origin): void;
  /** 新会话默认此刻是哪个号（底栏那颗按钮上的字）。 */
  defaultOf(origin: Origin): string | null;
  /** 开这条会话的恢复菜单（与右键「恢复 ▸」同一份）。 */
  openResume(sid: string): void;
  /** 这条会话是哪一家（线上的 kind，会话事实给的）；还不知道 ⇒ `null`。 */
  agentOf(sid: string): string | null;
  /** 打开一条会话（「打开父会话」）：切到它的标签页（同历史页在跑那一行的「切过去」）。 */
  openSession(sid: string): void;
  /** 这条会话在不在会话列表里（不在 ⇒「打开父会话」灰）。 */
  canOpenSession(sid: string): boolean;
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
  /** 时间轴：视窗 · 最后一趟回答 · 那一趟问的钥匙 · 第几趟（只认最后一趟）。 */
  tl: { view: TlView; plan: PlanRead | null; key: string; seq: number };
  /** 正在拖 / 正在填：数据到了先不重画。 */
  hold: boolean;
  pending: boolean;
  unsub: (() => void)[];
}

let open: Open | null = null;

/** 这条会话那一家的全称（话里用）；还不知道 ⇒ 那一家的泛称。 */
function agentName(host: AcctPanelHost, o: Open): string {
  return displayNameOf(host.agentOf(o.sid)) ?? copyText("acct.agent.unknown");
}

/** 这条会话那一家在额度账里的名字（路由第 1 段 ＝ 那一家的适配器 id）；还不知道 ⇒ `null`。 */
function routeOf(host: AcctPanelHost, o: Open): string | null {
  const kind = host.agentOf(o.sid);
  if (kind === null) return null;
  const got = lookupAgentProfile(kind);
  return got.known ? got.facts.adapterId : null;
}

/** 开 / 关这个会话的面板（再点同一个入口 ＝ 关）。 */
export function toggleAccountPanel(
  sid: string,
  origin: Origin,
  host: AcctPanelHost,
): void {
  if (open?.sid === sid) {
    void open.drawer.close();
    return;
  }
  openAccountPanel(sid, origin, host);
}

/** 命令面板「套用轮换规则…」：开（或留着）这个会话的面板，直接摊开来源下拉（与面板里同一份项、同一套写法）。 */
export function openSourcePicker(
  sid: string,
  origin: Origin,
  host: AcctPanelHost,
): void {
  if (open?.sid !== sid) openAccountPanel(sid, origin, host);
  open?.body.querySelector<HTMLElement>("[data-acct-src]")?.click();
}

/** 当前 tab 换了：面板开着就跟着换成那个会话（不关）。 */
export function followActive(
  a: { sid: string; origin: Origin } | null,
  host: AcctPanelHost,
): void {
  if (!open || a === null || open.sid === a.sid) return;
  openAccountPanel(a.sid, a.origin, host);
}

export function openAccountPanel(
  sid: string,
  origin: Origin,
  host: AcctPanelHost,
): void {
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
    sub: copyText("acct.panel.sub", {
      session: host.sessionTitle(sid),
      machine: machineLabel(origin),
    }),
    body,
    width: 440,
    bottom: statusBar
      ? Math.round(statusBar.getBoundingClientRect().height)
      : undefined,
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
    tl: { view: "24h", plan: null, key: "", seq: 0 },
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
  o.unsub.push(
    appStore.quota.subscribe(repaint),
    appStore.sessionRotation.subscribe(repaint),
    appStore.rotationRules.subscribe(repaint),
    appStore.accounts.subscribe(repaint),
  );
  render(o, host);
  // 焦点落在来源下拉（合着时不写任何东西）；没有那一块（中转没见过）⇒ 留在关闭钮上。
  body.querySelector<HTMLElement>("[data-acct-src]")?.focus();
  // 打开那一刻再问一次这个会话（推送丢过也能补上）；额度账同理。
  void refreshSessions(origin, [sid]);
  void refreshQuota(origin);
}

/** 拖完 / 填完：放开重画。 */
function release(o: Open, host: AcctPanelHost): void {
  o.hold = false;
  if (o.pending) {
    o.pending = false;
    render(o, host);
  }
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  cls: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function section(
  title: string,
  right?: HTMLElement,
): { root: HTMLElement; content: HTMLElement } {
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
    o.body.append(
      rotationBlock(o, read, quota, host),
      switchBlock(o, entry, read, quota, host),
      historyBlock(read),
    );
  } else {
    o.body.append(switchBlock(o, entry, undefined, quota, host));
  }
  o.body.append(footer(o, host));
  if (o.body.parentElement) o.body.parentElement.scrollTop = scroll;
}

// ─────────────────────────────── 当前

function slotMeter(q: QuotaShow, slot: string): HTMLElement {
  const x = q.slots.find((v) => v.slot === slot);
  const here = q.limiting === slot;
  let state: MeterState = "normal";
  if (!x || x.pct === undefined) state = "none";
  else if (q.stale) state = "stale";
  else if (x.full || (here && q.state === "refused")) state = "refused";
  else if (here && (q.state === "near" || q.state === "overageInUse"))
    state = "near";
  const value = slotText(q, slot);
  const at = x?.resetsAt;
  const rel = at === undefined ? null : (x?.resetsAtRelText ?? null);
  const said = x?.resetsAtText ?? "";
  const reset =
    at === undefined
      ? undefined
      : rel === null
        ? copyText("acct.reset.past", { at: said })
        : copyText("acct.reset.at", { at: said });
  const row = el("div", s.acctMeterRow);
  row.appendChild(
    meter({
      label: slotLabel(slot),
      ratio: (x?.pct ?? 0) / 100,
      state,
      value,
      reset,
    }),
  );
  row.appendChild(el("span", s.acctMeterRel, rel ?? ""));
  return row;
}

function tagEl(text: string, tone?: "cur" | "warn" | "error"): HTMLElement {
  const t = el("span", s.acctTag, text);
  t.title = text;
  t.dataset.acctTag = "";
  if (tone) t.dataset.shade = tone;
  return t;
}

function kindTags(
  q: Pick<QuotaShow, "kind" | "login" | "state">,
): HTMLElement[] {
  const tags = [
    tagEl(
      q.kind === "api" ? copyText("acct.kind.api") : copyText("acct.kind.sub"),
    ),
  ];
  if (q.login === "needsLogin")
    tags.push(tagEl(copyText("acct.tag.login"), "error"));
  if (q.login === "needsKey")
    tags.push(tagEl(copyText("acct.tag.key"), "error"));
  if (q.state === "overageInUse")
    tags.push(tagEl(copyText("acct.val.over"), "warn"));
  return tags;
}

function nowBlock(
  o: Open,
  entry: SessionRotationEntry | undefined,
  quota: QuotaRead | null,
  host: AcctPanelHost,
): HTMLElement {
  const read = entry?.read;
  if (!entry || !read || read.state === "absent") {
    const sec = section(copyText("acct.now.title"));
    const connect = button({
      label: copyText("acct.now.connect"),
      kind: "ghost",
      size: "compact",
      onClick: () => host.openSettings(o.origin),
    });
    sec.content.appendChild(
      banner("warn", copyText("acct.now.noRelay"), [connect]),
    );
    return sec.root;
  }
  const led = ledgerOf(quota, read.agent, read.account.current);
  const seen = led ? (led.seenAtText ?? "") : null;
  const right =
    seen === null
      ? undefined
      : el(
          "span",
          s.acctBlockNote,
          isLocalOrigin(o.origin)
            ? copyText("acct.now.seen", { at: seen })
            : copyText("acct.now.seenOther", { machine: o.origin, at: seen }),
        );
  const sec = section(copyText("acct.now.title"), right);
  if (quota === null) {
    const retry = button({
      label: copyText("acct.now.retry"),
      kind: "ghost",
      size: "compact",
      onClick: () => void refreshQuota(o.origin),
    });
    sec.content.appendChild(
      banner("error", copyText("acct.now.readFail"), [retry]),
    );
  }
  const who = el("div", s.acctWho);
  who.appendChild(acctAvatar(read.account.current, 20));
  who.appendChild(
    el("span", s.acctWhoName, accountLabel(read.account.current)),
  );
  who.append(...kindTags(read.quota));
  const from = swappedFrom(read);
  if (from)
    who.appendChild(
      el(
        "span",
        s.acctWhoFrom,
        copyText("acct.hover.from", {
          at: from.at,
          name: accountLabel(from.from),
        }),
      ),
    );
  sec.content.appendChild(who);
  if (read.quota.kind === "api") {
    const u = usageOf(read.quota, led?.reading);
    const line = el(
      "div",
      s.acctApiLine,
      read.quota.state === "refused" ? u.value : copyText("acct.val.noLimit"),
    );
    if (read.quota.state === "refused") line.dataset.shade = "refused";
    if (u.reset) line.appendChild(el("span", s.acctMeterRel, u.reset));
    sec.content.appendChild(line);
  } else {
    sec.content.append(
      slotMeter(read.quota, "5h"),
      slotMeter(read.quota, "7d"),
    );
  }
  return sec.root;
}

// ─────────────────────────────── 轮换

function quotaOf(
  quota: QuotaRead | null,
  agent: string,
  account: string,
): QuotaShow | null {
  const led = ledgerOf(quota, agent, account);
  if (led) return led;
  const u = quota?.unseen.find(
    (x) => x.agent === agent && x.account === account,
  );
  return u
    ? { kind: u.kind, state: "unseen", stale: false, slots: [], login: u.login }
    : null;
}

/** 一行右侧的用量：`5h ▮ 63% ↻18:30`（按量号 `无 5h / 7d`；被拒 `✕ ↻19:00`）。 */
/** 只画卡着它的那一个窗口（轮换列表行尾有兜底 · 封顶，照稿只留一格用量；另一格在悬停卡与时间轴里）。 */
function rowUsage(
  q: QuotaShow | null,
  reading: QuotaReadAccount["reading"],
): HTMLElement {
  const box = el("span", s.acctRowUsage);
  if (!q) return box;
  if (q.kind === "api") {
    const u = usageOf(q, reading);
    box.appendChild(
      el(
        "span",
        s.acctRowSlotVal,
        q.state === "refused" ? u.value : copyText("acct.val.noLimit"),
      ),
    );
    if (u.reset) box.appendChild(el("span", s.acctRowSlotReset, u.reset));
    if (q.state === "refused") box.dataset.shade = "refused";
    return box;
  }
  for (const slot of [q.limiting ?? "5h"]) {
    const x = q.slots.find((v) => v.slot === slot);
    const here = q.limiting === slot;
    const cell = el("span", s.acctRowSlot);
    cell.appendChild(el("span", s.acctRowSlotKey, slotLabel(slot)));
    const bar = el("span", s.acctMiniBar);
    const fill = el("span", s.acctMiniFill);
    fill.style.transform = `scaleX(${Math.max(0, Math.min(1, (x?.pct ?? 0) / 100))})`;
    bar.appendChild(fill);
    cell.appendChild(bar);
    cell.appendChild(el("span", s.acctRowSlotVal, slotText(q, slot)));
    if (here && x?.resetsAt !== undefined && slot === "5h") {
      const rel = x.resetsAtRelText ?? null;
      cell.appendChild(
        el(
          "span",
          s.acctRowSlotReset,
          rel === null
            ? copyText("acct.reset.past", { at: x.resetsAtText ?? "" })
            : copyText("acct.reset.at", { at: x.resetsAtText ?? "" }),
        ),
      );
    }
    if (x?.full || (here && q.state === "refused"))
      cell.dataset.shade = "refused";
    else if (here && (q.state === "near" || q.state === "overageInUse"))
      cell.dataset.shade = "warn";
    if (q.stale) cell.dataset.stale = "true";
    box.appendChild(cell);
  }
  return box;
}

/** 规则多过这么多条 ⇒ 来源下拉顶上出筛选框（选项里另有「跟随默认」「本会话」两项）。 */
const SRC_FILTER_OVER = 10;

/** 「跟随父会话」那一项右侧灰字里父标题至多几个字（后面还要放规则名）。 */
const PARENT_NOTE_CHARS = 12;

/** 截到 `n` 个字，截了补 `…`。 */
function clip(t: string, n: number): string {
  const cs = [...t];
  return cs.length > n ? `${cs.slice(0, n - 1).join("")}…` : t;
}

/** 来源下拉面板的宽（项右侧带规则摘要，比框宽）。 */
const SRC_MENU_W = 340;

/** 会话的来源在下拉里那一项的值：`follow` · `parent` · `custom` · `rule:<id>`。 */
type SrcKey = "follow" | "parent" | "custom" | `rule:${string}`;

function srcKeyOf(src: RotationSource): SrcKey {
  if (typeof src === "string") return src;
  return "rule" in src ? `rule:${src.rule}` : "parent";
}

function srcWrite(k: SrcKey): SessionRotationWrite {
  return k === "follow" || k === "custom" || k === "parent"
    ? k
    : { rule: k.slice("rule:".length) };
}

function rotationBlock(
  o: Open,
  read: Present,
  quota: QuotaRead | null,
  host: AcctPanelHost,
): HTMLElement {
  const rules = appStore.rotationRules.get().get(o.origin) ?? null;
  const def = defaultRuleOf(rules);
  const own = read.source === "custom";
  const follow = !own;
  const src = read.source;
  const parentSrc = typeof src === "object" && "parent" in src;
  // 跟随父会话：后端顺着父解析出此刻那条规则的名字（父是本会话那份 ⇒ 没有名字，只排那一行、不画列表）。
  const ruleRow = parentSrc
    ? (rules?.rules.find((x) => x.name === read.ruleName) ?? null)
    : typeof src === "object" && "rule" in src
      ? (rules?.rules.find((x) => x.id === src.rule) ?? def)
      : src === "follow"
        ? def
        : null;
  const r: Rotation | null = own
    ? (read.custom ?? def?.rotation ?? null)
    : (ruleRow?.rotation ?? null);
  const sec = section(copyText("acct.rot.title"));

  const bar = el("div", s.acctRotBar);
  const was = srcKeyOf(read.source);
  const nameOf = (k: SrcKey): string =>
    rules?.rules.find((x) => `rule:${x.id}` === k)?.name ?? "";
  const label = (k: SrcKey): string =>
    k === "follow"
      ? copyText("rot.src.follow")
      : k === "parent"
        ? copyText("rot.src.parent")
        : k === "custom"
          ? copyText("rot.src.custom")
          : copyText("rot.src.rule", { name: nameOf(k) });
  const parentTitle = read.parent ? host.sessionTitle(read.parent) : "";
  // 来源：下拉，只有在面板里明确点一项才写（合着时方向键一概不做）；写成了 toast 带撤销。
  // 项：跟随默认（灰字默认那条的名字）· 组名「规则」下每条一项（默认那条带「默认」、右侧摘要）· 本会话 · 分隔 · 存为规则… · 管理规则…
  const srcSel = select({
    label: copyText("rot.src.label"),
    options: [
      { value: "follow", label: copyText("rot.src.follow"), note: def?.name },
      // 跟随父会话：只在血缘里有父时列（没有就不出，不画灰项）；右侧灰字 ＝ 父标题 · 父此刻那条规则（父是本会话那份 ⇒ 本会话）。
      ...(read.parent
        ? [
            {
              value: "parent",
              label: copyText("rot.src.parent"),
              note: [
                clip(parentTitle, PARENT_NOTE_CHARS),
                read.ruleName ?? copyText("rot.src.custom"),
              ]
                .filter((x) => x !== "")
                .join(copyText("kit.text.sep")),
              // 合着时框里只写「跟随父会话」（父标题常很长，挤掉左边的「来源」）；父是谁写在下面那一行。
              shownNote: "",
            },
          ]
        : []),
      ...(rules?.rules ?? []).map((x) => ({
        value: `rule:${x.id}`,
        label: x.name,
        note: x.isDefault ? copyText("rot.src.tagDefault") : undefined,
        detail: x.summary,
        shown: copyText("rot.src.rule", { name: x.name }),
        peek: () => rulePeek(x),
      })),
      {
        value: "custom",
        label: copyText("rot.src.custom"),
        detail: copyText("rot.src.customNote"),
      },
    ],
    value: was,
    closedKeys: "open",
    menuWidth: SRC_MENU_W,
    // 规则多过 10 条 ⇒ 面板顶上筛选框（只筛规则那几项）。
    filterOver: SRC_FILTER_OVER + 2,
    filterLabel: copyText("rot.src.filter"),
    decorate: (items) => {
      const out: MenuItem[] = [];
      let headed = false;
      for (const it of items) {
        if (it.id?.startsWith("rule:") && !headed) {
          out.push({ label: copyText("rot.src.rules"), heading: true });
          headed = true;
        }
        if (it.id === "custom" && !headed)
          out.push({ label: copyText("rot.src.noRules"), enabled: false });
        out.push(it);
      }
      out.push({ label: "", divider: true });
      out.push({
        id: "saveAs",
        label: copyText("rot.src.saveAs"),
        onClick: () => saveAs(),
      });
      out.push({
        id: "manage",
        label: copyText("rot.src.manage"),
        onClick: () => host.openRules(o.origin),
      });
      return out;
    },
    onChange: (k) => void setSource(o, host, k as SrcKey, was, label),
  });
  const srcEl = srcSel.el;
  srcEl.dataset.acctSrc = "true";
  const srcRow = el("div", s.acctSrcRow);
  srcRow.append(
    el("span", s.acctTriggerLabel, copyText("rot.src.label")),
    srcEl,
  );
  const info = el("span", s.acctInfo);
  info.tabIndex = 0;
  info.setAttribute("aria-label", copyText("acct.rot.infoAria"));
  info.appendChild(icon("info", "compact"));
  attachTooltip(info, read.explain);
  srcRow.appendChild(info);
  // 存为规则…：存的是此刻生效的那一份（从跟随 / 规则也能存，用于「在它基础上做一条新的」）。
  const saveAs = (): void => {
    if (!r) return;
    openSaveAsRule(saveBtn, o.origin, r, (rule, link) => {
      toast(copyText("rot.done.saved", { name: rule.name }), "", {
        level: "success",
      });
      if (link) void save(o, host, { rule: rule.id });
    });
  };
  const saveBtn = button({
    label: copyText("rot.src.saveAs"),
    kind: "ghost",
    size: "compact",
    onClick: saveAs,
  });
  saveBtn.dataset.acctSaveAs = "true";
  srcRow.append(el("span", s.acctSpacer), saveBtn);
  sec.content.appendChild(srcRow);
  if (r) bar.appendChild(triggerControls(o, host, r, follow, read.atLimit));
  sec.content.appendChild(bar);
  if (r && own) {
    const how = el("div", s.acctRotBar);
    how.append(
      howControl(r, false, (next) => void save(o, host, { custom: next })),
      waitControl(r, false, (next) => void save(o, host, { custom: next })),
    );
    sec.content.appendChild(how);
  }

  if (o.saveFailed) {
    const retry = button({
      label: copyText("acct.now.retry"),
      kind: "ghost",
      size: "compact",
      onClick: () => void save(o, host, { custom: o.saveFailed as Rotation }),
    });
    sec.content.appendChild(
      banner("error", copyText("acct.rot.saveFail"), [retry]),
    );
  }
  if (parentSrc) {
    // `跟随父会话 orders · 规则 日常`（父是本会话那份 ⇒ `· 本会话`；父没经过中转 ⇒ `· 父会话未经中转 · 按默认 日常`）
    // ＋［打开父会话］［转为本会话］。列表只读（同来源是规则）。
    const name = parentTitle;
    const rule = read.ruleName ?? "";
    const said = read.parentMissing
      ? copyText("rot.src.leadParentMissing", { name, rule })
      : read.ruleName
        ? copyText("rot.src.leadParent", { name, rule })
        : copyText("rot.src.leadParentOwn", { name });
    const lead = el("div", s.acctLead);
    lead.dataset.acctLead = "true";
    lead.append(el("span", s.acctSpacer, said));
    const p = read.parent ?? "";
    const can = p !== "" && host.canOpenSession(p);
    const openBtn = button({
      label: copyText("rot.src.openParent"),
      kind: "secondary",
      size: "compact",
      onClick: () => host.openSession(p),
    });
    if (!can) setDisabled(openBtn, copyText("rot.src.openParentOff"));
    lead.append(
      openBtn,
      button({
        label: copyText("rot.src.detach"),
        kind: "secondary",
        size: "compact",
        onClick: () => void detach(o, host, name, was),
      }),
    );
    sec.content.appendChild(lead);
  } else if (follow && ruleRow) {
    // `规则 日常 · 默认 · 在用 4 会话`（跟随时 `跟随默认 · 规则 日常`）＋［编辑规则…］［转为本会话］。列表只读：共享的规则去设置里改。
    const parts = [
      src === "follow"
        ? copyText("rot.src.leadFollow", { name: ruleRow.name })
        : copyText("rot.src.leadRule", { name: ruleRow.name }),
    ];
    if (src !== "follow" && ruleRow.isDefault)
      parts.push(copyText("rot.src.tagDefault"));
    if (ruleRow.users.live > 0)
      parts.push(copyText("rot.src.inUse", { n: ruleRow.users.live }));
    const lead = el("div", s.acctLead);
    lead.dataset.acctLead = "true";
    lead.append(el("span", s.acctSpacer, parts.join(copyText("kit.text.sep"))));
    lead.append(
      button({
        label: copyText("rot.src.edit"),
        kind: "secondary",
        size: "compact",
        onClick: () => host.openRules(o.origin, ruleRow.id),
      }),
      button({
        label: copyText("rot.src.detach"),
        kind: "secondary",
        size: "compact",
        onClick: () => void detach(o, host, ruleRow.name, was),
      }),
    );
    sec.content.appendChild(lead);
  }
  if (r)
    sec.content.appendChild(
      rotationList(o, host, read, r, quota, !follow),
    );
  const tl = timelineFold(o, host, read, quota);
  sec.content.appendChild(tl);
  return sec.root;
}

/** `触发 (•)满 ( )≥[90]%` ＋ `无号可换 [继续跑 | 停]`（后者只在 `≥N%` 时有效）。 */
function triggerControls(
  o: Open,
  host: AcctPanelHost,
  r: Rotation,
  readonly: boolean,
  actual: AtLimit,
): HTMLElement {
  const box = el("span", s.acctTrigger);
  box.appendChild(el("span", s.acctTriggerLabel, copyText("acct.rot.trigger")));
  const name = `acct-trigger-${o.sid}`;
  const pctMode = r.when !== "full";
  const n = r.when === "full" ? 90 : r.when.threshold.n;
  const radio = (
    on: boolean,
    label: string,
    pick: () => void,
  ): HTMLLabelElement => {
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
    radio(
      !pctMode,
      copyText("acct.rot.trigFull"),
      () => void save(o, host, { custom: { ...r, when: "full" } }),
    ),
    radio(
      pctMode,
      copyText("acct.rot.trigPct"),
      () =>
        void save(o, host, { custom: { ...r, when: { threshold: { n } } } }),
    ),
    num,
    el("span", s.acctTriggerUnit, copyText("acct.rot.pctUnit")),
  );
  // 无号可换时怎么办（稿里没画的那一格）：两态分段；「满」触发时没有 N%，灰着。
  const lim = el("span", s.acctLimit);
  const label = el("span", s.acctTriggerLabel, copyText("acct.lim.label"));
  attachTooltip(label, () =>
    pctMode
      ? copyText("acct.lim.labelHint", { n })
      : copyText("acct.lim.fullOnly"),
  );
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
  if (r.atLimit === "stop" && actual === "continue" && pctMode)
    lim.appendChild(
      el(
        "span",
        s.acctBlockNote,
        copyText("acct.lim.downgraded", { agent: agentName(host, o) }),
      ),
    );
  const wrap = el("span", s.acctTriggerWrap);
  wrap.append(box, lim);
  return wrap;
}

function rotationList(
  o: Open,
  host: AcctPanelHost,
  read: Present,
  r: Rotation,
  quota: QuotaRead | null,
  editable: boolean,
): HTMLElement {
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
    if (q && (q.state === "refused" || q.login !== "ok"))
      line.dataset.dim = "true";
    const handle = el("span", s.acctHandle);
    handle.appendChild(icon("drag", "compact"));
    handle.setAttribute(
      "aria-label",
      copyText("acct.row.dragAria", { name: accountLabel(row.account) }),
    );
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = row.on;
    box.setAttribute(
      "aria-label",
      copyText("acct.row.checkAria", { name: accountLabel(row.account) }),
    );
    const locked = row.start || row.account === cur;
    box.disabled =
      !editable ||
      locked ||
      q?.login === "needsLogin" ||
      q?.login === "needsKey";
    if (row.account === cur && editable)
      attachTooltip(line, copyText("acct.rot.lockHint"));
    box.addEventListener("change", () => {
      void save(o, host, { custom: toggled(r, row.account, box.checked) });
    });
    if (editable) line.appendChild(handle);
    line.append(
      box,
      acctAvatar(row.account),
      el("span", s.acctRowName, accountLabel(row.account)),
    );
    if (row.start) line.appendChild(tagEl(copyText("acct.tag.start")));
    if (row.account === cur)
      line.appendChild(tagEl(copyText("acct.tag.cur"), "cur"));
    if (q) {
      if (q.kind === "api") line.appendChild(tagEl(copyText("acct.kind.api")));
      if (q.login === "needsLogin")
        line.appendChild(tagEl(copyText("acct.tag.login"), "error"));
      if (q.login === "needsKey")
        line.appendChild(tagEl(copyText("acct.tag.key"), "error"));
      if (q.state === "overageInUse")
        line.appendChild(tagEl(copyText("acct.val.over"), "warn"));
    }
    // 封顶：本会话那份可改（行尾一个小按钮开浮层，编的是这号全部窗口那一格）；只读的写成小标签。
    // 兜底（排在封顶之后）：本会话那份顺序里具名的号上一个开关（开 ⇒ 实心标；关 ⇒ 悬停 / 行内有焦点才出）；只读的只画开着的那个实心标。
    const named = r.order.includes(row.account);
    if (editable && !row.start)
      line.appendChild(
        capButton(
          o.origin,
          r,
          row.account,
          (next) => void save(o, host, { custom: next }),
        ),
      );
    else
      for (const t of capTags(r, row.account)) {
        const tag = tagEl(t);
        tag.dataset.cap = "";
        line.appendChild(tag);
      }
    if (editable && named)
      line.appendChild(
        fallbackToggle(
          r,
          row.account,
          accountLabel(row.account),
          (next) => void save(o, host, { custom: next }),
        ),
      );
    else if ((r.fallback ?? []).includes(row.account))
      line.appendChild(fallbackMark());
    line.appendChild(
      rowUsage(q, ledgerOf(quota, read.agent, row.account)?.reading),
    );
    if (editable) {
      // 键盘：Alt+↑ / Alt+↓ 移位、空格勾（复选框自己管）。
      line.addEventListener("keydown", (ev) => {
        if (
          ev.isComposing ||
          !ev.altKey ||
          (ev.key !== "ArrowUp" && ev.key !== "ArrowDown")
        )
          return;
        ev.preventDefault();
        const to = ev.key === "ArrowUp" ? i - 1 : i + 1;
        if (to < 0 || to >= rows.length) return;
        void save(o, host, { custom: moved(r, rows, i, to) });
      });
      handle.addEventListener("pointerdown", (ev) =>
        startDrag(
          ev,
          o,
          host,
          list,
          elems,
          i,
          (to) => void save(o, host, { custom: moved(r, rows, i, to) }),
        ),
      );
    }
    elems.push(line);
    list.appendChild(line);
  });
  return list;
}

/** 只从把手拖：拖起的行浮起、落点顶边一条插入线、原位留影子；松手即存；拖出列表 ＝ 不动；Esc 取消。 */
function startDrag(
  ev: PointerEvent,
  o: Open,
  host: AcctPanelHost,
  list: HTMLElement,
  rows: HTMLElement[],
  from: number,
  done: (to: number) => void,
): void {
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
    let k = rows.findIndex(
      (r) =>
        y <
        r.getBoundingClientRect().top + r.getBoundingClientRect().height / 2,
    );
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

/** 写这个会话的轮换：成 ⇒ 重问这个会话；败 ⇒ 列表退回原样、块顶一条错误条（重试写同一份）。回成没成。 */
async function save(
  o: Open,
  host: AcctPanelHost,
  w: SessionRotationWrite,
): Promise<boolean> {
  let ok = true;
  try {
    const got = await writeSessionRotation(o.origin, [o.sid], w);
    const one = got[o.sid];
    if (one && one.state !== "done") throw new Error(one.code);
    o.saveFailed = null;
  } catch (e) {
    console.warn("[acct] rotation-session-set 失败：", e);
    o.saveFailed = typeof w === "object" && "custom" in w ? w.custom : null;
    ok = false;
    if (open === o) render(o, host);
  }
  void refreshSessions(o.origin, [o.sid]).then(() => {
    if (open === o) render(o, host);
  });
  return ok;
}

/** 转为本会话：照此刻生效的那条拷成本会话的（之后脱钩）；toast `orders · 本会话（从 日常 转来）[撤销]`，撤销 ＝ 写回原来源。 */
async function detach(
  o: Open,
  host: AcctPanelHost,
  name: string,
  was: SrcKey,
): Promise<void> {
  if (!(await save(o, host, "detach"))) return;
  toast(
    copyText("rot.done.detach", { session: host.sessionTitle(o.sid), name }),
    "",
    {
      level: "success",
      action: {
        label: copyText("kit.toast.undo"),
        run: () => void save(o, host, srcWrite(was)),
      },
    },
  );
}

/** 换来源：写一次；成了 toast `orders · 跟随默认 [撤销]`，撤销 ＝ 写回原来源（本会话那份后端一直留着）。 */
async function setSource(
  o: Open,
  host: AcctPanelHost,
  to: SrcKey,
  was: SrcKey,
  label: (k: SrcKey) => string,
): Promise<void> {
  if (!(await save(o, host, srcWrite(to)))) return;
  toast(
    copyText("rot.done.src", {
      session: host.sessionTitle(o.sid),
      src: label(to),
    }),
    "",
    {
      level: "success",
      action: {
        label: copyText("kit.toast.undo"),
        run: () => void save(o, host, srcWrite(was)),
      },
    },
  );
}

// ─────────────────────────────── 时间轴（折在轮换下）

/** 时间轴：后端 `rotation-plan {sid, view}` 的回答照排；顶行 ＝ 后端那一句（卡住时琥珀条），右侧 `6h | 24h | 7d`。
 * 回答按「这一刻的会话轮换 ＋ 额度账 ＋ 视窗」记一把钥匙：钥匙变了才重问，回来了重画（同一把钥匙不再问 ⇒ 不会画了又问）。 */
function timelineFold(
  o: Open,
  host: AcctPanelHost,
  read: Present,
  quota: QuotaRead | null,
): HTMLElement {
  const key = `${o.tl.view}|${JSON.stringify(read)}|${quota?.now ?? 0}`;
  if (o.tl.key !== key) {
    o.tl.key = key;
    const seq = ++o.tl.seq;
    readPlan(o.origin, { sid: o.sid, view: o.tl.view }).then(
      (p) => {
        if (open !== o || o.tl.seq !== seq) return;
        o.tl.plan = p;
        if (o.hold) o.pending = true;
        else render(o, host);
      },
      () => {
        if (open !== o || o.tl.seq !== seq) return;
        o.tl.plan = null;
      },
    );
  }
  const p = o.tl.plan;
  const head = p ? headLine(p) : null;
  const body = el("div", s.acctTimeline);
  const top = el("div", s.acctTlTop);
  // 顶行按段换行：窄了在段与段之间换（不把「估 几点 到 N%」拆成两截；一段比整行还宽才在段内折）。
  //   卡住那一句照常折（琥珀框里按字折更紧）。
  const line = el("span", s.acctTlHead, head?.blocked ? head.text : "");
  if (!head?.blocked)
    (head?.parts ?? []).forEach((t, i) => {
      if (i > 0) line.append(copyText("kit.text.sep"));
      line.appendChild(el("span", s.acctTlHeadPart, t));
    });
  line.dataset.tlHead = head?.blocked ? "blocked" : "now";
  top.append(
    line,
    viewSwitch(o.tl.view, (v) => {
      o.tl.view = v;
      render(o, host);
    }),
  );
  body.appendChild(top);
  if (p) body.appendChild(timelineAxis(p, { track: copyText("rot.tl.track"), compact: true }));
  // 折着时摘要 ＝ 顶行那一句；摊开时顶行在里面，摘要不重写一遍。
  const opened = o.timelineOpen || head?.blocked === true;
  return fold({
    title: copyText("acct.tl.title"),
    summary: opened ? "" : (head?.text ?? ""),
    open: opened,
    body,
    onToggle: (v) => {
      o.timelineOpen = v;
      render(o, host);
    },
  });
}

// ─────────────────────────────── 切换

function switchBlock(
  o: Open,
  entry: SessionRotationEntry | undefined,
  read: Present | undefined,
  quota: QuotaRead | null,
  host: AcctPanelHost,
): HTMLElement {
  const sec = section(copyText("acct.sw.title"));
  // 中转没见过这个会话 ⇒ 不知道它的路由名，取这台额度账里那一家（今天只有一家）。
  const agent =
    read?.agent ??
    routeOf(host, o) ??
    quota?.accounts[0]?.agent ??
    quota?.unseen[0]?.agent ??
    "";
  const cur = read?.account.current ?? null;
  const inPlace = read
    ? read.account.inPlace
    : entry?.read.state === "absent"
      ? entry.read.inPlace
      : "noRelay";
  const candidates = [...(quota?.accounts ?? []), ...(quota?.unseen ?? [])]
    .filter((x) => x.agent === agent && x.account !== "_")
    .map((x) => x.account);
  // 缺省选后端给的下一个；没给（中转没见过）⇒ 此刻发得出去的头一个（`usableNow`，后端判的）。
  const pick =
    o.pick ??
    read?.next ??
    candidates.find((a) => a !== cur && (quota?.usableNow ?? []).includes(a)) ??
    candidates.find((a) => a !== cur) ??
    null;
  const pickBtn = el("button", s.acctPick);
  pickBtn.type = "button";
  pickBtn.setAttribute("aria-label", copyText("acct.sw.pickAria"));
  if (pick) {
    pickBtn.append(
      acctAvatar(pick),
      el("span", s.acctPickName, accountLabel(pick)),
    );
    const q = quotaOf(quota, agent, pick);
    if (q) {
      const u = usageOf(q, ledgerOf(quota, agent, pick)?.reading);
      pickBtn.appendChild(el("span", s.acctPickUsage, usageText(u)));
    }
  } else {
    pickBtn.appendChild(
      el("span", s.acctPickName, copyText("acct.sw.optNone")),
    );
  }
  pickBtn.appendChild(icon("caretDown", "compact"));
  pickBtn.addEventListener("click", () => {
    const items: MenuItem[] = candidates.map((a) => {
      const q = quotaOf(quota, agent, a);
      const u = q ? usageOf(q, ledgerOf(quota, agent, a)?.reading) : null;
      const usage = u ? usageText(u) : undefined;
      const blocked =
        a === cur || q?.login === "needsLogin" || q?.login === "needsKey";
      return {
        label: accountLabel(a),
        avatar: acctAvatar(a),
        checked: a === pick,
        detail:
          a === cur
            ? copyText("acct.tag.cur")
            : q?.login === "needsLogin"
              ? copyText("acct.tag.login")
              : q?.login === "needsKey"
                ? copyText("acct.tag.key")
                : usage,
        detailTone:
          u?.tone === "refused" || u?.tone === "near" ? "warn" : undefined,
        enabled: !blocked,
        onClick: () => {
          o.pick = a;
          o.switchError = null;
          render(o, host);
        },
      };
    });
    openMenu({ el: pickBtn, align: "start" }, items, {
      label: copyText("acct.sw.pickAria"),
    });
  });
  const hotOk = inPlace === "ok";
  const mode = hotOk ? o.mode : "restart";
  const go = button({
    label:
      mode === "restart" ? copyText("acct.sw.restart") : copyText("acct.sw.go"),
    kind: "primary",
    onClick: () => void doSwitch(o, host, pick, cur, read, go),
  });
  const row = el("div", s.acctSwitchRow);
  row.append(pickBtn, go);
  sec.content.appendChild(row);

  const name = `acct-mode-${o.sid}`;
  const modes = el("div", s.acctModes);
  const radio = (
    m: "hot" | "restart",
    label: string,
    disabled: boolean,
  ): HTMLLabelElement => {
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
  if (!hotOk)
    modes.appendChild(
      el(
        "span",
        s.acctReason,
        reasonLabel(inPlace, { agent, target: pick ?? "" }),
      ),
    );
  modes.append(radio("restart", copyText("acct.sw.restart"), false));
  sec.content.appendChild(modes);
  if (read)
    sec.content.appendChild(
      el(
        "div",
        s.acctHint,
        copyText("acct.sw.hotHint", { name: accountLabel(read.account.start) }),
      ),
    );
  sec.content.appendChild(
    el("div", s.acctHint, copyText("acct.sw.restartHint")),
  );
  if (!pick || pick === cur) setDisabled(go, copyText("acct.sw.optNone"));
  if (o.switchError) sec.content.appendChild(banner("error", o.switchError));
  return sec.root;
}

async function doSwitch(
  o: Open,
  host: AcctPanelHost,
  target: string | null,
  cur: string | null,
  read: Present | undefined,
  go: HTMLButtonElement,
): Promise<void> {
  if (!target) return;
  const agent = read?.agent ?? routeOf(host, o) ?? "";
  const hot = (read?.account.inPlace ?? "noRelay") === "ok" && o.mode === "hot";
  setDisabled(go, copyText("acct.sw.busy"));
  try {
    if (hot) {
      const got = (await switchHot(o.origin, [o.sid], target))[o.sid];
      if (!got || got.state !== "done") {
        o.switchError = copyText("acct.sw.failHot", {
          cur: accountLabel(cur ?? "_"),
          reason: reasonLabel(got ? got.code : "", { agent, target }),
        });
      } else {
        o.switchError = null;
        o.pick = null;
        toast(copyText("acct.sw.doneHot", { name: accountLabel(target) }), "", {
          level: "info",
        });
      }
      return;
    }
    await restartSwitch(o, host, target);
  } catch (e) {
    console.warn("[acct] rotation-switch 失败：", e);
    const reason = copyText("acct.reason.unknown");
    o.switchError = hot
      ? copyText("acct.sw.failHot", { cur: accountLabel(cur ?? "_"), reason })
      : copyText("acct.sw.failRestart", { reason });
  } finally {
    setDisabled(go, null);
    await refreshSessions(o.origin, [o.sid]);
    if (open === o) render(o, host);
  }
}

/** 等那台定位 · 停旧 ＋ 起新那几步的底数（等报出之外）。 */
const RESTART_BASE_MS = 60_000;

/** 重启切换：先问那台会打断什么（有才问）、再交那台 `rotation-switch`（重启那一形，逐个交给 `session-restart`）、成了开终端接上。 */
async function restartSwitch(
  o: Open,
  host: AcctPanelHost,
  target: string,
): Promise<void> {
  // 用新号重起要知道是哪一家（会话事实给的）；还不知道 ⇒ 不起、说清，不落哪一家。
  const kind = host.agentOf(o.sid);
  if (kind === null) {
    o.switchError = copyText("tabSessionActions.agent.unknown");
    return;
  }
  const standing = await standingOf(o.origin, o.sid);
  if (standing?.kind !== "running") {
    o.switchError = copyText("acct.sw.failRestart", {
      reason: reasonLabel("not_in_terminal", {
        agent: agentName(host, o),
        target,
      }),
    });
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
    ...(await startSettings(o.origin, kind)),
  };
  const got = (
    await switchRestart(
      o.origin,
      [args],
      target,
      ARRIVAL_BUDGET_MS + RESTART_BASE_MS,
    )
  )[o.sid];
  o.switchError = null;
  if (!got || got.state !== "done") {
    const reason = reasonLabel(got ? got.code : "", {
      agent: agentName(host, o),
      target,
    });
    const log = {
      label: copyText("acct.sw.log"),
      run: () =>
        void commands
          .open_log_file()
          .catch((e) => console.warn("open_log_file failed:", e)),
    };
    if (got?.old === "ended") {
      toast(
        copyText("acct.sw.failRestartEnded", {
          ended: copyText("sessionState.ended.name"),
          reason,
        }),
        "",
        {
          level: "error",
          action: [
            {
              label: copyText("acct.sw.resume"),
              run: () => host.openResume(o.sid),
            },
            log,
          ],
        },
      );
    } else {
      toast(copyText("acct.sw.failRestart", { reason }), "", {
        level: "error",
        action: [log],
      });
    }
    return;
  }
  o.pick = null;
  toast(
    copyText("acct.sw.doneRestart", {
      name: accountLabel(target),
      session: host.sessionTitle(o.sid),
    }),
    "",
    { level: "info" },
  );
  await runRemoteAttach(o.origin, kind, got.terminal, { quiet: true });
}

// ─────────────────────────────── 记录 · 底栏

const HISTORY_SHOWN = 5;

function historyBlock(read: Present): HTMLElement {
  const sec = section(copyText("acct.hist.title"));
  const line = (at: string, parts: HTMLElement[]): HTMLElement => {
    const r = el("div", s.acctHistRow);
    r.append(el("span", s.acctHistAt, at), ...parts);
    return r;
  };
  const rows = [...read.account.history].reverse().map((h) => {
    const w = whyOf(h);
    const move = el("span", s.acctHistMove);
    move.append(
      acctAvatar(h.from),
      document.createTextNode(accountLabel(h.from)),
    );
    if (h.to !== h.from)
      move.append(
        el("span", s.acctHistArrow, copyText("acct.strip.arrow")),
        acctAvatar(h.to),
        document.createTextNode(accountLabel(h.to)),
      );
    const why = el("span", s.acctHistWhy, w.why);
    if (w.reset) why.appendChild(el("span", s.acctHistReset, w.reset));
    return line(h.atText ?? "", [move, why]);
  });
  const startRow = el(
    "span",
    s.acctHistMove,
    copyText("acct.hist.start", { name: accountLabel(read.account.start) }),
  );
  rows.push(
    line(
      read.account.history.length === 0 ? (read.account.sinceText ?? "") : "",
      [startRow],
    ),
  );
  sec.content.append(...rows.slice(0, HISTORY_SHOWN));
  if (rows.length > HISTORY_SHOWN) {
    const more = el("div", s.acctHistMore);
    more.append(...rows.slice(HISTORY_SHOWN));
    sec.content.appendChild(
      fold({
        title: copyText("acct.hist.more", { n: rows.length - HISTORY_SHOWN }),
        open: false,
        body: more,
      }),
    );
  }
  return sec.root;
}

function footer(o: Open, host: AcctPanelHost): HTMLElement {
  const f = el("div", s.acctFoot);
  f.appendChild(el("span", s.acctFootLabel, copyText("acct.foot.default")));
  const def = host.defaultOf(o.origin);
  const b = el("button", s.acctPick);
  b.type = "button";
  if (def)
    b.append(acctAvatar(def), el("span", s.acctPickName, accountLabel(def)));
  b.appendChild(icon("caretDown", "compact"));
  b.addEventListener("click", () => {
    closeMenu();
    host.openDefaultMenu(b, o.origin);
  });
  f.append(
    b,
    el("span", s.acctSpacer),
    button({
      label: copyText("acct.foot.manage"),
      kind: "ghost",
      size: "compact",
      onClick: () => host.openSettings(o.origin),
    }),
  );
  return f;
}
