/**
 * 对一个会话做的动作：恢复 · 换号重启 · 杀 tmux 会话 · 打开工作目录 · 在新窗口打开 · 把终端窗口拉到前面；
 * 外加 tab 层零散的后端调用（忘掉会话 · 把 monitor 拉到前面 · DEV 探针日志）。
 * 只经 `commands.x(…)` 说话（不直呼 `invoke`）；宿主只给四样只读 / 回调（`TabSessionHost`），不碰 tab 栏、不碰流 DOM。
 */
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import type { AccountAsk } from "./launch-account";
import type { Launch } from "./generated/Launch";
import { toast, failToast, noteMessage } from "./kit/toast";
import { detailOf, saidOf } from "./kit/detail";
import { openSession } from "./session-open";
import { isLocalOrigin, isRemoteOrigin, type Machine } from "./ipc/origin";
import type { FrontTold } from "./generated/FrontTold";
import { commands } from "./ipc/commands";
import { probeSessionRecord, type RecordProbe } from "./session-reads";
import { openFileWindow } from "./file-window";
import { askSessions } from "./tab-batch-run";
import type { SessionsBatchReply } from "./generated/SessionsBatchReply";
// 标签页里的会话都是流跟的那一家（记录树那一家）。
import type { Tab } from "./tab-model";
import { copyText } from "./copy-table";
import { askEnd, sayReply } from "./end-confirm";
import { offerResyncRetry, resyncMachines, resyncMachinesSaid } from "./resync";

/** DEV 断言出口：把状态转移写成可 grep 的 `[e2e]` 行。`import.meta.env.DEV` 门控，生产构建整支消除。 */
export function e2eLog(line: string): void {
  if (import.meta.env.DEV) {
    console.info(line);
    void commands.frontend_perf_log({ lines: line }).catch(() => {});
  }
}

/** 会话动作要宿主给的四样东西（全是读数或回调，动作本身不持有 tab 集合）。 */
export interface TabSessionHost {
  /** 按 sid 取 tab（`TabManager` 那张表；没有 ⇒ `undefined`）。 */
  tab(sid: string): Tab | undefined;
}

export class TabSessionActions {
  constructor(private readonly host: TabSessionHost) {}

