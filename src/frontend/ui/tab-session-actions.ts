/**
 * 〔拆 `tabs.ts` ⑤〕**对一个会话做的动作**。
 *
 * resume（直连 / tmux / 就地）· 换号重启（等 compact · 等退出）· 杀 tmux 会话 · 打开工作目录 ·
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
import { askConfirm, type ConfirmFn } from "./ask-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import type { SessionAccount } from "./accounts";
import { localFollowPlan, withAccount } from "./launch-account";
import { fetchAccounts } from "./account-reads";
import { resolveAccount } from "./accounts";
import { restartLocateFailureMessage } from "./account-restart";
import { resumeLocalSession } from "./local-resume";
import { restartWithAccount } from "./account-restart";
import { showActionFailureToast } from "./error-toast";
import { runRemoteResume, runRemoteAttach } from "./remote-launch-run";
// 本机 = `LOCAL_ORIGIN`（`"<local>"`，与 Rust `origin.rs::LOCAL` 跨语言对拍）；
// 「是不是本机」只经 `ipc/origin.ts` 判。`accounts.ts` 那个同名的 `"__local__"` 已退役 —— 全仓只剩一个本机表示。
import { isLocalOrigin, isRemoteOrigin } from "./ipc/origin";
import { commands } from "./ipc/commands";
import { probeSessionRecord, reasonOf, type RecordProbe } from "./session-reads";
import { lastAccounts } from "./history-reads";
import { getBehavior } from "./behavior";
// F78：远端会话「打开工作目录」→ 用该机配置开文件窗口进入远端 cwd（而非只提示打不开）。老 SFTP 面板删了。
import { openFileWindow } from "./file-window";
import {
  readRemoteConfig,
  findHostByOrigin,
  resolveResumeCommand,
} from "./remote-config";
import { standingOf } from "./tmux-sessions";
import { callStart, callStop, planStarts, sayReply, type Reply, type StartItem } from "./tab-batch-run";
import { AGENT_PROFILE } from "./agent-profile";
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
  /** A3：远端 live 探测的会话账号行（换号重启定位失败时拿它选哪一句话）。 */
  sessionAccount(sid: string): SessionAccount | undefined;
  /** 单个 tab 的账号徽章就地重刷（换号重启 in-flight 状态变化时）。 */
  refreshAccountBadgeFor(sid: string): void;
  /** resume 一跳问过那台后端之后，把「记录在不在」落进这条 tab 的状态（`TabManager.markRecord`）。 */
  markRecord(sid: string, present: boolean): void;
}

export class TabSessionActions {
  /** account-ux U6：正在换号重启中的 sid（防同一会话并发重启：新起的进程被后一条编排杀掉）。 */
  readonly restartingSids = new Set<string>();
  /** F04：正在 resumeTabTmux 中的 sid（对称 `restartingSids`）——双击"Resume（tmux）"之间没有
   *  互斥时，两次并发调用各自查一次陈旧的 `list_remote_tmux` 快照、各自算出"该建哪个名字"，
   *  可能算出两个不同名字、真建出两个都声称同一 sid 的 tmux 容器（R10 的一个具体、可关闭的成因，
   *  见 F04 计划 §2 综合来源方案 A §7.4）。 */
  private resumingSids = new Set<string>();
  /** A5：换号重启时「等旧号 compact 完成」的 per-sid 回调。onLine 见该 sid 的 compact 摘要行即 resolve。 */
  private compactWaiters = new Map<string, () => void>();

  constructor(private readonly host: TabSessionHost) {}

  /**
   * A5：`onLine` 每来一行都问一次 —— 有这个 sid 的 compact 等待者、且这一行就是 compact 摘要
   * ⇒ 放行那个等待者（换号重启编排随即从 compact 步进入 kill 步）。
   * 常态零开销：没有任何等待者时调用方连这一问都不问（`hasCompactWaiters`）。
   * `isCompact` 是个惰性判定，只在真有等待者时才求值（与原先 `waiter && isCompactRecord(...)` 同序）。
   */
  hasCompactWaiters(): boolean {
    return this.compactWaiters.size > 0;
  }

