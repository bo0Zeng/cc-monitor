/**
 * **标签页栏上的「隐藏」那几件**：帧上那一格落到 tab 上 · 右键 / 批量「隐藏」「取消隐藏」交的命令 · 撤销 · 隐藏了又结束的收走。
 *
 * 隐藏不隐藏只照后端那一格（`Tab.hidden`，会话宣告与推帧主题「会话隐藏」带来）：点了「隐藏」只交本机后端一条（`sessions-batch`，按台扇出 · 合那一句都在后端），
 * 照回包出那一条（成了的带撤销：撤销交的那几问也是回包写好的），栏上那一格等推帧来了才挪（界面不先挪、不判、不数）。
 * 挪进「已隐藏」时它的固定与分组一起摘掉（同关闭的摘法）；从这里点的隐藏被撤销 ⇒ 推帧说「不隐藏了」那一刻原位、原分组、原固定放回；
 * 别的取消隐藏 ⇒ 回到散的标签页末尾。零 DOM：toast 与命令经宿主（判据换替身）。
 */
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";
import type { TabBarPrefs } from "./tab-bar-prefs";
import { isEnded } from "./face-can";
import { sameHidden, type SessionHidden } from "./session-hidden";
import { copyText } from "./copy-table";
import { fullTitle } from "./session-face";
import type { SessionsBatchReply } from "./generated/SessionsBatchReply";
import type { SessionsOp } from "./generated/SessionsOp";
import type { SessionItem } from "./generated/SessionItem";

/** 隐藏那几件要宿主做的：交命令 · 出 toast · 重画 · 关掉一个已结束的。 */
export interface TabHideHost {
  /** 交本机后端一批（`sessions-batch`：`hide` · `unhide` · 撤销那几问），回合好的回包；问不到本机后端 ⇒ 抛。 */
  send(op: SessionsOp, items: readonly SessionItem[]): Promise<SessionsBatchReply>;
  /** 照回包出那一条（`end-confirm.ts::sayReply`）；`undo` 给了 ⇒ 同一条上［撤销］（`Ctrl Z` 同样）。 */
  say(reply: SessionsBatchReply, undo?: { run: () => void; settle: () => void }): void;
  /** 本机后端问不到那一句。 */
  failed(title: string, e: unknown): void;
  refreshTabBar(): void;
  /** 关掉一个已结束的（同 ×：拆流、让后端忘掉）。 */
  close(sid: string): void;
}

const itemOf = (t: Tab): SessionItem => ({ machine: t.origin, sid: t.sessionId });

/** 本机后端问不到时那一条的标题：一个 ⇒ 点名那一个；几个 ⇒ 那一项。`on` ＝ 交的是不是「隐藏」。 */
function missedTitle(on: boolean, tabs: readonly Tab[]): string {
  if (tabs.length > 1) return on ? copyText("tabBatch.menu.hide", { n: tabs.length }) : copyText("tabBatch.menu.unhide", { n: tabs.length });
  const title = fullTitle(tabs[0]);
  return on ? copyText("tabBar.hide.failed", { title }) : copyText("tabBar.unhide.failed", { title });
}

/** 隐藏之前那一刻（撤销时放回）：顺序里那一格 · 分组 · 固定。 */
interface Before {
  index: number;
  group: string | null;
  pinned: boolean;
}

export class TabHide {
  /** 从这里点了「隐藏」、撤销期还没过的那几个：推帧说不隐藏了 ⇒ 照这一份放回。 */
  private readonly before = new Map<string, Before>();

  constructor(
    private readonly store: TabStore,
    private readonly prefs: TabBarPrefs,
    private readonly host: TabHideHost,
  ) {}