  /** 在独立只读窗口打开（右键 / 快捷键 / 拖拽撕离）。拖拽撕离给落点屏幕坐标，后端在那里摆窗口；不给就居中。 */
  async openInNewWindow(
    sid: string,
    screenX?: number,
    screenY?: number,
  ): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab) return;
    try {
      await commands.open_session_in_new_window({
        sessionId: sid,
        origin: tab.origin,
        title: tab.title,
        ...(screenX !== undefined && screenY !== undefined
          ? { x: screenX, y: screenY }
          : {}),
      });
    } catch (e) {
      failToast(copyText("tabSessionActions.openInWindow.failed"), e);
    }
  }

  /**
   * ［重新连接］（机器条 · 会话头 · 新建会话框）：那台断着在退避里等 ⇒ 立刻重拨一次（壳那一侧 `backend_start` ⇒ 本机后端 `machine-wake`）。
   * 拒了（停用着 · 正在连 · 机器表里没有 · 本机后端不在）⇒ 那一句原样进状态栏消息；连上 · 正在连那一态照常由那台的成品画在那一行上。
   */
  reconnect(origin: string): void {
    void this.redial(origin).catch((e: unknown) => noteMessage(saidOf(e), detailOf(e)));
  }

  /** 立刻重拨那台一次；没拨成 ⇒ 抛（拉窗那一条上的［更新］点了之后要知道拨没拨成）。 */
  redial(origin: string): Promise<void> {
    return commands.backend_start({ origin }).then(() => undefined);
  }

  async rereadMachines(origins: Iterable<string>): Promise<string[]> {
    const rs = await resyncMachines(origins);
    if (rs.length === 0) {
      toast(copyText("resync.machines.title"), copyText("resync.machines.none"), { level: "info" });
      return [];
    }
    const ok = rs.filter((m) => "r" in m).map((m) => m.origin);
    const title = ok.length === rs.length ? copyText("resync.machines.title") : copyText("resync.machines.titlePartial");
    toast(title, resyncMachinesSaid(rs), ok.length === rs.length ? { level: "info" } : undefined);
    return ok;
  }

  /**
   * 那台「重新对齐」过 ⇒ 它上面的固定条逐条问一次记录还在不在（与 resume 前那一问同一个 `history-record`）。
   * 号的记录树都链回同一份共享库 ⇒ 不按号分树，问那台的家。
   * 没了的那台会重发一帧脸（记录已不在）；这里说一句、**不自动摘** —— 点那条提示才摘（`unpin`）。问不到 ⇒ 不标。
   */
  async flagPinsWithoutRecord(origin: string, pinned: Tab[], unpin: (sid: string) => void): Promise<void> {
    if (pinned.length === 0) return;
    const gone: Tab[] = [];
    for (const tab of pinned) {
      let probe: RecordProbe;
      try {
        probe = await probeSessionRecord(origin, tab.sessionId, undefined);
      } catch {
        continue;
      }
      if (!probe.present) gone.push(tab);
    }
    if (gone.length === 0) return;
    toast(
      copyText("sessionState.pinGone.title"),
      copyText("sessionState.pinGone.body", { n: gone.length, names: gone.map((t) => t.title).join(", ") }),
      { level: "info", onClick: () => gone.forEach((t) => unpin(t.sessionId)) },
    );
  }

  /** 说不清（那台暂时看不见）⇒ 不恢复、照脸说一句（为什么不恢复 ＋ 那台离线那一句，都是后端写的）：会话也许还在跑，再起一份就是同一会话两个 claude。 */
  private refuseUnseen(tab: Tab): boolean {
    const face = tab.face;
    if (face?.dot !== "unknown") return false;
    toast(face.can.resume.can === "no" ? face.can.resume.why : face.name, face.line, { level: "info" });
    return true;
  }

  /**
   * 恢复这一个：说不清那台在不在跑 ⇒ 不恢复；否则交那台（`session-open`：判号 · 判放在哪 · 问记录 · 活进程门 · 在跑就接上），
   * 记录在不在那台记进账本、随整份脸回来。`ask` 里没有的格 ＝ 跟随 / 照这台。同一个会话在飞时再点不重发（`session-open.ts`）。
   */
  async resume(sid: string, ask: { account?: AccountAsk; run?: Launch } = {}): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab || this.refuseUnseen(tab)) return;
    await openSession(tab.origin, { what: { kind: "resume", sid }, ...ask, local: isLocalOrigin(tab.origin) });
  }

  /**
   * 结束这个会话：结束会话那一个确认框（`end-confirm.ts`）答了之后交本机后端一项的那一批（`sessions-batch`，与批量同一条：那台按 sid 认出名单里那一行、
   * 交 `terminal-end`，过三道门、杀句柄；命中几个 · 不在终端里都是那台判、那台说；点名那一句本机后端写）。变已结束由会话流兜（不主动改状态）。
   */
  endSession(origin: string, sid: string, title: string, opts?: { confirm?: ConfirmFn }): void {
    const kill = async (): Promise<void> => {
      let reply: SessionsBatchReply;
      try {
        reply = await askSessions("stop", [{ machine: origin, sid }]);
      } catch (e) {
        failToast(copyText("tabSessionActions.kill.failed", { title }), e);
        return;
      }
      const one = reply.results[0];
      // 关卡 2 拒的 ⇒ 提示带「对齐后重试」（只对这个会话重验 ＋ 重打，再过一次关卡）：挑码选按钮，那几句照回包。
      if (one?.why === "wrong_owner") offerResyncRetry(origin, sid, reply.said, one.said ?? "", kill);
      else sayReply(reply);
    };
    void (async () => {
      if (!(await askEnd([title], opts?.confirm ?? confirmDialog))) return;
      await kill();
    })();
  }

  /** 打开 Tab 的工作目录：本机 ⇒ 系统文件管理器；远端 ⇒ 文件窗口进那台的那个目录。没有目录 ⇒ 不做。 */
  async openTabCwd(sid: string): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab?.projectDir) return;
    // 远端路径本机打不开 ⇒ 文件窗口只凭那台名字开进该目录（认不出那台那一句由壳说）。
    if (isRemoteOrigin(tab.origin)) {
      await openFileWindow(tab.origin, { dir: tab.projectDir });
      return;
    }
    try {
      await commands.os_open({ kind: "path", path: tab.projectDir });
    } catch (e) {
      console.warn(`[tabs] os_open ${tab.projectDir} failed:`, e);
    }
  }
}

