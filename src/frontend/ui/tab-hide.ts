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
import { isEnded } from "./face-can";
import { sameHidden, undoHide, type HideSet, type SessionHidden } from "./session-hidden";
import { copyText } from "./copy-table";
import type { Hide } from "./generated/Hide";
import { fullTitle } from "./session-face";
import type { EndRow } from "./end-confirm";
import { detailOf, saidOf } from "./kit/detail";

/** 隐藏那几件要宿主做的：交命令 · 出 toast · 重画 · 关掉一个已结束的。 */
export interface TabHideHost {
  /** 改单个那一格（`history-annotate {hide}`），回改之前那一格；失败 ⇒ 抛。 */
  setHide(tab: Tab, to: HideSet): Promise<Hide | null>;
  /** 可撤的 toast（撤销 ⇒ `undo`；过期 ⇒ `settle`）。 */
  undoToast(title: string, undo: () => void, settle: () => void): void;
  /** 没交成那一句。 */
  failed(title: string, e: unknown): void;
  /**
   * 一批（≥ 2 个）里有没成的：结束那一种结局一条（`end-confirm.ts::sayRows`，汇总照 `tab-batch-menu.ts::batchHead(action)`）；
   * `undo` 给了 ⇒ 同一条上［撤销］成了的那几个。
   */
  batch(action: string, rows: readonly EndRow[], undo?: { run: () => void; settle: () => void }): void;
  refreshTabBar(): void;
  /** 关掉一个已结束的（同 ×：拆流、让后端忘掉）。 */
  close(sid: string): void;
}

/** 交了没成的那一个：哪一个 · 交的是不是「隐藏」· 那一句。 */
interface Missed {
  tab: Tab;
  on: boolean;
  e: unknown;
}

/** 一批的逐条结局（照交的先后）：没成的带那台那一句与详情，别的算成了。 */
function rowsOf(tabs: readonly Tab[], missed: readonly Missed[]): EndRow[] {
  return tabs.map((t): EndRow => {
    const m = missed.find((x) => x.tab === t);
    if (!m) return { title: fullTitle(t), outcome: "done", why: "" };
    const detail = detailOf(m.e);
    return { title: fullTitle(t), outcome: "failed", why: saidOf(m.e), ...(detail !== "" ? { detail } : {}) };
  });
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
   * 右键 / 批量「隐藏」：交命令；回包成了才给撤销 toast，只数成了的那几个（`已隐藏 orders` · `已隐藏 ×3`）；
   * 没成的各说那一句（全没成 ⇒ 只有那几句）。不弹确认；已经隐藏的跳过。
   */
  hide(sids: readonly string[]): void {
    const tabs = sids.map((s) => this.store.tabs.get(s)).filter((t): t is Tab => t !== undefined && (t.hidden ?? null) === null);
    if (tabs.length === 0) return;
    for (const t of tabs) this.before.set(t.sessionId, { index: this.store.orderedIds.indexOf(t.sessionId), group: t.group, pinned: t.pinned });
    // 撤销回到隐藏之前那一份单独设置（各个照后端回的改之前那一格，`undoHide`），不一律钉成「单独显示」。
    const prevs = new Map<string, Hide | null>();
    void this.send(tabs, () => "on", (t, prev) => prevs.set(t.sessionId, prev)).then((missed) => {
      const done = tabs.filter((t) => prevs.has(t.sessionId));
      const undo = {
        run: () => void this.send(done, (t) => undoHide(prevs.get(t.sessionId) ?? null)).then((m) => this.sayMissed(m)),
        settle: () => done.forEach((t) => this.before.delete(t.sessionId)),
      };
      // 一批里有没成的 ⇒ 一条结局（「已隐藏 k · 失败 ×m」·［撤销］［查看］），不逐个弹。
      if (tabs.length > 1 && missed.length > 0) {
        this.host.batch(copyText("tabBatch.action.hide"), rowsOf(tabs, missed), done.length > 0 ? undo : undefined);
        return;
      }
      this.sayMissed(missed);
      if (done.length === 0) return;
      const title = done.length === 1 ? copyText("tabBar.hide.done", { title: fullTitle(done[0]) }) : copyText("tabBar.hide.doneMany", { n: done.length });
      this.host.undoToast(title, undo.run, undo.settle);
    });
  }

  /**
   * 「已隐藏」里的行 ·批量「取消隐藏」：写 `hide: "off"`（规则隐藏的一样，之后不再按那一条隐藏）。推帧到了才挪回栏上。
   */
  unhide(sids: readonly string[]): void {
    const tabs = sids.map((s) => this.store.tabs.get(s)).filter((t): t is Tab => t !== undefined && (t.hidden ?? null) !== null);
    if (tabs.length === 0) return;
    void this.send(tabs, () => "off").then((m) => {
      if (tabs.length > 1 && m.length > 0) this.host.batch(copyText("tabBatch.action.unhide"), rowsOf(tabs, m));
      else this.sayMissed(m);
    });
  }

  /** 逐个交（`to` ＝ 这一个交什么）；交成了 ⇒ `done(那一个, 改之前那一格)`。回没成的那几个（不出声，调用方定怎么说）。 */
  private async send(tabs: readonly Tab[], to: (t: Tab) => HideSet, done?: (t: Tab, prev: Hide | null) => void): Promise<Missed[]> {
    const missed: Missed[] = [];
    await Promise.all(
      tabs.map((t) => {
        const word = to(t);
        return this.host.setHide(t, word).then(
          (prev) => done?.(t, prev),
          (e: unknown) => {
            this.before.delete(t.sessionId);
            missed.push({ tab: t, on: word === "on", e });
          },
        );
      }),
    );
    return missed;
  }

  /** 没成的各说那一句。 */
  private sayMissed(missed: readonly Missed[]): void {
    for (const { tab, on, e } of missed) {
      const title = fullTitle(tab);
      this.host.failed(on ? copyText("tabBar.hide.failed", { title }) : copyText("tabBar.unhide.failed", { title }), e);
    }
  }

  /** 隐藏了又结束的：不留（「已隐藏」只列活的，要看去历史）；正看着的那一个切走再收。 */
  reap(): void {
    for (const t of [...this.store.tabs.values()]) {
      if ((t.hidden ?? null) !== null && isEnded(t.face) && t.sessionId !== this.store.activeId) this.host.close(t.sessionId);
    }
  }
}
