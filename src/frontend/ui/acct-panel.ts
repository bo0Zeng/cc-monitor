/**
 * 「账号」面板（右侧抽屉）：状态栏账号按钮 · 右键「账号…」· 会话头 ⋯ 三处开的同一个。
 * 头下一句 `起会话的号 work · 能热换` ＋ 轮换（`rot-session.ts`：打开的配置 · 描述 · 信息条 · `参数…` `刷新策略` `切换…` · 合图列表；
 * 号详情 · 参数… 两页）＋ 底栏 `新会话默认 [work ▾] … 管理账号…`。`切换…` 是一张浮层（选号 · 热切换 / 重启切换）。
 *
 * - 数据只经 `quota-read` · `rotation-configs-read` · `rotation-strategies-read` · `rotation-session-*` · `rotation-plan` · `rotation-switch`
 *   与推送（`acct-center.ts` 收进 `appStore`）。界面不判「能不能换 / 该不该换」，不跑策略。
 * - 实时：数据变了原地重画；正在拖时不重画（松手再画）。
 */
import { lookupAgentProfile, displayNameOf } from "./agent-profile";
import { appStore, type SessionRotationEntry } from "./app-store";
import { refreshConfigs, refreshQuota, refreshSessions, strategiesOf } from "./acct-center";
import { acctAvatar } from "./acct-dom";
import { ledgerOf, machineLabel, reasonLabel } from "./acct-view";
import { copyText } from "./copy-table";
import { askSessionInterrupts } from "./interrupt-reads";
import { commands } from "./ipc/commands";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button, setDisabled } from "./kit/button";
import { openDrawer, type DrawerHandle } from "./kit/drawer";
import { icon } from "./kit/icon";
import { confirmInterrupts } from "./kit/interrupts";
import { closeMenu, openMenu, type MenuItem } from "./kit/menu";
import { meter, type MeterState } from "./kit/meter";
import { closePopover, openPopover, swapPopoverContent } from "./kit/popover";
import { toast } from "./kit/toast";
import { ARRIVAL_BUDGET_MS } from "./launch-arrival";
import { accountLabel, slotLabel, slotText, type QuotaRead } from "./acct-words";
import { switchHot, switchRestart } from "./quota-reads";
import { runRemoteAttach } from "./remote-launch-run";
import { detailView, newRotState, paramsView, rotationList, type Present, type RotCtx, type RotState } from "./rot-session";
import { standingOf } from "./sessions-where";
import { startSettings } from "./tab-batch-run";
import type { QuotaShow } from "./generated/QuotaShow";
import s from "./acct.module.css";

