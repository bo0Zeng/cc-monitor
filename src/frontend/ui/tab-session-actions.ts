/**
 * 〔拆 `tabs.ts` ⑤〕**对一个会话做的动作**。
 *
 * resume（直连 / tmux / 就地）· 换号重启（交那台一条命令做完）· 杀 tmux 会话 · 打开工作目录 ·
 * 在新窗口打开 · 切到终端窗口；外加 tab 层那几条零散的后端调用（忘掉会话 · 把 monitor 拉到前面 ·
 * 红绿灯快照 · DEV 探针日志）。
 *
 * # 本文件不再直呼 `invoke`
 *
 * U2 拆 `tabs.ts` 时把 tab 层的每一条 `invoke` 收进本文件（「直接 `import { invoke }` 的生产文件」是
 * `generated-boundary-guard.vitest.ts` 的恒等计数，拆开不许涨）。C4a 把这 11 处连同 `accounts.ts` 那 5 处
 * 一起收进包装层 `ipc/commands.ts`（缺的十条命令补进去）⇒ 那个计数 3 → 1，只剩包装层自己；
 * 本文件与 tab 层其余几份一样，只经 `commands.x(…)` 说话。
 *
 * # 它要宿主给什么
 *
 * 只要四样只读 / 回调（`TabSessionHost`）：按 sid 取 tab · 能不能 attach · 会话账号行 · 重刷一个账号徽章。
 * 不碰 tab 栏、不碰流 DOM。方法体逐字从 `tabs.ts` 搬来，唯一的改写是 `this.tabs.get(` 等四处宿主读数
 * 换成 `this.host.…`（同一个值，换了个取法）。
 */
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { askOf, FOLLOW, type AccountAsk } from "./launch-account";
import { resumeLocalSession } from "./local-resume";
import { toast } from "./kit/toast";
import { runRemoteResume } from "./remote-launch-run";
// 本机 = `LOCAL_ORIGIN`（`"<local>"`，与 Rust `origin.rs::LOCAL` 跨语言对拍）；
// 「是不是本机」只经 `ipc/origin.ts` 判。`accounts.ts` 那个同名的 `"__local__"` 已退役 —— 全仓只剩一个本机表示。
import { isLocalOrigin, isRemoteOrigin, type Origin } from "./ipc/origin";
import { planRemoteFront } from "./remote-terminal-front";
import type { FrontResult } from "./front-result";
import type { FrontOutcome } from "./generated/FrontOutcome";
import { commands } from "./ipc/commands";
import { probeSessionRecord, reasonOf, type RecordProbe } from "./session-reads";
// F78：远端会话「打开工作目录」→ 用该机配置开文件窗口进入远端 cwd（而非只提示打不开）。老 SFTP 面板删了。
import { openFileWindow } from "./file-window";
import {
  readRemoteConfig,
  findHostByOrigin,
  resumeCommandFor,
} from "./remote-config";
import { callStop, sayReply, type Reply, type StartItem } from "./tab-batch-run";
import { startInTmuxThenAttach } from "./tmux-resume";
// 标签页里的会话都是流跟的那一家（记录树那一家）。
import type { Tab } from "./tab-model";
import { copyText } from "./copy-table";
import { decodeKilled, saidOfControl } from "./tmux-control";
import { offerResyncRetry, resyncMachines, resyncMachinesSaid } from "./resync";

/**
 * auto-e2e F-E0:DEV-only 断言出口。同 e2e-probe.ts 的 `log()`——把状态转移写成可 grep 的
 * `[e2e]` 行(console.info + frontend_perf_log → monitor 日志)。**`import.meta.env.DEV` 门控**:
 * 生产构建 DEV 恒 false,整支被 vite 静态消除(zero prod 包含,同 e2e-probe 范式)。
 */
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
  /** E73：attach / 「杀死空 tmux」这几个动作对这个会话有没有意义。 */
  isAttachable(sid: string): boolean;
  /** resume 一跳问过那台后端之后，把「记录在不在」落进这条 tab 的状态（`TabManager.markRecord`）。 */
  markRecord(sid: string, present: boolean): void;
}

export class TabSessionActions {
  /** F04：正在 resumeTabTmux 中的 sid——双击"Resume（tmux）"之间没有
   *  互斥时，两次并发调用各自查一次陈旧的 `list_remote_tmux` 快照、各自算出"该建哪个名字"，
   *  可能算出两个不同名字、真建出两个都声称同一 sid 的 tmux 容器（R10 的一个具体、可关闭的成因，
   *  见 F04 计划 §2 综合来源方案 A §7.4）。 */
  private resumingSids = new Set<string>();
  constructor(private readonly host: TabSessionHost) {}

