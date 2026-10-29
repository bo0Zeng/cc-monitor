/**
 * 「账号」面板（右侧抽屉）：状态栏账号按钮 · 右键「账号…」· 会话头 ⋯ 三处开的同一个。
 * 头下一句（中转没见过这个会话 · 额度账读不出时的那一条）＋ 底栏 `新会话默认 [work ▾] … 管理账号…`。
 * 中间是这个会话的轮换（提示 · 策略一行 · 顺序，`rot-session.ts`；与设置窗「轮换」栏只共用策略一行与提示）。
 * 没有当前会话（`sid` 为 `null`）⇒ 同一个面板只有底栏那半段（状态栏按钮只开这一个，不另给下拉）。
 *
 * - 数据只经 `quota-read` · `rotation-view` 与推送（`acct-center.ts` 收进 `appStore`）。界面不判「能不能换 / 该不该换」。
 */
import { appStore, quotaOf, type QuotaSlot, type SessionRotationEntry } from "./app-store";
import { refreshQuota, refreshSessions } from "./acct-center";
import { acctAvatar } from "./acct-dom";
import { machineLabel } from "./acct-view";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { el } from "./kit/el";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { openDrawer, type DrawerHandle } from "./kit/drawer";
import { icon } from "./kit/icon";
import { closeMenu } from "./kit/menu";
import { mountSessionRotation, type SessionRot } from "./rot-session";
import { accountLabel } from "./acct-words";
import s from "./acct.module.css";

/** 主窗口交进来的几样（都是回调，面板不认 tab 栏）。 */
export interface AcctPanelHost {
  /** 会话的标题（面板头 `orders · 本机`）。 */
  sessionTitle(sid: string): string;
  /** 设置窗那台的账号页（`管理账号…` · `接入…`）。 */
  openSettings(origin: Origin): void;
  /** 底栏「新会话默认」那一颗：开那台的默认账号下拉（与无会话时状态栏按钮的同一个）。 */
  openDefaultMenu(anchor: HTMLElement, origin: Origin): void;
  /** 新会话默认此刻是哪个号（底栏那颗按钮上的字）。 */
  defaultOf(origin: Origin): string | null;
}

interface Open {
  /** `null` ＝ 没有当前会话：只有底栏。 */
  sid: string | null;
  origin: Origin;
  drawer: DrawerHandle;
  body: HTMLElement;
  rot: SessionRot | null;
  unsub: (() => void)[];
}

let open: Open | null = null;

/** 开 / 关这个会话的面板（再点同一个入口 ＝ 关）；`sid` 为 `null` ＝ 没有当前会话那一形（`origin` 那台的新会话默认）。 */
export function toggleAccountPanel(sid: string | null, origin: Origin, host: AcctPanelHost): void {
  if (open !== null && open.sid === sid) {
    void open.drawer.close();
    return;
  }
  openAccountPanel(sid, origin, host);
}

/** 当前 tab 换了：面板开着就跟着换成那个会话（不关）；没有会话了 ⇒ 换成本机那一形。 */
export function followActive(a: { sid: string; origin: Origin } | null, host: AcctPanelHost, idle: Origin): void {
  if (!open || open.sid === (a?.sid ?? null)) return;
  if (a === null) openAccountPanel(null, idle, host);
  else openAccountPanel(a.sid, a.origin, host);
}