/** 主窗口交进来的几样（都是回调，面板不认 tab 栏）。 */
export interface AcctPanelHost {
  /** 会话的标题（面板头 `orders · 本机`）。 */
  sessionTitle(sid: string): string;
  /** 会话的项目目录（重启切换要交那台）。 */
  cwdOf(sid: string): string;
  /** 设置窗那台的账号页（`管理账号…` · `设置…` · `接入…`）。 */
  openSettings(origin: Origin): void;
  /** 设置窗那台的「轮换」栏（`管理配置…` ⇒ 配置页那一份 · `看说明与源码 ›` ⇒ 策略页那一份）。 */
  openRotation(origin: Origin, at?: { config: string } | { strategy: string }): void;
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

interface Open {
  sid: string;
  origin: Origin;
  drawer: DrawerHandle;
  body: HTMLElement;
  /** 切换那一块选的号（`null` ＝ 按后端给的下一个）。 */
  pick: string | null;
  mode: "hot" | "restart";
  /** `切换…` 浮层的错误条（没成的那一句）。 */
  switchError: string | null;
  /** 轮换那一块：哪一页 · 选中 · 多选那一条 · 合图。 */
  rot: RotState;
  /** 正在拖：数据到了先不重画。 */
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

/** 命令面板「套用轮换配置…」：开（或留着）这个会话的面板，回到列表那一页、直接摊开配置下拉（与面板里同一份项）。 */
export function openConfigPicker(
  sid: string,
  origin: Origin,
  host: AcctPanelHost,
): void {
  if (open?.sid !== sid) openAccountPanel(sid, origin, host);
  if (!open) return;
  if (open.rot.view.k !== "list") {
    open.rot.view = { k: "list" };
    render(open, host);
  }
  open.body.querySelector<HTMLElement>("[data-rot-cfg]")?.click();
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
    rot: newRotState(),
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
    appStore.rotationConfigs.subscribe(repaint),
    appStore.rotationStrategies.subscribe(repaint),
    appStore.accounts.subscribe(repaint),
  );
  render(o, host);
  // 焦点落在配置下拉（合着时不写任何东西）；没有那一块（中转没见过）⇒ 留在关闭钮上。
  body.querySelector<HTMLElement>("[data-rot-cfg]")?.focus();
  // 打开那一刻再问一次这个会话（推送丢过也能补上）；额度账 · 配置表同理。
  void refreshSessions(origin, [sid]);
  void refreshQuota(origin);
  void refreshConfigs(origin);
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
  const focusKey = (document.activeElement as HTMLElement | null)?.dataset?.rcRow;
  o.body.replaceChildren();
  const read = entry?.read;
  o.body.append(...stub(o, entry, quota, host));
  if (read?.state === "present" && entry) {
    const c = rotCtx(o, read, quota, host);
    const v = o.rot.view;
    const known = v.k !== "detail" || read.rows.some((r) => r.account === v.account);
    if (!known) o.rot.view = { k: "list" };
    const w = o.rot.view;
    o.body.append(w.k === "detail" ? detailView(c, w.account) : w.k === "params" ? paramsView(c) : rotationList(c));
  } else if (entry?.read.state === "absent" || !entry) {
    // 中转没见过：只剩 `切换…`（重启切换）。
    const line = el("div", s.acctBlock);
    const sw = button({ label: copyText("rot.main.switch"), kind: "secondary", size: "compact", onClick: () => openSwitch(o, host, sw) });
    sw.dataset.rotSwitch = "true";
    line.appendChild(sw);
    o.body.append(line);
  }
  o.body.append(footer(o, host));
  if (o.body.parentElement) o.body.parentElement.scrollTop = scroll;
  if (focusKey) o.body.querySelector<HTMLElement>(`[data-rc-row="${CSS.escape(focusKey)}"]`)?.focus();
}

/** 轮换那一块要的几样（`rot-session.ts`）。 */
function rotCtx(o: Open, read: Present, quota: QuotaRead | null, host: AcctPanelHost): RotCtx {
  return {
    sid: o.sid,
    origin: o.origin,
    read,
    configs: appStore.rotationConfigs.get().get(o.origin) ?? null,
    strategies: strategiesOf(o.origin),
    quotaOf: (a) => quotaOf(quota, read.agent, a),
    meters: (q) => [slotMeter(q, "5h"), slotMeter(q, "7d")],
    library: () => [...(quota?.accounts ?? []), ...(quota?.unseen ?? [])].filter((x) => x.agent === read.agent && x.account !== "_").map((x) => x.account),
    st: o.rot,
    render: () => {
      if (open === o) render(o, host);
    },
    reload: async () => {
      await refreshSessions(o.origin, [o.sid]);
      if (open === o) render(o, host);
    },
    hold: (on) => (on ? (o.hold = true) : release(o, host)),
    sessionTitle: host.sessionTitle(o.sid),
    openSwitch: (anchor) => openSwitch(o, host, anchor),
    openRotation: (at) => host.openRotation(o.origin, at),
  };
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

/** 头下那一句：`起会话的号 work · 能热换`（不能热换 ⇒ 为什么）＋ 中转没见过 / 额度账读不出时的那一条。 */
function stub(o: Open, entry: SessionRotationEntry | undefined, quota: QuotaRead | null, host: AcctPanelHost): HTMLElement[] {
  const read = entry?.read;
  const out: HTMLElement[] = [];
  if (!entry || !read || read.state === "absent") {
    const connect = button({ label: copyText("acct.now.connect"), kind: "ghost", size: "compact", onClick: () => host.openSettings(o.origin) });
    out.push(banner("warn", copyText("acct.now.noRelay"), [connect]));
    return out;
  }
  const inPlace = read.inPlace === "ok" ? copyText("rot.panel.hotOk") : reasonLabel(read.inPlace, { agent: agentName(host, o), target: read.current });
  const line = el("div", s.acctStub, copyText("rot.panel.stub", { acct: accountLabel(read.start), inPlace }));
  line.dataset.acctStub = "true";
  out.push(line);
  if (quota === null) {
    const retry = button({ label: copyText("acct.now.retry"), kind: "ghost", size: "compact", onClick: () => void refreshQuota(o.origin) });
    out.push(banner("error", copyText("acct.now.readFail"), [retry]));
  } else if (quota.state === "unreadable") {
    out.push(banner("warn", quota.text ?? quota.reason ?? copyText("acct.now.readFail"), [], quota.detail ?? ""));
  }
  return out;
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
    ? { kind: u.kind, state: "unseen", stale: false, slots: [], login: u.login, usage: u.usage }
    : null;
}

// ─────────────────────────────── 切换

/** `切换…`：一张浮层（选号 · 热切换 / 重启切换 · 两句说明）；成了收起。 */
function openSwitch(o: Open, host: AcctPanelHost, anchor: HTMLElement): void {
  const paint = (): HTMLElement => {
    const entry = appStore.sessionRotation.get().get(o.sid);
    const read = entry?.read.state === "present" ? entry.read : undefined;
    return switchBlock(o, entry, read, appStore.quota.get().get(o.origin) ?? null, host, () => swapPopoverContent(anchor, paint()));
  };
  openPopover(anchor, paint(), { label: copyText("acct.sw.title"), align: "end" });
}

function switchBlock(
  o: Open,
  entry: SessionRotationEntry | undefined,
  read: Present | undefined,
  quota: QuotaRead | null,
  host: AcctPanelHost,
  repaint: () => void,
): HTMLElement {
  const sec = section(copyText("acct.sw.title"));
  // 中转没见过这个会话 ⇒ 不知道它的路由名，取这台额度账里那一家（今天只有一家）。
  const agent =
    read?.agent ??
    routeOf(host, o) ??
    quota?.accounts[0]?.agent ??
    quota?.unseen[0]?.agent ??
    "";
  const cur = read?.current ?? null;
  const inPlace = read
    ? read.inPlace
    : entry?.read.state === "absent"
      ? entry.read.inPlace
      : "noRelay";
  const candidates = [...(quota?.accounts ?? []), ...(quota?.unseen ?? [])]
    .filter((x) => x.agent === agent && x.account !== "_")
    .map((x) => x.account);
  // 缺省选有效优先级里在用的号之后、此刻挑得上的第一个（后端排好的行序）；中转没见过 ⇒ 此刻发得出去的头一个（`usableNow`，后端判的）。
  const pick =
    o.pick ??
    read?.rows.find((r) => r.account !== cur && r.skip !== true)?.account ??
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
      pickBtn.appendChild(el("span", s.acctPickUsage, q.usage.text));
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
      const u = q?.usage;
      const usage = u?.text;
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
        detailTone: u && u.tone !== "plain" ? "warn" : undefined,
        enabled: !blocked,
        onClick: () => {
          o.pick = a;
          o.switchError = null;
          repaint();
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
    onClick: () => void doSwitch(o, host, pick, cur, read, go, repaint),
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
      repaint();
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
        copyText("acct.sw.hotHint", { name: accountLabel(read.start) }),
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
  repaint: () => void,
): Promise<void> {
  if (!target) return;
  const agent = read?.agent ?? routeOf(host, o) ?? "";
  const hot = (read?.inPlace ?? "noRelay") === "ok" && o.mode === "hot";
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
        closePopover();
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
    if (open === o) {
      render(o, host);
      if (go.isConnected) repaint();
    }
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

// ─────────────────────────────── 底栏

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