  /**
   * issue #10：在独立只读窗口打开指定 session（Tab 右键 / 快捷键 / 拖拽撕离）。
   *
   * `screenX` / `screenY`（可选）= 拖拽撕离的落点屏幕坐标（来自 mouseup 的
   * `e.screenX/screenY`）。两者都给出时透传给后端在该处摆放新窗口；右键 / 快捷键
   * 不传则后端走默认居中。
   */
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
      toast(copyText("tabSessionActions.openInWindow.failed"), String(e));
    }
  }

  /**
   * **resume 一跳先问那台后端：这条会话的记录还在不在**（最后一条
   * 「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」）。
   *
   * - 不在 ⇒ 说清查的是哪台、哪棵记录树，tab 落「记录已不在」，返回 `false`（调用方**不开终端**）；
   * - 在 ⇒ 「记录已不在」翻回已结束（记录回来了），返回 `true`；
   * - 问不到（通道没起 / 后端太旧不认 `history-record` / 超时）⇒ 当「不知道」、返回 `true`：照今天的路走，
   *   **不许把「问不到」当成「不在」**（那会把一条其实接得上的 resume 拦掉）。
   * 照起，但说一句「查不到记录还在不在」；形状不对按说「两端契约对不上」
   *   （`session-reads.ts::reasonOf` 那一份，不另写）—— 出声不静默。
   *
   * 判定住那台的后端（只收 sid），这里只读答案 —— 前端不做文件存在性探测。
   *
   * `configDir` = **这次 resume 要用的那个账号配置目录**（那台后端判完号、开窗之前交回来的那一个）。
   * 那台后端就在那棵树里找；不带（账号 0 / 不指定）⇒ 查它自己的家目录。
   */
  private async recordStillThere(tab: Tab, configDir?: string): Promise<boolean> {
    let probe: RecordProbe;
    try {
      // 经通道直接问那台后端的 `history-record`（`session-reads.ts::probeSessionRecord`）。
      probe = await probeSessionRecord(tab.origin, tab.sessionId, configDir);
    } catch (e) {
      // 问不到 / 形状不对（旧后端）同「不知道」：只有一个明明白白的 `present: false` 才拦 —— 但说出来。
      toast(
        copyText("tabSessionActions.recordUnknown.title"),
        reasonOf(e, tab.origin),
      );
      return true;
    }
    this.host.markRecord(tab.sessionId, probe.present);
    if (probe.present) return true;
    toast(
      copyText("sessionState.recordGone.title"),
      copyText("sessionState.recordGone.body", {
        who: isLocalOrigin(tab.origin) ? copyText("tabSessionActions.who.local") : copyText("tabSessionActions.who.remote", { machine: tab.origin }),
        root: probe.root,
        sid: tab.sessionId,
      }),
    );
    return false;
  }

  /**
   * tab 栏「重新读取」：`origins` 里每台一次整机对齐 ＋ 补读，做完按台说一句。补出来的行照常经流到达。
   * 回成功的那几台（调用方对它们做「对齐做完」那一步）。
   */
  /** 离线条的［重新连接］：那台断着在退避里等 ⇒ 立刻重拨一次（壳那一侧 `backend_start`：流在跑就是「别等了」）。 */
  reconnect(origin: string): void {
    void this.redial(origin).catch((e: unknown) => console.warn("reconnect failed:", e));
  }

  /** 立刻重拨那台一次；没拨成 ⇒ 抛（↗ 浮层［更新］换上之后要知道拨没拨成）。 */
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
   * 没了的**标出来**（`markRecord`）、说一句；**不自动摘** —— 点那条提示才摘（`unpin`）。问不到 ⇒ 不标。
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
      this.host.markRecord(tab.sessionId, probe.present);
      if (!probe.present) gone.push(tab);
    }
    if (gone.length === 0) return;
    toast(
      copyText("sessionState.pinGone.title"),
      copyText("sessionState.pinGone.body", { n: gone.length, names: gone.map((t) => t.title).join(", ") }),
      { level: "info", onClick: () => gone.forEach((t) => unpin(t.sessionId)) },
    );
  }

  /**
   * F37：手动 resume 一个已结束（灰）的 Tab。与历史浏览器 ↺ 同一套语义：
   * 本地 → 新终端窗口跑 resume（尊重 F34 自定义命令，缺省 cc 检测→claude）；
   * 远端 → F41 一键拉起 wt.exe/PowerShell 跑 `ssh -t …`，失败回退复制命令。
   * resume 成功后 CC 续写同一 jsonl，既有「会话复活」路径会自动把灰 Tab 点亮。
   */
  /** 说不清（那台暂时看不见）⇒ 不恢复、说一句：会话也许还在跑，再起一份就是同一会话两个 claude。 */
  private refuseUnseen(tab: Tab): boolean {
    if (tab.state.liveness !== "unseen") return false;
    toast(copyText("sessionState.unseen.noResume"), copyText("sessionState.unseen.tooltip"), {
      level: "info",
    });
    return true;
  }

  /** 这个会话是哪一家（会话事实到了才知道）；还不知道 ⇒ 说一句、`null`（不落哪一家）。 */
  private agentOrSay(tab: Tab): string | null {
    if (tab.agent !== null) return tab.agent;
    toast(copyText("tabSessionActions.agent.unknown"), "", { level: "info" });
    return null;
  }

  async resumeTab(sid: string, accountName?: string, useBase = false): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab || this.refuseUnseen(tab)) return;
    const agent = this.agentOrSay(tab);
    if (agent === null) return;
    // 用哪个号那台判（点名的号 / 账号 0 / 跟随这条会话上次的号）；判完、开窗之前问记录还在不在（查那个号那棵树）。
    const account = askOf(accountName, useBase);
    const preflight = (configDir: string | undefined): Promise<boolean> => this.recordStillThere(tab, configDir);
    if (isRemoteOrigin(tab.origin)) {
      const origin = tab.origin;
      await runRemoteResume(
        origin,
        agent,
        sid,
        tab.projectDir ?? "",
        await resumeCommandFor(origin),
        { account, preflight },
      );
      return;
    }
    // 本机 resume 的编排只有一份（`local-resume.ts`）：铸名 → 起（本机后端判号）。
    await resumeLocalSession({
      agent,
      sid,
      cwd: tab.projectDir ?? "",
      account,
      launcher: await resumeCommandFor(tab.origin),
      preflight,
    });
  }

  /**
   * F52：tmux 版 resume（远端专用）——在远端 tmux 会话 `cc-<sid8>` 里幂等 resume Claude。
   * 与 resumeTab 的直连版并列;本地 tab（origin===null）无 tmux 用例,直接 return。
   *
   * F04：`resumingSids` 互斥——双击之间没有互斥时，两次并发调用各自
   * 查一次陈旧快照、各自可能算出不同的 fresh tmux 名，真建出两个都声称同一 sid 的容器（R10 的
   * 一个具体、可关闭的成因）。
   *
   * F09：`accountName` 与 `resumeTab`（直连版）的参数顺序/语义对齐——此前本方法完全没有显式选号
   * 能力（账号恒是跟随），是 account×container 没做到真正正交的一个实现缺口
   * （旧扁平菜单从未提供"把此归档会话切到账号 X（tmux）"这一项，反映的正是这个缺口）；flyout
   * 把 account 组与 container 组做成正交修饰后，这个缺口必须补上，否则"账号=X + 容器=tmux"
   * 这个组合在 UI 上可选却在实现上是假的。
   */
  async resumeTabTmux(sid: string, accountName?: string, useBase = false): Promise<void> {
    if (this.resumingSids.has(sid)) return;
    this.resumingSids.add(sid);
    try {
      await this.resumeTabTmuxInner(sid, askOf(accountName, useBase));
    } finally {
      this.resumingSids.delete(sid);
    }
  }

  private async resumeTabTmuxInner(sid: string, account: AccountAsk): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab || isLocalOrigin(tab.origin) || this.refuseUnseen(tab)) return;
    // 起法与批量「在 tmux 里后台起」同一条（那台判号 · 判在不在跑 · 空 tmux 就地键入 · 铸名交一行 ccm，建完不接进去），
    //   单个只多一步：起好之后开一个终端接进去。
    const item: StartItem = { sid, cwd: tab.projectDir ?? "", account };
    await this.startInTmuxThenAttach(tab, item, (again) => this.resumeTabTmuxInner(sid, again ?? account));
  }

  /**
   * 交那台在 tmux 里起这一个（`sessions-start`，与批量同一条），起好了（或本来就在跑）开一个终端接进去。
   * 起法本身不依赖标签页对象，住 `tmux-resume.ts`（历史页「恢复 ▾」同一条）；这里只把那台说的「记录在不在」落进这条 tab。
   */
  private async startInTmuxThenAttach(
    tab: Tab,
    item: StartItem,
    again: (account?: AccountAsk) => Promise<void>,
  ): Promise<void | false> {
    const agent = this.agentOrSay(tab);
    if (agent === null) return false;
    return startInTmuxThenAttach(
      { origin: tab.origin, agent, sid: tab.sessionId, cwd: item.cwd },
      item.account ?? FOLLOW,
      { again, onRecord: (present) => this.host.markRecord(tab.sessionId, present) },
    );
  }

  /** 本机：在那个空 tmux 里就地 resume，再开一个终端接进去（与批量同一条；账号跟随同本机 Resume）。 */
  async resumeLocalInTmux(sid: string, account: AccountAsk = FOLLOW): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab) return;
    const item: StartItem = { sid, cwd: tab.projectDir ?? "", account };
    await this.startInTmuxThenAttach(tab, item, (again) => this.resumeLocalInTmux(sid, again ?? account));
  }

  /**
   * 杀死这个会话所在的 tmux 会话：二次确认后交那台（`sessions-stop`，与批量同一条：那台按 sid 认出是哪个、过三道门、杀句柄）。
   * `name` 只用来在确认框里说清是哪个（菜单就绪时那台答的）。变已结束由会话流兜（不主动改状态）。
   * `idle` = 只剩空 shell 的那个（文案别说「正在运行的 Claude」；杀掉它 ⇒ tab 变已结束 ⇒ 可 Resume）。
   */
  killInTmux(origin: string, sid: string, name: string, opts?: { confirm?: ConfirmFn; idle?: boolean; title?: string }): void {
    const isLocal = isLocalOrigin(origin);
    const confirmFn: ConfirmFn = opts?.confirm ?? confirmDialog;
    const machine = isLocal ? copyText("tabSessionActions.who.local") : origin;
    const title = opts?.title ?? name;
    const keep = { label: copyText("kit.interrupts.keep"), items: [copyText("tabSessionActions.kill.keeps")] };
    // tmux 会话名放在正文，不放菜单。Claude 已退出、只剩 tmux 会话的：标题点名 tmux 会话。
    const spec = opts?.idle
      ? {
          title: copyText("sessionState.killIdle.title", { name }),
          body: copyText("sessionState.reconnectable.name"),
          rows: [keep],
          action: copyText("sessionState.killIdle.action"),
        }
      : {
          title: copyText("tabSessionActions.kill.title", { title }),
          rows: [{ label: copyText("kit.interrupts.cut"), items: [copyText("tabSessionActions.kill.cuts", { machine, name })] }, keep],
          action: copyText("tabSessionActions.kill.action"),
        };
    const kill = async (): Promise<void> => {
      let r: Reply;
      try {
        [r] = await callStop(origin, [sid]);
      } catch (e) {
        toast(copyText("tabSessionActions.kill.failed", { title }), saidOfControl(e));
        return;
      }
      if (r.outcome !== "done") {
        const said = sayReply(origin, "stop", r);
        // 关卡 2 拒的 ⇒ 提示带「对齐后重试」（只对这个会话重验 ＋ 重打，再过一次关卡）。
        if (r.why === "wrong_owner") offerResyncRetry(origin, sid, copyText("tabSessionActions.kill.failed", { title }), said, kill);
        else toast(copyText("tabSessionActions.kill.failed", { title }), said);
        return;
      }
      // 顺手从 cc-bus 名册注销的结局说一句（没有要说的就不说；说法同单个 `kill` 那一份）。
      let bus: string | null = null;
      try {
        bus = decodeKilled(origin, r.session ?? name, { session: r.session ?? name, killed: true, bus: r.bus });
      } catch {
        bus = null;
      }
      toast(copyText("sessionState.kill.done", { title }), bus ?? "", { level: "success" });
    };
    void (async () => {
      if (!(await confirmFn({ ...spec, danger: true }))) return;
      await kill();
    })();
  }

  /** 打开指定 Tab 的 cwd。本地 → 系统文件管理器；远端 → 文件窗口进入该远端目录（F78）。无 cwd 忽略。 */
  async openTabCwd(sid: string): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab?.projectDir) return;
    // F78：远端 Tab 的 cwd 是远端路径，本地 openPath 打不开——改成用该机配置开 SFTP 进入该目录
    // （Batch9-F29 曾从静默 no-op 改成 info 提示；现进一步真能浏览）。找不到该机配置才回退提示。
    if (isRemoteOrigin(tab.origin)) {
      const host = findHostByOrigin((await readRemoteConfig()).hosts, tab.origin);
      if (host && host.host.trim() !== "" && host.user.trim() !== "") {
        void openFileWindow(host, { dir: tab.projectDir });
        return;
      }
      // 找到但缺 host/user = 配置不完整；没找到 = 未配置——分开措辞（审计建议）。
      const why = host
        ? copyText("tabSessionActions.openCwd.incomplete")
        : copyText("tabSessionActions.openCwd.noConfig");
      toast(
        copyText("tabSessionActions.openCwd.title"),
        copyText("tabSessionActions.openCwd.body", { machine: tab.origin, cwd: tab.projectDir, why }),
        { level: "info" },
      );
      return;
    }
    try {
      await openPath(tab.projectDir);
    } catch (e) {
      console.warn(`[tabs] openPath ${tab.projectDir} failed:`, e);
    }
  }
}