  settleCompact(sid: string, isCompact: () => boolean): void {
    const waiter = this.compactWaiters.get(sid);
    if (waiter && isCompact()) {
      this.compactWaiters.delete(sid);
      waiter();
    }
  }

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
      showActionFailureToast(copyText("tabSessionActions.openInWindow.failed"), String(e));
    }
  }

  /**
   * audit-fixes F01（修 B1，full-audit 阻塞）：resume 前**现读磁盘** pin，不读内存镜像
   * `accountLastByS`。后者是 tab 徽章的 10s 刷新数据源，在①启动首轮刷新前（空 Map）②
   * 上次账号那一问（本机后端 `history-last-accounts`）抛错被 main.ts 无条件覆写成空 ③刚显式钉 pin 后 10s 内还没轮询到，
   * 这三种窗口里读它 → `withAccount` 的不-clobber 守卫拿到假 priorPin=null → 把磁盘真实
   * pin 静默覆盖成全局当前账号。与 history.ts:1489 的「现读」同口径，三处 resume 一致。
   * 读不到（无 pin / 查询失败）→ undefined → withAccount 落全局账号/基座，与旧行为一致
   * （区别只是不再"错误地覆盖"既有 pin）。
   */
  private async readSessionPin(sid: string): Promise<string | undefined> {
    try {
      const map = await lastAccounts();
      return map?.[sid];
    } catch {
      return undefined;
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
   * `configDir` = **这次 resume 要用的那个账号配置目录**（远端：`withAccount` 解析出的 `mods.configDir`；
   * 本机：`localLaunchConfigDirSync`）。那台后端就在那棵树里找；不带（基座）⇒ 查它自己的家目录。
   * 因此这一问挪到了账号解析**之后**：改之前它先于解析、只查家目录 ⇒ 会话起在另一个账号根下时误拦。
   */
  private async recordStillThere(tab: Tab, configDir?: string): Promise<boolean> {
    let probe: RecordProbe;
    try {
      // 经通道直接问那台后端的 `history-record`（`session-reads.ts::probeSessionRecord`）。
      probe = await probeSessionRecord(tab.origin, tab.sessionId, configDir);
    } catch (e) {
      // 问不到 / 形状不对（旧后端）同「不知道」：只有一个明明白白的 `present: false` 才拦 —— 但说出来。
      showActionFailureToast(
        copyText("tabSessionActions.recordUnknown.title"),
        reasonOf(e, copyText("tabSessionActions.recordUnknown.oldBackend")),
      );
      return true;
    }
    this.host.markRecord(tab.sessionId, probe.present);
    if (probe.present) return true;
    showActionFailureToast(
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
  async rereadMachines(origins: Iterable<string>): Promise<string[]> {
    const rs = await resyncMachines(origins);
    if (rs.length === 0) {
      showActionFailureToast(copyText("resync.machines.title"), copyText("resync.machines.none"), { level: "info", durationMs: 4000 });
      return [];
    }
    const ok = rs.filter((m) => "r" in m).map((m) => m.origin);
    const title = ok.length === rs.length ? copyText("resync.machines.title") : copyText("resync.machines.titlePartial");
    showActionFailureToast(title, resyncMachinesSaid(rs), ok.length === rs.length ? { level: "info", durationMs: 6000 } : undefined);
    return ok;
  }

  /**
   * 那台「重新对齐」过 ⇒ 它上面的固定条逐条问一次记录还在不在（与 resume 前那一问同一个
   * `history-record`，查的是 resume 会用的那棵账号树，解析规则同 `withAccount` 跟随 / `localFollowPlan`）。
   * 没了的**标出来**（`markRecord`）、说一句；**不自动摘** —— 点那条提示才摘（`unpin`）。问不到 / 说不清查哪棵树 ⇒ 不标。
   */
  async flagPinsWithoutRecord(origin: string, pinned: Tab[], unpin: (sid: string) => void): Promise<void> {
    if (pinned.length === 0) return;
    const local = isLocalOrigin(origin);
    const state = local ? undefined : await fetchAccounts(origin).catch(() => undefined);
    const pins = local ? undefined : await lastAccounts().catch(() => undefined);
    const gone: Tab[] = [];
    for (const tab of pinned) {
      let configDir: string | undefined;
      if (local) {
        const plan = localFollowPlan(tab.sessionId);
        if (plan.kind === "pinGone") continue;
        configDir = plan.kind === "named" ? plan.configDir : undefined;
      } else {
        const lastAccount = pins?.[tab.sessionId] ?? null;
        const r = state ? resolveAccount(state, { follow: { lastAccount } }) : lastAccount ? null : ({ kind: "base" } as const);
        if (r === null || r.kind === "unavailable") continue;
        configDir = r.kind === "account" ? r.configDir : undefined;
      }
      let probe: RecordProbe;
      try {
        probe = await probeSessionRecord(origin, tab.sessionId, configDir);
      } catch {
        continue;
      }
      this.host.markRecord(tab.sessionId, probe.present);
      if (!probe.present) gone.push(tab);
    }
    if (gone.length === 0) return;
    showActionFailureToast(
      copyText("sessionState.pinGone.title"),
      copyText("sessionState.pinGone.body", { n: gone.length, names: gone.map((t) => t.title).join(", ") }),
      { level: "info", durationMs: 20_000, onClick: () => gone.forEach((t) => unpin(t.sessionId)) },
    );
  }

  /**
   * F37：手动 resume 一个已结束（灰）的 Tab。与历史浏览器 ↺ 同一套语义：
   * 本地 → 新终端窗口跑 resume（尊重 F34 自定义命令，缺省 cc 检测→claude）；
   * 远端 → F41 一键拉起 wt.exe/PowerShell 跑 `ssh -t …`，失败回退复制命令。
   * resume 成功后 CC 续写同一 jsonl，既有「会话复活」路径会自动把灰 Tab 点亮。
   */
  async resumeTab(sid: string, accountName?: string, useBase = false): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab) return;
    // 先问记录还在不在；不在 ⇒ 已经说过了，不开终端。
    // 问的是**这次要用的那个账号根**：远端在 `withAccount` 解析之后问（下面 `run` 里），本机拿本机那一份。
    const behavior = await getBehavior();
    if (isRemoteOrigin(tab.origin)) {
      // A4：带账号统一走 withAccount（点击时重解析 configDir + 记 lastAccount 源②，与 history 同口径）。
      // 本地账号切换是 A7，此处忽略（withAccount 只在远端调）。
      const origin = tab.origin;
      const cwd = tab.projectDir ?? "";
      await withAccount(
        origin,
        accountName ?? null,
        // `runRemoteResume` 现在返回 boolean（Phase G：别把失败读成成功）。这条路的
        // 反馈由它自己的 toast 承担，`withAccount` 只要 `void` ⇒ 显式丢弃。
        async (mods) => {
          if (!(await this.recordStillThere(tab, mods.configDir))) return false; // 拦下 ⇒ 不记上次的账号
          await runRemoteResume(
            origin,
            sid,
            cwd,
            await resolveResumeCommand(origin, behavior.resumeCommandRemote),
            mods,
          );
        },
        {
          sessionId: sid,
          // 要的号（显式点的 / 这条会话的 pin）选不了 ⇒ `withAccount` 自己不起、说清、给「用当前账号」的显式选择
          //   （三处先前各带一份「账号不可用」回调提示，提示完按基座起 —— 与提示说的也不一致）。
          // account-ux U3:未显式选号 → 跟随(lastAccount sticky → 当前账号 → 基座)。显式选号维持 A4。
          // audit-fixes F01(修 B1):pin 现读磁盘,不读内存镜像 accountLastByS（见 readSessionPin）。
          // F01 步骤2:useBase = 显式「用基座 resume」——不注入、不跟随(老会话住基座,别被 follow
          //   注入全局当前账号导致 claude --resume 在错数据目录找不到会话，即 #75 主因的逃生口)。
          follow: accountName || useBase ? undefined : { lastAccount: await this.readSessionPin(sid) },
        },
      );
      return;
    }
    // 本机 resume 的编排只有一份（`local-resume.ts`）：校验 sid → 铸名 → 起 → 记 pin。
    //   这里先前逐字抄着一份（连同内联的「列本机 tmux → 铸名」六行），注释里记着 #75 · #76 ·
    //   `D1 阻-1` · `D3 阻-2` 四次「这里修了、那里漏了」。账号跟随这条会话上次的号（同远端 `follow`）。
    await resumeLocalSession({
      sid,
      cwd: tab.projectDir ?? "",
      account: { kind: "follow" },
      launcher: behavior.resumeCommandLocal,
      // 记录还在不在，问的是**这次要用的那个账号根**（账号解析之后、拉起之前；远端那条在 `withAccount` 的 run 里问）。
      preflight: (configDir) => this.recordStillThere(tab, configDir),
    });
  }

  /**
   * F52：tmux 版 resume（远端专用）——在远端 tmux 会话 `cc-<sid8>` 里幂等 resume Claude。
   * 与 resumeTab 的直连版并列;本地 tab（origin===null）无 tmux 用例,直接 return。
   *
   * F04：`resumingSids` 互斥（对称 `restartingSids`）——双击之间没有互斥时，两次并发调用各自
   * 查一次陈旧快照、各自可能算出不同的 fresh tmux 名，真建出两个都声称同一 sid 的容器（R10 的
   * 一个具体、可关闭的成因）。
   *
   * F09：`accountName` 与 `resumeTab`（直连版）的参数顺序/语义对齐——此前本方法完全没有显式选号
   * 能力（`withAccount` 恒传 `null`），是 account×container 没做到真正正交的一个实现缺口
   * （旧扁平菜单从未提供"把此归档会话切到账号 X（tmux）"这一项，反映的正是这个缺口）；flyout
   * 把 account 组与 container 组做成正交修饰后，这个缺口必须补上，否则"账号=X + 容器=tmux"
   * 这个组合在 UI 上可选却在实现上是假的。
   */
  async resumeTabTmux(sid: string, accountName?: string, useBase = false): Promise<void> {
    if (this.resumingSids.has(sid)) return;
    this.resumingSids.add(sid);
    try {
      await this.resumeTabTmuxInner(sid, accountName, useBase);
    } finally {
      this.resumingSids.delete(sid);
    }
  }

  private async resumeTabTmuxInner(
    sid: string,
    accountName: string | undefined,
    useBase: boolean,
  ): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab || isLocalOrigin(tab.origin)) return;
    const behavior = await getBehavior();
    const origin = tab.origin;
    const launcher = (await resolveResumeCommand(origin, behavior.resumeCommandRemote)).trim() || AGENT_PROFILE.defaultLauncher;
    // 起法与批量「在 tmux 里后台起」同一条（那台判在不在跑 · 空 tmux 就地键入 · 铸名交一行 ccm，建完不接进去），
    //   单个只多一步：起好之后开一个终端接进去。账号照旧经 `withAccount`（显式点的号 / 跟随上次的号）。
    await withAccount(
      origin,
      accountName ?? null,
      async (mods) => {
        const item: StartItem = {
          sid,
          cwd: tab.projectDir ?? "",
          account: mods.configDir ? { kind: "named", name: mods.accountName ?? null, configDir: mods.configDir } : { kind: "base" },
          model: mods.modelOverride ?? null,
          launcher,
          defaultLauncher: AGENT_PROFILE.defaultLauncher,
        };
        return this.startInTmuxThenAttach(tab, item, () => this.resumeTabTmuxInner(sid, accountName, useBase));
      },
      {
        sessionId: sid,
        // useBase / 显式选号 = 不跟随、不注入（老会话住基座，别被跟随注入全局账号）。pin 现读磁盘（见 readSessionPin）。
        follow: accountName || useBase ? undefined : { lastAccount: await this.readSessionPin(sid) },
      },
    );
  }

  /**
   * 交那台在 tmux 里起这一个（`sessions-start`，与批量同一条），起好了（或本来就在跑）开一个终端接进去。
   * 回 `false` = 没起（记录没了 / 起不了）⇒ 调用方不记「上次用的号」。
   */
  private async startInTmuxThenAttach(tab: Tab, item: StartItem, again: () => Promise<void>): Promise<void | false> {
    const origin = tab.origin;
    let r: Reply;
    try {
      [r] = await callStart(origin, "tmux", [item]);
    } catch (e) {
      showActionFailureToast(copyText("remoteLaunchRun.inPlace.notRun"), saidOfControl(e));
      return false;
    }
    if (r.why === "record_gone") {
      this.host.markRecord(tab.sessionId, false);
      showActionFailureToast(
        copyText("sessionState.recordGone.title"),
        copyText("sessionState.recordGone.body", {
          who: isLocalOrigin(origin) ? copyText("tabSessionActions.who.local") : copyText("tabSessionActions.who.remote", { machine: origin }),
          root: r.detail,
          sid: tab.sessionId,
        }),
      );
      return false;
    }
    if (r.outcome === "failed" || r.session === null) {
      const said = sayReply(origin, "start", r);
      if (r.why === "wrong_owner") offerResyncRetry(origin, tab.sessionId, copyText("remoteLaunchRun.inPlace.notRun"), said, again);
      else showActionFailureToast(copyText("remoteLaunchRun.inPlace.notRun"), said);
      return false;
    }
    if (r.outcome === "done") this.host.markRecord(tab.sessionId, true);
    // 在跑的不止一个 ⇒ 接第一个（接回可撤销），但说出来。
    if (r.why === "ambiguous") {
      showActionFailureToast(
        copyText("tabSessionActions.dupes.title"),
        copyText("tabSessionActions.dupes.body", { n: r.detail.split(", ").length, name: r.session }),
        { level: "info", durationMs: 8000 },
      );
    }
    await runRemoteAttach(origin, r.session);
  }

  /** 本机：在那个空 tmux 里就地 resume，再开一个终端接进去（与批量同一条；账号跟随同本机 Resume）。 */
  async resumeLocalInTmux(sid: string): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab) return;
    const plan = await planStarts(tab.origin, [tab]);
    if (plan.items.length === 0) {
      showActionFailureToast(copyText("remoteLaunchRun.inPlace.notRun"), plan.skipped[0]?.why ?? "");
      return;
    }
    if ((await this.startInTmuxThenAttach(tab, plan.items[0], () => this.resumeLocalInTmux(sid))) !== false) {
      plan.record.get(sid)?.();
    }
  }

  /** A5：造一个「等该 sid compact 完成」的 awaitCompact——注册 waiter 与超时竞速，两路都清理 waiter
   *  防泄漏。resolve(true)=onLine 检测到 compact 摘要行 / resolve(false)=超时（编排器照 §5.2 不阻断、续 kill）。
   *  默认 5min（§5）。 */
  awaitCompactFor(sid: string, timeoutMs = 300_000): () => Promise<boolean> {
    return () =>
      new Promise<boolean>((resolve) => {
        let settled = false;
        const finish = (v: boolean): void => {
          if (settled) return;
          settled = true;
          this.compactWaiters.delete(sid);
          clearTimeout(timer);
          resolve(v);
        };
        this.compactWaiters.set(sid, () => finish(true));
        const timer = setTimeout(() => finish(false), timeoutMs);
      });
  }

  /** A5：活跃会话换号重启——先解析该会话当前所在的 tmux 名（send-keys/kill 目标），再走
   *  `restartWithAccount` 编排（§5）。会话不在本工具 tmux（非本工具起/已漂移）→ 提示无法重启。
   * 本机会话（`origin === null`）也走这一条，origin 取 `<local>`。 */
  async restartTabWithAccount(
    sid: string,
    accountName: string,
    compactFirst: boolean,
    confirmFn?: ConfirmFn,
  ): Promise<boolean> {
    const tab = this.host.tab(sid);
    if (!tab) return false;
    // D 审计（重要）：同一 sid 的并发重启会互相打架——A 已 kill+resume 起了新 claude，B 再 kill
    // → 把刚起来的新会话又杀了再 resume 一遍（还多弹一个终端窗口）。点击到弹确认之间有多个 await（getBehavior/list_remote_tmux/
    // fetchAccounts/checkTrust）且无反馈，双击很自然 → 在唯一入口（右键菜单的 Restart flyout；
    // ⇄ 按钮/批量对齐已随 F09 删除）上游拦住。
    if (this.restartingSids.has(sid)) {
      // F09 Phase D 审计（UX，重要）：⇄ 按钮删除前，命中这条守卫时 UI 上至少有"⇄ 立刻置灰"这个
      // 间接信号；现在右键菜单是唯一入口，点了却什么反应都没有（含最长 5 分钟的 compact 等待），
      // 用户大概率以为没点中、再点一次——给个明确提示，别让破坏性操作的
      // in-flight 防抖对用户完全不可见。
      showActionFailureToast(
        copyText("tabSessionActions.restart.busyTitle"),
        copyText("tabSessionActions.restart.busy"),
        { level: "info", durationMs: 4000 },
      );
      return false;
    }
    this.restartingSids.add(sid);
    this.host.refreshAccountBadgeFor(sid);
    try {
      return await this.restartTabWithAccountInner(sid, tab, accountName, compactFirst, confirmFn);
    } finally {
      this.restartingSids.delete(sid);
      this.host.refreshAccountBadgeFor(sid);
    }
  }

  private async restartTabWithAccountInner(
    sid: string,
    tab: Tab,
    accountName: string,
    compactFirst: boolean,
    confirmFn?: ConfirmFn,
  ): Promise<boolean> {
    // 本机会话的 origin 是 `<local>`：下面每一跳（tmux 快照 / send-keys / kill /
    // 账号清单 / 信任预检）都按 origin 分流，本机走得通；resume 那一跳在 `restartWithAccount` 里分。
    const origin = tab.origin;
    const cwd = tab.projectDir ?? "";
    const behavior = await getBehavior();
    // 这个会话在哪个 tmux 会话里：问那台（判定只在后端），一律现问。
    // 破坏性重启必须恰好命中一个在跑的（不按目录猜）；命中多个 ⇒ 拒（选错了不可逆）。
    const standing = await standingOf(origin, sid);
    if (standing?.kind === "ambiguous") {
      showActionFailureToast(
        copyText("tabSessionActions.restart.refusedTitle"),
        copyText("tabSessionActions.restart.dupes", { n: standing.names.length }),
        { level: "info", durationMs: 8000 },
      );
      return false;
    }
    const live = standing?.kind === "running" ? { name: standing.names[0] } : undefined;
    if (!live) {
      // `K-P5g`：这句话原来把**两条成因**并排摆着（「不在本工具 tmux 里」**或**「不是本工具
      // 起的」），而当时没有任何东西分得开它们。现在分得开了——`--session-accounts` 读回来的
      // 身份 token（`launchId`）说得出这条会话是不是从本工具这条路起来的，于是这里**拿它做
      // 决定**：选哪一条成因、给哪一句补救。判据见 `account-restart.ts::restartLocateFailureMessage`
      // 头注与 `accounts.vitest.ts`；本处的接线由 `tabs.vitest.ts` 那两条对照钉着。
      const msg = restartLocateFailureMessage(this.host.sessionAccount(sid), {
        local: isLocalOrigin(origin),
      });
      showActionFailureToast(msg.title, msg.body, { level: "info", durationMs: 8000 });
      return false;
    }
    return await restartWithAccount({
      origin,
      sessionId: sid,
      cwd,
      tmuxName: live.name,
      accountName,
      launcher:
        isLocalOrigin(origin)
          ? behavior.resumeCommandLocal
          : await resolveResumeCommand(origin, behavior.resumeCommandRemote),
      compactFirst,
      // `confirmFn` 保留为可选参数（批量对齐曾用 `() => true` 跳过逐会话确认，随 F09 一并删除）；
      // 唯一现存调用点（右键菜单的 Restart flyout）不传 → 仍走 restartWithAccount 自带的破坏性二次确认。
      confirm: confirmFn,
      // A5 step5：真检测器——onLine 见该 sid 的 compact 摘要行即 resolve，超时（5min）按 §5.2 续 kill。
      awaitCompact: this.awaitCompactFor(sid),
    });
  }

  /**
   * 杀死这个会话所在的 tmux 会话：二次确认后交那台（`sessions-stop`，与批量同一条：那台按 sid 认出是哪个、过三道门、杀句柄）。
   * `name` 只用来在确认框里说清是哪个（菜单就绪时那台答的）。变已结束由会话流兜（不主动改状态）。
   * `idle` = 只剩空 shell 的那个（文案别说「正在运行的 Claude」；杀掉它 ⇒ tab 变已结束 ⇒ 可 Resume）。
   */
  killInTmux(origin: string, sid: string, name: string, opts?: { confirm?: ConfirmFn; idle?: boolean }): void {
    const isLocal = isLocalOrigin(origin);
    const where = isLocal ? copyText("tabSessionActions.who.local") : copyText("tabSessionActions.who.remoteShort");
    const body = opts?.idle ? copyText("sessionState.killIdle.confirm") : copyText("tabSessionActions.kill.body", { where });
    const confirmFn: ConfirmFn = opts?.confirm ?? askConfirm;
    const machine = isLocal ? copyText("tabSessionActions.who.local") : origin;
    const message = copyText("tabSessionActions.kill.confirm", { name, machine, body, caveat: "" });
    const kill = async (): Promise<void> => {
      let r: Reply;
      try {
        [r] = await callStop(origin, [sid]);
      } catch (e) {
        showActionFailureToast(copyText("tabSessionActions.kill.failed"), saidOfControl(e));
        return;
      }
      if (r.outcome !== "done") {
        const said = sayReply(origin, "stop", r);
        // 关卡 2 拒的 ⇒ 提示带「对齐后重试」（只对这个会话重验 ＋ 重打，再过一次关卡）。
        if (r.why === "wrong_owner") offerResyncRetry(origin, sid, copyText("tabSessionActions.kill.failed"), said, kill);
        else showActionFailureToast(copyText("tabSessionActions.kill.failed"), said);
        return;
      }
      const killed = r.session ?? name;
      const who = isLocal ? copyText("tabSessionActions.who.local") : copyText("tabSessionActions.who.remote", { machine: origin });
      const done = opts?.idle
        ? copyText("sessionState.killIdle.done", { who, name: killed })
        : copyText("sessionState.killLive.done", { who, name: killed });
      // 顺手从 cc-bus 名册注销的结局说一句（没有要说的就不说；说法同单个 `kill` 那一份）。
      let bus: string | null = null;
      try {
        bus = decodeKilled(origin, killed, { session: killed, killed: true, bus: r.bus });
      } catch {
        bus = null;
      }
      showActionFailureToast(copyText("tabSessionActions.kill.done"), bus === null ? done : `${done}\n${bus}`, {
        level: "info",
        durationMs: 6000,
      });
    };
    void (async () => {
      if (!(await confirmFn(message))) return;
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
      showActionFailureToast(
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

/**
 * 拉对应终端到前台。v1.7 实现：后端查 sid_hwnd_cache + 复合指纹校验 + SetForegroundWindow。
 *
 * 失败模式（任一都会显示在 toast 上）：
 *   - "未绑定窗口"：该 session 启动时没经过 cc function 握手（直接跑 claude 而非 cc）
 *   - "窗口已不存在"：用户关掉了对应 PS/WT 窗口
 *   - "HWND 复用"：原窗口关闭后 HWND 被另一个无关窗口拿到
 *   - "切到终端窗口超时"：极端情况下 Win32 调用卡住（正文按 CP1 裁词改：不说 invoke / Win32、不说毫秒数）
 */
export function bringTerminalToFront(sessionId: string): Promise<void> {
  const timeoutMs = 5000;
  return Promise.race([
    commands.bring_terminal_to_front({ sessionId }),
    new Promise<never>((_, reject) =>
      window.setTimeout(
        () => reject(new Error(copyText("tabSessionActions.front.timeout"))),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_terminal_to_front ${sessionId} failed:`, e);
    // P4.5: 改走统一 toast stack（去掉单例 #bring-terminal-toast 的"先到先被覆盖"问题）。
    showActionFailureToast(copyText("tabSessionActions.front.failed"), String(e?.message ?? e));
  });
}

/**
 * Feature ②：拉远端 Tab 对应的本地终端窗口到前台。
 *
 * **分派与归因整条在后端**（`bind.rs::resolve_remote_front`）：
 * 先按启动令牌 `sid → token → HWND`，拉不到再走 `ccm-rbind-<sid>` 标题退路；
 * 失败时说的话只由「这个会话是不是 cc-monitor 启动的」一个布尔决定。
 *
 * ⇒ 这里原先那段 E73「失败之后再打一次 `list_remote_tmux` 分四档猜」**删了** ——
 *   它是 `§8.5 ②` 点名要收的四套判断之一，而且它把 tmux 放回了 ↗ 的前提链上
 *   （用户逐字「不能依赖 tmux」）。后端那句话原样给用户，不再在前端二次解释。
 *
 * 失败模式（后端原文）：带令牌但窗口已关 · 不是 cc-monitor 启动的 · 标题退路也没扫到 /
 * 扫到的窗口校验不过；另有 "切到终端窗口超时"（极端情况下 Win32 调用卡住）。
 */
export function bringRemoteTerminalToFront(sessionId: string): Promise<void> {
  // #41:后端现扫重试窗口抬到 4s(ON_DEMAND_BIND_*,覆盖首次 attach 的标题四跳传播),故前端超时须
  // 抬到其上、留 Win32 activate 余量——5s→8s,否则前端超时会和后端重试撞车(刚要绑上就被判超时)。
  const timeoutMs = 8000;
  return Promise.race([
    commands.bring_remote_terminal_to_front({ sessionId }),
    new Promise<never>((_, reject) =>
      window.setTimeout(
        () => reject(new Error(copyText("tabSessionActions.front.timeout"))),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_remote_terminal_to_front ${sessionId} failed:`, e);
    showActionFailureToast(copyText("tabSessionActions.front.failed"), String(e?.message ?? e));
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