export function openAccountPanel(sid: string | null, origin: Origin, host: AcctPanelHost): void {
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
    sub: sid === null ? machineLabel(origin) : copyText("acct.panel.sub", { session: host.sessionTitle(sid), machine: machineLabel(origin) }),
    body,
    width: 540,
    bottom: statusBar ? Math.round(statusBar.getBoundingClientRect().height) : undefined,
    onClose: () => {
      if (open?.drawer !== drawer) return;
      for (const u of open.unsub) u();
      open = null;
    },
  });
  const o: Open = { sid, origin, drawer, body, rot: null, unsub: [] };
  open = o;
  const repaint = (): void => {
    if (open === o) render(o, host);
  };
  if (sid === null) {
    o.unsub.push(appStore.accounts.subscribe(repaint));
    render(o, host);
    return;
  }
  const rot = mountSessionRotation(origin, sid);
  o.rot = rot;
  // 这个会话的轮换变了（推送 ⇒ `acct-center` 换了它那一格）⇒ 重读（拖着一行时先记下，静止了再读）。
  let lastEntry = appStore.sessionRotation.get().get(sid);
  const onRotation = (): void => {
    const e = appStore.sessionRotation.get().get(sid);
    if (e === lastEntry) return;
    lastEntry = e;
    repaint();
    if (open === o) void rot.refresh();
  };
  o.unsub.push(
    appStore.quota.subscribe(repaint),
    appStore.sessionRotation.subscribe(onRotation),
    appStore.accounts.subscribe(repaint),
    appStore.sessionAccount.subscribe(repaint),
  );
  render(o, host);
  void rot.refresh();
  // 打开那一刻再问一次这个会话（推送丢过也能补上）；额度账同理。
  void refreshSessions(origin, [sid]);
  void refreshQuota(origin);
}

function render(o: Open, host: AcctPanelHost): void {
  if (o.sid === null || o.rot === null) {
    o.body.replaceChildren(footer(o, host));
    return;
  }
  // 上游是自设的（那台在 `session-account` 里写好那一句）⇒ 额度看不到、换号不适用：只出那一条，不画轮换。
  const own = appStore.sessionAccount.get().get(o.sid)?.read.ownUpstream;
  if (own) {
    o.body.replaceChildren(banner("info", own), footer(o, host));
    return;
  }
  const entry = appStore.sessionRotation.get().get(o.sid);
  const head = stub(o, entry, quotaOf(o.origin), host);
  // 中转没见过这个会话 ⇒ 轮换那几块没得画（头下那一条说清楚了）。
  o.body.replaceChildren(...head, ...(entry !== undefined && entry.read === null ? [] : [o.rot.root]), footer(o, host));
}

/**
 * 头下那一条：中转没见过这个会话 · 额度账还没问回来（读取中）· 整问失败（出错那一层写好的那一句 ＋ 重试）·
 * 那台读不出（后端写好的那一句）。读取中 · 重试是面板自己的骨架字，不随那份账变。
 */
function stub(o: Open, entry: SessionRotationEntry | undefined, quota: QuotaSlot, host: AcctPanelHost): HTMLElement[] {
  const out: HTMLElement[] = [];
  if (entry !== undefined && entry.read === null) {
    const connect = button({ label: copyText("acct.now.connect"), kind: "ghost", size: "compact", onClick: () => host.openSettings(o.origin) });
    out.push(banner("warn", copyText("acct.now.noRelay"), [connect]));
    return out;
  }
  const reading = copyText("kit.state.reading");
  const retryLabel = copyText("acct.now.retry");
  if (quota.state === "pending") {
    out.push(banner("info", reading));
  } else if (quota.state === "failed") {
    const retry = button({ label: retryLabel, kind: "ghost", size: "compact", onClick: () => void refreshQuota(o.origin) });
    out.push(banner("error", quota.said, [retry], quota.detail));
  } else if (quota.state === "unreadable" && quota.text !== null) {
    // 那一句后端写好（`quota_rows::head_text`：读不出恒有），面板照抄。
    out.push(banner("warn", quota.text, [], quota.detail ?? ""));
  }
  return out;
}

// ─────────────────────────────── 底栏

function footer(o: Open, host: AcctPanelHost): HTMLElement {
  const f = el("div", s.acctFoot);
  f.appendChild(el("span", s.acctFootLabel, copyText("acct.foot.default")));
  const def = host.defaultOf(o.origin);
  const b = el("button", s.acctPick);
  b.type = "button";
  if (def)
    b.append(acctAvatar(def, 16, o.origin), el("span", s.acctPickName, accountLabel(def)));
  b.appendChild(icon("caretDown", "compact"));
  b.addEventListener("click", () => {
    closeMenu();
    host.openDefaultMenu(b, o.origin);
  });
  f.append(
    b,
    el("span", s.acctSpacer),
    button({
      label: copyText("acct.menu.manage"),
      kind: "ghost",
      size: "compact",
      onClick: () => host.openSettings(o.origin),
    }),
  );
  return f;
}