/** 「终端」拉窗在飞的会话：同一个会话在飞时再点不重发。 */
const frontInFlight = new Set<string>();
/** 过了这么久还没结果才进「进行中」（防闪）；一旦进了，至少停这么久。 */
export const FRONT_PENDING_AFTER_MS = 300;
export const FRONT_PENDING_MIN_MS = 400;

/**
 * 「终端」拉窗的一次点击：同一个会话在飞 ⇒ 不重发；`run` 超过 [`FRONT_PENDING_AFTER_MS`] 还没完 ⇒ `pending(true)`
 * （按钮进「进行中」），完了且已停够 [`FRONT_PENDING_MIN_MS`] ⇒ `pending(false)`。
 */
export async function frontOnce<T>(sid: string, run: () => Promise<T>, pending: (on: boolean) => void): Promise<T | undefined> {
  if (frontInFlight.has(sid)) return undefined;
  frontInFlight.add(sid);
  let shownAt: number | null = null;
  // 调度：一次性 —— 拉窗在飞超过 300ms 才把按钮换成「进行中」，答到了就清
  const show = window.setTimeout(() => {
    shownAt = Date.now();
    pending(true);
  }, FRONT_PENDING_AFTER_MS);
  try {
    return await run();
  } finally {
    window.clearTimeout(show);
    if (shownAt !== null) {
      const left = FRONT_PENDING_MIN_MS - (Date.now() - shownAt);
      // 调度：一次性 —— 「进行中」至少停 400ms 再收，防闪
      if (left > 0) await new Promise<void>((r) => window.setTimeout(r, left));
      pending(false);
    }
    frontInFlight.delete(sid);
  }
}

/**
 * 「终端」拉会话的终端窗口到前台（本机会话与远端会话同一条）：壳一条命令做完 —— 问本机后端这条会话在这台电脑上的哪儿、
 * 先按标签再沿链找窗口、校验、拉前，回切过去了或本机后端写好的那一张（`front-result.ts` 照摆）。
 * 壳问本机后端没问成 ⇒ 拒（那一份失败原样，调用方走 `frontAskFailed`）。
 */
export async function bringTerminalToFront(origin: Machine, sessionId: string): Promise<FrontTold> {
  return commands.bring_terminal_to_front({ origin, sid: sessionId });
}

/** 关 tab 时让后端把这个会话的重放历史丢掉（失败只记日志）。 */
export function forgetSession(sessionId: string): void {
  void commands.forget_session({ sessionId }).catch((e) => {
    console.warn(`forget_session ${sessionId} failed:`, e);
  });
}

/** 报给壳「主窗口此刻在看这几个会话」（整份换；失败只记日志）：没在看的会话，它的行不上流（壳合成每台的名单）。 */
export function reportWatching(sessions: { origin: Machine; sessionId: string }[]): void {
  void commands.watch_sessions({ sessions }).catch((e: unknown) => {
    console.warn("[tabs] 报「在看哪个会话」失败：", e);
  });
}

/** 报给壳「主窗口手上有这几个 tab」（整份换；失败只记日志）：壳随名单报给各台，那台不在说的它答已结束。 */
export function reportKept(sessions: { origin: Machine; sessionId: string }[]): void {
  void commands.kept_sessions({ sessions }).catch((e: unknown) => {
    console.warn("[tabs] 报「手上有哪几个 tab」失败：", e);
  });
}

/** 自动跟随时把 monitor 窗口拉到前台（失败只记日志）。 */
export function bringMonitorToFront(): void {
  void commands.bring_monitor_to_front().catch((e) => {
    console.warn("bring_monitor_to_front failed:", e);
  });
}