/** ↗ 在飞的会话：同一个会话在飞时再点不重发。 */
const frontInFlight = new Set<string>();
/** 过了这么久还没结果才进「进行中」（防闪）；一旦进了，至少停这么久。 */
export const FRONT_PENDING_AFTER_MS = 300;
export const FRONT_PENDING_MIN_MS = 400;

/**
 * ↗ 的一次点击：同一个会话在飞 ⇒ 不重发；`run` 超过 [`FRONT_PENDING_AFTER_MS`] 还没完 ⇒ `pending(true)`
 * （按钮进「进行中」），完了且已停够 [`FRONT_PENDING_MIN_MS`] ⇒ `pending(false)`。
 */
export async function frontOnce<T>(sid: string, run: () => Promise<T>, pending: (on: boolean) => void): Promise<T | undefined> {
  if (frontInFlight.has(sid)) return undefined;
  frontInFlight.add(sid);
  let shownAt: number | null = null;
  // 调度：一次性 —— ↗ 在飞超过 300ms 才把按钮换成「进行中」，答到了就清
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

/** 壳那一跳的期限（极端情况下 Win32 调用卡住）：到了 ⇒ 「无应答」。 */
const FRONT_SHELL_MS = 5000;

/** 壳那一跳：期限内回结局；到期 ⇒ `timeout`；出错 ⇒ `unknown`（细节可复制）。 */
async function shellFront(ask: () => Promise<FrontOutcome>): Promise<FrontResult> {
  let timer = 0;
  try {
    return await Promise.race([
      ask(),
      new Promise<FrontResult>((r) => {
        // 调度：一次性 —— ↗ 壳那一跳的期限，到点落成「无应答」（本机 / 远端共用）
        timer = window.setTimeout(() => r({ kind: "timeout" }), FRONT_SHELL_MS);
      }),
    ]);
  } catch (e) {
    console.warn("terminal front failed:", e);
    return { kind: "unknown", detail: String((e as Error)?.message ?? e) };
  } finally {
    window.clearTimeout(timer);
  }
}

/** 拉本机会话的终端到前台：壳按 sid → 窗口缓存找、三重指纹校验、拉前；回结局族（界面照族排版，`front-result.ts`）。 */
export function bringTerminalToFront(sessionId: string): Promise<FrontResult> {
  return shellFront(() => commands.bring_terminal_to_front({ sessionId }));
}

/**
 * 拉远端会话对应的本机终端窗口：点那一刻现查 —— 那台谁在显示它 · 按窗口标签找 · 本机哪串进程开着那条连接，顺序住
 * `remote-terminal-front.ts`；最后一跳壳沿进程链找属主的窗口、校验、拉前。每一步的结局都落成结局族。
 */
export async function bringRemoteTerminalToFront(origin: Origin, sessionId: string): Promise<FrontResult> {
  // 按窗口标签那一问出了事（monitor 没回话 / 抛了）⇒ 当没对上，接着按连接对。
  const byLabel = (terminals: unknown[]): Promise<FrontResult | null> =>
    commands.bring_remote_terminal_to_front({ terminals }).catch((e: unknown) => {
      console.warn("terminal front by label failed:", e);
      return null;
    });
  const plan = await planRemoteFront(origin, sessionId, byLabel);
  if ("result" in plan) return plan.result;
  return shellFront(async () => {
    const o = await commands.bring_remote_terminal_to_front({ chain: plan.chain });
    if (o === null) throw new Error("bring_remote_terminal_to_front: no outcome for a chain");
    return o;
  });
}

/** 关 tab 时让后端 event_replay 把这个 session 的历史也丢掉（失败只记日志）。 */
export function forgetSession(sessionId: string): void {
  void commands.forget_session({ sessionId }).catch((e) => {
    console.warn(`forget_session ${sessionId} failed:`, e);
  });
}

/** v2.4 issue #2：自动跟随时把 monitor 窗口拉到前台（失败只记日志）。 */
export function bringMonitorToFront(): void {
  void commands.bring_monitor_to_front().catch((e) => {
    console.warn("bring_monitor_to_front failed:", e);
  });
}

