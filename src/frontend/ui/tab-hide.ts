/**
 * **标签页栏上的「隐藏」那几件**：帧上那一格落到 tab 上 · 右键 / 批量「隐藏」「取消隐藏」交的命令 · 撤销 · 隐藏了又结束的收走。
 *
 * 隐藏不隐藏只照后端那一格（`Tab.hidden`，会话宣告与推帧主题「会话隐藏」带来）：点了「隐藏」只交命令、给撤销 toast，
 * 栏上那一格等推帧来了才挪（界面不先挪、不判）。挪进「已隐藏」时它的固定与分组一起摘掉（同关闭的摘法）；
 * 从这里点的隐藏被撤销 ⇒ 推帧说「不隐藏了」那一刻原位、原分组、原固定放回；别的取消隐藏 ⇒ 回到散的标签页末尾。
 * 零 DOM：toast 与命令经宿主（判据换替身）。
 */
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";
import type { TabBarPrefs } from "./tab-bar-prefs";
import { isResumeOnly } from "./tab-session-state";
import { sameHidden, type HideSet, type SessionHidden } from "./session-hidden";
import { copyText } from "./copy-table";
import { fullTitle } from "./session-face";

/** 隐藏那几件要宿主做的：交命令 · 出 toast · 重画 · 关掉一个已结束的。 */
export interface TabHideHost {
  /** 改单个那一格（`history-annotate {hide}`）；失败 ⇒ 抛。 */
  setHide(tab: Tab, to: HideSet): Promise<void>;
  /** 可撤的 toast（撤销 ⇒ `undo`；过期 ⇒ `settle`）。 */
  undoToast(title: string, undo: () => void, settle: () => void): void;
  /** 没交成那一句。 */
  failed(title: string, e: unknown): void;
  refreshTabBar(): void;
  /** 关掉一个已结束的（同 ×：拆流、让后端忘掉）。 */
  close(sid: string): void;
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
   * 后端那一格到了（宣告 · 推帧）：换上，变了才动栏。挪进「已隐藏」⇒ 摘固定 · 出组（先记下，撤销放回）；
   * 挪出 ⇒ 撤销期里的照原样放回，别的回到散的末尾。回「变没变」。
   */
  land(sid: string, hidden: SessionHidden | null): boolean {
    const tab = this.store.tabs.get(sid);
    if (!tab) return false;
    const was = tab.hidden ?? null;
    if (sameHidden(was, hidden)) return false;
    tab.hidden = hidden;
    if (was === null && hidden !== null) {
      if (tab.pinned) this.prefs.setPinnedMany([sid], false);
      if (tab.group !== null) void this.prefs.forgetTab(tab);
    } else if (was !== null && hidden === null) {
      const b = this.before.get(sid);
      this.before.delete(sid);
      const at = this.store.orderedIds.indexOf(sid);
      if (at >= 0) this.store.orderedIds.splice(at, 1);
      this.store.orderedIds.splice(b ? Math.min(b.index, this.store.orderedIds.length) : this.store.orderedIds.length, 0, sid);
      if (b?.group != null && this.prefs.collections.some((c) => c.id === b.group)) void this.prefs.joinGroup(sid, b.group);
      if (b?.pinned) this.prefs.setPinnedMany([sid], true);
      void this.prefs.persistOrder();
    }
    this.host.refreshTabBar();
    this.reap();
    return true;
  }

  /** 右键 / 批量「隐藏」：交命令、给撤销 toast（`已隐藏 orders` · `已隐藏 3 个`）；不弹确认。已经隐藏的跳过。 */
  hide(sids: readonly string[]): void {
    const tabs = sids.map((s) => this.store.tabs.get(s)).filter((t): t is Tab => t !== undefined && (t.hidden ?? null) === null);
    if (tabs.length === 0) return;
    for (const t of tabs) this.before.set(t.sessionId, { index: this.store.orderedIds.indexOf(t.sessionId), group: t.group, pinned: t.pinned });
    void this.send(tabs, "on");
    const title = tabs.length === 1 ? copyText("tabBar.hide.done", { title: fullTitle(tabs[0]) }) : copyText("tabBar.hide.doneMany", { n: tabs.length });
    this.host.undoToast(
      title,
      () => void this.send(tabs, "off"),
      () => tabs.forEach((t) => this.before.delete(t.sessionId)),
    );
  }

  /**
   * 「已隐藏」里的行 ·批量「取消隐藏」：写 `hide: "off"`（规则隐藏的一样，之后不再按那一条隐藏）。推帧到了才挪回栏上。
   */
  unhide(sids: readonly string[]): void {
    const tabs = sids.map((s) => this.store.tabs.get(s)).filter((t): t is Tab => t !== undefined && (t.hidden ?? null) !== null);
    if (tabs.length > 0) void this.send(tabs, "off");
  }

  private async send(tabs: readonly Tab[], to: HideSet): Promise<void> {
    await Promise.all(
      tabs.map((t) =>
        this.host.setHide(t, to).catch((e: unknown) => {
          this.before.delete(t.sessionId);
          const title = fullTitle(t);
          this.host.failed(to === "on" ? copyText("tabBar.hide.failed", { title }) : copyText("tabBar.unhide.failed", { title }), e);
        }),
      ),
    );
  }

  /** 隐藏了又结束的：不留（「已隐藏」只列活的，要看去历史）；正看着的那一个切走再收。 */
  reap(): void {
    for (const t of [...this.store.tabs.values()]) {
      if ((t.hidden ?? null) !== null && isResumeOnly(t.state) && t.sessionId !== this.store.activeId) this.host.close(t.sessionId);
    }
  }
}
