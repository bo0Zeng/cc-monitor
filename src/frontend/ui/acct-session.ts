/**
 * 会话里的两样（轮换第五版）：**换号条**（消息流里换号那一刻一行居中的 `⇄ work → team · 权重最高  账号…`）与
 * **停发提示**（会话停发时，会话头下、消息流上：`已停发 · 一个号都用不了 · 18:30 team 恢复后接着发（还有 4 小时）` ［账号…］）。
 *
 * - 都只照这个会话的 `rotation-view`（`appStore.sessionRotation`）画：换号记录 `log`（为什么那一句 `because` 是后端配好的：策略写的，没写就照后端看得见的原因）·
 *   提示 `hints` 里的停发（字是后端写好的，与面板同一处 `hintText` 填「还有多久」）。
 * - 换号条按到达那一刻追加在消息流内容层末尾；打开窗口之前就有的换号不回填（那一刻在流里的位置说不准）。
 * 只在主窗口接（带一份 CSS Module，理由同 `record-file-notice.ts`）。判据：`tests/frontend/ui/acct-session.vitest.ts`。
 */
import { appStore, type SessionRotationEntry } from "./app-store";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import { accountLabel } from "./acct-words";
import { holdOf } from "./rot-words";
import s from "./acct-session.module.css";

export interface AcctSessionHost {
  streamContentOf(sid: string): HTMLElement | null;
  openPanel(sid: string, origin: Origin): void;
}

/** 每个会话见过的最新一条换号记录的时刻（第一次见到的那一份只记下，不回填）。 */
const seenLog = new Map<string, number>();

function span(cls: (e: HTMLSpanElement) => void, text: string): HTMLSpanElement {
  const e = document.createElement("span");
  cls(e);
  e.textContent = text;
  return e;
}

/** 一条换号记录 ⇒ 换号条。 */
export function stripOf(
  sid: string,
  entry: SessionRotationEntry,
  index: number,
  host: AcctSessionHost,
): HTMLElement | null {
  const h = entry.read?.log[index];
  if (!h || h.from === h.to) return null;
  const root = document.createElement("div");
  root.className = s.acctSessStrip;
  root.dataset.acctStrip = String(h.at);
  const text = document.createElement("span");
  text.className = s.acctSessStripText;
  text.append(
    icon("swap", "compact"),
    span(
      (e) => (e.className = s.acctSessStripMove),
      copyText("rot.strip.move", {
        from: accountLabel(h.from),
        to: accountLabel(h.to),
      }),
    ),
    span((e) => (e.className = s.acctSessStripSep), copyText("kit.text.sep")),
    span((e) => (e.className = s.acctSessStripWhy), h.because),
  );
  const open = button({
    label: copyText("rot.strip.open"),
    kind: "ghost",
    size: "compact",
    onClick: () => host.openPanel(sid, entry.origin),
  });
  root.append(
    span((e) => (e.className = s.acctSessStripLine), ""),
    text,
    open,
    span((e) => (e.className = s.acctSessStripLine), ""),
  );
  return root;
}

/** 停发提示：有就画 / 换字，没有就摘。 */
function paintNotice(
  sid: string,
  entry: SessionRotationEntry | undefined,
  host: AcctSessionHost,
): void {
  const content = host.streamContentOf(sid);
  if (!content) return;
  let el = content.querySelector<HTMLElement>(":scope > [data-acct-notice]");
  const text = holdOf(entry?.read);
  if (text === null || !entry) {
    el?.remove();
    return;
  }
  if (!el) {
    el = document.createElement("div");
    el.className = s.acctSessNotice;
    el.dataset.acctNotice = "";
    content.prepend(el);
  }
  el.replaceChildren(
    banner("warn", text, [
      button({
        label: copyText("rot.strip.open"),
        kind: "secondary",
        size: "compact",
        onClick: () => host.openPanel(sid, entry.origin),
      }),
    ]),
  );
}

/** 一个会话的轮换格到了 / 变了 ⇒ 新来的换号记录各追加一条换号条 ＋ 停发提示照此刻重画。 */
export function onSessionEntry(
  sid: string,
  entry: SessionRotationEntry,
  host: AcctSessionHost,
): void {
  paintNotice(sid, entry, host);
  const log = entry.read?.log;
  if (!log) return;
  // 按时刻认新来的（记录至多留 32 条，满了以后条数不再涨）。
  const newest = log.reduce((m, h) => Math.max(m, h.at), 0);
  const had = seenLog.get(sid);
  seenLog.set(sid, Math.max(had ?? 0, newest));
  if (had === undefined) return;
  const content = host.streamContentOf(sid);
  if (!content) return;
  log.forEach((h, i) => {
    if (h.at <= had) return;
    const strip = stripOf(sid, entry, i, host);
    if (strip) content.appendChild(strip);
  });
}

/** 主窗口接线：订会话轮换那一格；只处理变了的那几个会话。 */
export function acctSessionWiring(host: AcctSessionHost): {
  repaintNotice(sid: string): void;
} {
  let last: ReadonlyMap<string, SessionRotationEntry> = new Map();
  appStore.sessionRotation.subscribe((next) => {
    for (const [sid, e] of next)
      if (last.get(sid) !== e) onSessionEntry(sid, e, host);
    last = next;
  });
  return {
    repaintNotice: (sid) =>
      paintNotice(sid, appStore.sessionRotation.get().get(sid), host),
  };
}

/** 测试：清掉见过的记录数。 */
export function __resetSeenLogForTests(): void {
  seenLog.clear();
}
