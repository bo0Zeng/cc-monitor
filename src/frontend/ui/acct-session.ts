/**
 * 会话里的两样：**换号条**（消息流里换号那一刻一行居中的 `⇄ 14:20  work → personal · work 5h ✕ ↻19:00 · 账号…`）与
 * **提示条**（本会话发不出去时，会话头下、消息流上：`轮换内账号均已满 · 最早 team ↻18:10 (+1h30m)` ［账号…］）。
 *
 * - 都只按后端给的事实画：换号记录（`account.history`）· 卡住（`blocked`）· 可用的按量号（`fallbackApi`）。
 * - 换号条按发生那一刻追加在消息流内容层的末尾（之后来的记录排在它后面）；打开窗口之前就有的换号只在面板「记录」里，不回填进流
 *   （那一刻在流里的位置说不准）。
 * - 「还有多久」只在画的那一刻算（数据变了 / 切到这个 tab），不起定时器。
 * 只在主窗口接（带一份 CSS Module，理由同 `record-file-notice.ts`）。
 */
import { appStore, type SessionRotationEntry } from "./app-store";
import { refreshSessions } from "./acct-center";
import { bannerOf, whyOf } from "./acct-view";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import { accountLabel } from "./acct-words";
import { switchHot } from "./quota-reads";
import s from "./acct-session.module.css";

export interface AcctSessionHost {
  streamContentOf(sid: string): HTMLElement | null;
  openPanel(sid: string, origin: Origin): void;
}

/** 每个会话已经画过 / 见过多少条换号记录（第一次见到的那一份只记数，不回填）。 */
const seenHistory = new Map<string, number>();

function span(cls: string, text: string): HTMLSpanElement {
  const e = document.createElement("span");
  e.className = cls;
  e.textContent = text;
  return e;
}

/** 一条换号记录 ⇒ 换号条。 */
export function stripOf(sid: string, entry: SessionRotationEntry, index: number, host: AcctSessionHost): HTMLElement | null {
  if (entry.read.state !== "present") return null;
  const h = entry.read.account.history[index];
  if (!h || h.from === h.to) return null;
  const root = document.createElement("div");
  root.className = s.acctSessStrip;
  root.dataset.acctStrip = String(h.at);
  const why = whyOf(h);
  const quota = appStore.quota.get().get(entry.origin) ?? null;
  const toApi = quota?.accounts.find((a) => a.account === h.to)?.kind === "api";
  const to = toApi ? copyText("acct.strip.api", { to: accountLabel(h.to) }) : accountLabel(h.to);
  const text = document.createElement("span");
  text.className = s.acctSessStripText;
  const swap = icon("swap", "compact");
  text.append(
    swap,
    span(s.acctSessStripAt, h.atText ?? ""),
    span(s.acctSessStripMove, copyText("acct.strip.move", { from: accountLabel(h.from), to })),
    span(s.acctSessStripSep, copyText("kit.text.sep")),
    span(s.acctSessStripWhy, why.why),
  );
  if (why.reset) text.appendChild(span(s.acctSessStripWhy, why.reset));
  const open = button({ label: copyText("acct.strip.open"), kind: "ghost", size: "compact", onClick: () => host.openPanel(sid, entry.origin) });
  root.append(span(s.acctSessStripLine, ""), text, open, span(s.acctSessStripLine, ""));
  return root;
}

/** 提示条：有就画 / 换字，没有就摘。 */
function paintBanner(sid: string, entry: SessionRotationEntry | undefined, host: AcctSessionHost): void {
  // 放在消息流内容那一层（与消息同宽、居中）的最前面，贴顶不随滚动走；它不是记录，不进时间序。
  const content = host.streamContentOf(sid);
  if (!content) return;
  let el = content.querySelector<HTMLElement>(`:scope > .${s.acctSessNotice}`);
  const b = bannerOf(entry);
  if (!b || !entry) {
    el?.remove();
    return;
  }
  if (!el) {
    el = document.createElement("div");
    el.className = s.acctSessNotice;
    content.prepend(el);
  }
  const actions: HTMLElement[] = [button({ label: copyText("acct.strip.open"), kind: "secondary", size: "compact", onClick: () => host.openPanel(sid, entry.origin) })];
  const api = entry.read.state === "present" ? entry.read.fallbackApi : undefined;
  if (api !== undefined && b.tone === "warn") {
    actions.push(
      button({
        label: copyText("acct.banner.toApi", { name: accountLabel(api) }),
        kind: "secondary",
        size: "compact",
        onClick: () => void switchHot(entry.origin, [sid], api).finally(() => void refreshSessions(entry.origin, [sid])),
      }),
    );
  }
  el.replaceChildren(banner("warn", b.text, actions));
}

/** 一个会话的轮换格到了 / 变了 ⇒ 新出现的换号记录各追加一条换号条 ＋ 提示条按此刻重画。 */
export function onSessionEntry(sid: string, entry: SessionRotationEntry, host: AcctSessionHost): void {
  paintBanner(sid, entry, host);
  if (entry.read.state !== "present") return;
  const n = entry.read.account.history.length;
  const had = seenHistory.get(sid);
  seenHistory.set(sid, n);
  if (had === undefined || n <= had) return;
  const content = host.streamContentOf(sid);
  if (!content) return;
  for (let i = had; i < n; i++) {
    const strip = stripOf(sid, entry, i, host);
    if (strip) content.appendChild(strip);
  }
}

/** 主窗口接线：订会话轮换那一格；只处理变了的那几个会话。 */
export function acctSessionWiring(host: AcctSessionHost): { repaintBanner(sid: string): void } {
  let last: ReadonlyMap<string, SessionRotationEntry> = new Map();
  appStore.sessionRotation.subscribe((next) => {
    for (const [sid, e] of next) if (last.get(sid) !== e) onSessionEntry(sid, e, host);
    last = next;
  });
  return { repaintBanner: (sid) => paintBanner(sid, appStore.sessionRotation.get().get(sid), host) };
}
