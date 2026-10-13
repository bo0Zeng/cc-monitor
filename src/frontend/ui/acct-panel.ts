/**
 * 「账号」面板（右侧抽屉）：状态栏账号按钮 · 右键「账号…」· 会话头 ⋯ 三处开的同一个。
 * 头下一句（中转没见过这个会话 · 额度账读不出时的那一条）＋ 底栏 `新会话默认 [work ▾] … 管理账号…`。
 * 轮换那一块（策略下拉 · 号牌 · 总体设置 · 模拟 · `现在用`）第 4 刀照 `轮换-v5` 的稿施工，此刻空着。
 *
 * - 数据只经 `quota-read` · `rotation-view` 与推送（`acct-center.ts` 收进 `appStore`）。界面不判「能不能换 / 该不该换」。
 */
import { appStore, type SessionRotationEntry } from "./app-store";
import { refreshQuota, refreshSessions } from "./acct-center";
import { acctAvatar } from "./acct-dom";
import { machineLabel } from "./acct-view";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { openDrawer, type DrawerHandle } from "./kit/drawer";
import { icon } from "./kit/icon";
import { closeMenu } from "./kit/menu";
import { accountLabel, type QuotaRead } from "./acct-words";
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
  sid: string;
  origin: Origin;
  drawer: DrawerHandle;
  body: HTMLElement;
  unsub: (() => void)[];
}

let open: Open | null = null;

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
  const o: Open = { sid, origin, drawer, body, unsub: [] };
  open = o;
  const repaint = (): void => {
    if (open === o) render(o, host);
  };
  o.unsub.push(appStore.quota.subscribe(repaint), appStore.sessionRotation.subscribe(repaint), appStore.accounts.subscribe(repaint));
  render(o, host);
  // 打开那一刻再问一次这个会话（推送丢过也能补上）；额度账同理。
  void refreshSessions(origin, [sid]);
  void refreshQuota(origin);
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function render(o: Open, host: AcctPanelHost): void {
  const entry = appStore.sessionRotation.get().get(o.sid);
  const quota = appStore.quota.get().get(o.origin) ?? null;
  o.body.replaceChildren(...stub(o, entry, quota, host), footer(o, host));
}

/** 头下那一条：中转没见过这个会话 · 额度账读不出。 */
function stub(o: Open, entry: SessionRotationEntry | undefined, quota: QuotaRead | null, host: AcctPanelHost): HTMLElement[] {
  const out: HTMLElement[] = [];
  if (entry !== undefined && entry.read === null) {
    const connect = button({ label: copyText("acct.now.connect"), kind: "ghost", size: "compact", onClick: () => host.openSettings(o.origin) });
    out.push(banner("warn", copyText("acct.now.noRelay"), [connect]));
    return out;
  }
  if (quota === null) {
    const retry = button({ label: copyText("acct.now.retry"), kind: "ghost", size: "compact", onClick: () => void refreshQuota(o.origin) });
    out.push(banner("error", copyText("acct.now.readFail"), [retry]));
  } else if (quota.state === "unreadable") {
    out.push(banner("warn", quota.text ?? quota.reason ?? copyText("acct.now.readFail"), [], quota.detail ?? ""));
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
      label: copyText("acct.foot.manage"),
      kind: "ghost",
      size: "compact",
      onClick: () => host.openSettings(o.origin),
    }),
  );
  return f;
}