  /**
   * 后端那一格到了（宣告 · 推帧）：换上，变了才动栏。挪进「已隐藏」⇒ 摘固定 · 不画在组里（先记下，撤销放回）；
   * 挪出 ⇒ 撤销期里的照原样放回，别的回到散的末尾。回「变没变」。
   */
  land(sid: string, hidden: SessionHidden | null): boolean {
    const tab = this.store.tabs.get(sid);
    if (!tab) return false;
    const was = tab.hidden ?? null;
    if (sameHidden(was, hidden)) return false;
    tab.hidden = hidden;
    if (was === null && hidden !== null) {
      if (tab.pinned) void this.prefs.setPinnedMany([sid], false);
      tab.group = null; // 隐藏的不画在组里（组员关系不动：那是那台的注解）
    } else if (was !== null && hidden === null) {
      const b = this.before.get(sid);
      this.before.delete(sid);
      const at = this.store.orderedIds.indexOf(sid);
      if (at >= 0) this.store.orderedIds.splice(at, 1);
      this.store.orderedIds.splice(b ? Math.min(b.index, this.store.orderedIds.length) : this.store.orderedIds.length, 0, sid);
      // 画回原来那一组：账上有 ⇒ 照账；没接账 ⇒ 照隐藏前记下的那一格。
      if (b?.group != null && this.prefs.collections.some((c) => c.id === b.group)) tab.group = b.group;
      this.prefs.adoptGroup(tab);
      if (b?.pinned) void this.prefs.setPinnedMany([sid], true);
      void this.prefs.persistOrder();
    }
    this.host.refreshTabBar();
    this.reap();
    return true;
  }

  /**
   * 右键 / 批量「隐藏」：交本机后端一条；照回包说（成了的那几个带撤销，撤销交回包写好的那几问；一个都没成 ⇒ 只那一句）。
   * 不弹确认；已经隐藏的不交。
   */
  hide(sids: readonly string[]): void {
    const tabs = sids.map((s) => this.store.tabs.get(s)).filter((t): t is Tab => t !== undefined && (t.hidden ?? null) === null);
    if (tabs.length === 0) return;
    for (const t of tabs) this.before.set(t.sessionId, { index: this.store.orderedIds.indexOf(t.sessionId), group: t.group, pinned: t.pinned });
    const forget = (ids: Iterable<string>): void => {
      for (const id of ids) this.before.delete(id);
    };
    void this.host.send("hide", tabs.map(itemOf)).then(
      (reply) => {
        const done = new Set(reply.results.filter((o) => o.outcome === "done").map((o) => o.sid));
        forget(tabs.map((t) => t.sessionId).filter((id) => !done.has(id)));
        if (reply.undo.length === 0) {
          this.host.say(reply);
          return;
        }
        const undo = {
          // 撤销回到隐藏之前那一份单独设置：后端照各自改之前那一格分好的那几问，原样交回。
          run: () => void Promise.all(reply.undo.map((q) => this.host.send(q.op, q.items))).then((rs) => rs.filter((r) => r.failed).forEach((r) => this.host.say(r)), (e: unknown) => this.host.failed(missedTitle(false, tabs), e)),
          settle: () => forget(done),
        };
        this.host.say(reply, undo);
      },
      (e: unknown) => {
        forget(tabs.map((t) => t.sessionId));
        this.host.failed(missedTitle(true, tabs), e);
      },
    );
  }

  /**
   * 「已隐藏」里的行 · 批量「取消隐藏」：交 `unhide`（单个那一格写 `off`：规则隐藏的一样，之后不再按那一条隐藏）。推帧到了才挪回栏上；
   * 都成了不另出声，有没成的照回包说。
   */
  unhide(sids: readonly string[]): void {
    const tabs = sids.map((s) => this.store.tabs.get(s)).filter((t): t is Tab => t !== undefined && (t.hidden ?? null) !== null);
    if (tabs.length === 0) return;
    void this.host.send("unhide", tabs.map(itemOf)).then(
      (reply) => {
        if (reply.failed) this.host.say(reply);
      },
      (e: unknown) => this.host.failed(missedTitle(false, tabs), e),
    );
  }

  /** 隐藏了又结束的：不留（「已隐藏」只列活的，要看去历史）；正看着的那一个切走再收。 */
  reap(): void {
    for (const t of [...this.store.tabs.values()]) {
      if ((t.hidden ?? null) !== null && isEnded(t.face) && t.sessionId !== this.store.activeId) this.host.close(t.sessionId);
    }
  }
}
