/**
 * 〔U2 · 拆 `tabs.ts` ⑤〕**对一个会话做的动作**。
 *
 * resume（直连 / tmux / 就地）· 换号重启（等 compact · 等退出）· 杀 tmux 会话 · 打开工作目录 ·
 * 在新窗口打开 · 切到终端窗口；外加 tab 层那几条零散的后端调用（忘掉会话 · 把 monitor 拉到前面 ·
 * 红绿灯快照 · DEV 探针日志）。
 *
 * # 〔C4a · 第四波 · 子步 2〕本文件不再直呼 `invoke`
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
import {
  runRemoteResume,
  runRemoteResumeTmux,
  runRemoteResumeIntoExistingTmux,
  runRemoteAttach,
} from "./remote-launch-run";
// 〔C4a · `设计/05 §8` 步 2〕本机 = `LOCAL_ORIGIN`（`"<local>"`，与 Rust `origin.rs::LOCAL` 跨语言对拍）；
// 「是不是本机」只经 `ipc/origin.ts` 判。`accounts.ts` 那个同名的 `"__local__"` 已退役 —— 全仓只剩一个本机表示。
import { isLocalOrigin, isRemoteOrigin } from "./ipc/origin";
import { commands } from "./ipc/commands";
import { probeSessionRecord, reasonOf, type RecordProbe } from "./session-reads";
import { lastAccounts } from "./history-reads";
import { listingFromFetch, mintFromListing, refuseUnmintable } from "./tmux-name-mint";
import { getBehavior } from "./behavior";
// F78：远端会话「打开工作目录」→ 用该机配置开文件窗口进入远端 cwd（而非只提示打不开）。〔F7b〕老 SFTP 面板删了。
import { openFileWindow } from "./file-window";
import {
  readRemoteConfig,
  findHostByOrigin,
  resolveResumeCommand,
} from "./remote-config";
import {
  findClaudeTmuxMatches,
  findIdleTmux,
  type TmuxSession,
} from "./tmux-sessions";
import type { Tab } from "./tab-model";
import { copyText } from "./copy-table";
import { killSession, saidOfControl } from "./tmux-control";
import { isIdentityRefusal, offerResyncRetry, resync } from "./resync";

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

/** tmux 反查缓存 TTL:菜单打开按需查,短缓存避免重复右键狂拉 ssh。 */
export const TMUX_CACHE_TTL_MS = 8000;

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
  /** 〔U4b · G1〕resume 一跳问过那台后端之后，把「记录在不在」落进这条 tab 的状态（`TabManager.markRecord`）。 */
  markRecord(sid: string, present: boolean): void;
}

export class TabSessionActions {
  /** F51：per-origin tmux 会话短缓存(反查 attach)。null=该 origin 无 tmux。 */
  readonly tmuxCache = new Map<string, { ts: number; sessions: TmuxSession[] | null }>();
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
   * 上次账号那一问（〔C4d〕本机后端 `history-last-accounts`）抛错被 main.ts 无条件覆写成空 ③刚显式钉 pin 后 10s 内还没轮询到，
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
   * 〔U4b · 第四波 · G1〕**resume 一跳先问那台后端：这条会话的记录还在不在**（`设计/01 §6.2` 最后一条
   * 「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」）。
   *
   * - 不在 ⇒ 说清查的是哪台、哪棵记录树（`01 §6.9`），tab 落「记录已不在」，返回 `false`（调用方**不开终端**）；
   * - 在 ⇒ 「记录已不在」翻回已结束（记录回来了），返回 `true`；
   * - 问不到（通道没起 / 后端太旧不认 `history-record` / 超时）⇒ 当「不知道」、返回 `true`：照今天的路走，
   *   **不许把「问不到」当成「不在」**（那会把一条其实接得上的 resume 拦掉）。
   *   〔FIX2 · `99 §2.1 ㉟②`〕照起，但说一句「查不到记录还在不在」；形状不对按 `05 §14.3` 说「两端契约对不上」
   *   （`session-reads.ts::reasonOf` 那一份，不另写）—— 出声不静默。
   *
   * 判定住那台的后端（只收 sid），这里只读答案 —— 前端不做文件存在性探测（`30 §B.6`）。
   *
   * 〔GP1 · 第四波〕`configDir` = **这次 resume 要用的那个账号配置目录**（远端：`withAccount` 解析出的 `mods.configDir`；
   * 本机：`localLaunchConfigDirSync`）。那台后端就在那棵树里找；不带（基座）⇒ 查它自己的家目录。
   * 因此这一问挪到了账号解析**之后**：改之前它先于解析、只查家目录 ⇒ 会话起在另一个账号根下时误拦（`设计/30 §8` 第 4 条）。
   */
  private async recordStillThere(tab: Tab, configDir?: string): Promise<boolean> {
    let probe: RecordProbe;
    try {
      // 〔C4c · 第四波 4B〕经通道直接问那台后端的 `history-record`（`session-reads.ts::probeSessionRecord`）。
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

  /** 〔RESYNC · V149〕tab「重新读取」：只对这个会话对齐 ＋ 从游标补读（`resync{sid}`）。补出来的行照常经流到达。 */
  async rereadTab(sid: string): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab) return;
    try {
      await resync(tab.origin, sid);
    } catch (e) {
      showActionFailureToast(copyText("tabSessionActions.reread.failed"), saidOfControl(e));
    }
  }

  /**
   * 〔RESYNC · `99 §2.1` ㉟①〕那台「重新对齐」过 ⇒ 它上面的固定条逐条问一次记录还在不在（与 resume 前那一问同一个
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
    // 〔U4b · G1〕先问记录还在不在；不在 ⇒ 已经说过了，不开终端。
    // 〔GP1〕问的是**这次要用的那个账号根**：远端在 `withAccount` 解析之后问（下面 `run` 里），本机拿本机那一份。
    const behavior = await getBehavior();
    if (isRemoteOrigin(tab.origin)) {
      // A4：带账号统一走 withAccount（点击时重解析 configDir + 记 lastAccount 源②，与 history 同口径）。
      // 本地账号切换是 A7，此处忽略（withAccount 只在远端调）。
      const origin = tab.origin;
      const cwd = tab.cwd ?? "";
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
          // 〔FE1 · D-h〕要的号（显式点的 / 这条会话的 pin）选不了 ⇒ `withAccount` 自己不起、说清、给「用当前账号」的显式选择
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
    // 〔FE1〕本机 resume 的编排只有一份（`local-resume.ts`）：校验 sid → 铸名 → 起 → 记 pin。
    //   这里先前逐字抄着一份（连同内联的「列本机 tmux → 铸名」六行），注释里记着 #75 · #76 ·
    //   `D1 阻-1` · `D3 阻-2` 四次「这里修了、那里漏了」。账号跟随这条会话上次的号（同远端 `follow`）。
    await resumeLocalSession({
      sid,
      cwd: tab.cwd ?? "",
      account: { kind: "follow" },
      launcher: behavior.resumeCommandLocal,
      // 〔GP1〕记录还在不在，问的是**这次要用的那个账号根**（账号解析之后、拉起之前；远端那条在 `withAccount` 的 run 里问）。
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
    const cwd = tab.cwd ?? "";
    // F74：先查该 origin 的 tmux 列表，据 @ccm_sid 分两路。**attach 决策对新鲜度最敏感**——
    // 8s 缓存里的 @ccm_sid 可能已被 /branch 漂移（快照记 N=A，N 此刻跑 B）→ 据陈旧快照 attach
    // 又会撞进漂移会话，正是本刀要修的 bug。故这里**总是新查、不读缓存**（用户主动 resume，一次
    // ssh 可接受；与 resolveAttachMenuItem 的 attach 一律新查对齐），查回来仍写缓存惠及其它路径。
    // 查询失败（undefined）→ 找不到活的那一个，走到下面 fresh 分支；〔FE1〕那一支要铸名，
    //   而「没问到」不是「零会话」⇒ 在那里拒、说清（先前这里 `?? null` 把两者压成一个，空集铸名 = #76）。
    const fetched = await this.fetchTmuxFresh(origin);
    const sessions = fetched ?? null;
    // ① 目标 sid 正活在某 tmux（@ccm_sid 命中）→ 直接 attach 它，回到活的后端，别重开一个。
    // F04（R10）：命中 ≥2 个时**仍 attach 到第一个**（resume 非破坏性、可撤销：重新点一次就能换
    // 目标，不像 kill 一旦选错代价不可逆），但诚实告知——不静默假装只有一个。分级理由见 F04
    // 计划 §2 取舍④。
    const matches = findClaudeTmuxMatches(sessions, sid);
    if (matches.length > 0) {
      if (matches.length > 1) {
        showActionFailureToast(
          copyText("tabSessionActions.dupes.title"),
          copyText("tabSessionActions.dupes.body", { n: matches.length, name: matches[0].name }),
          { level: "info", durationMs: 8000 },
        );
      }
      await runRemoteAttach(origin, matches[0].name);
      return;
    }
    // ①.5 audit-fixes F03（idle-tmux 就地复用）：目标 sid 的 tmux 还在（@ccm_sid 精确命中）但
    // command≠claude —— 即 claude 已退、只剩交互 shell 的**空 cc-<sid8>**。往它**就地** resume
    // （复用原会话名，不 new-session）→ 不产 `cc-<sid8>-N` 孤儿（治 #76 根因）+ 空 shell 起得了 claude
    // （治 create-gate 短路只 attach 空 shell 的 #75 一条）。仅 @ccm_sid 精确命中才复用（不按 cwd 猜，
    // 免撞同目录漂移会话）。
    // E73：明说不可 attach 的会话**不算 idle-tmux** —— 它那个「前台不是 claude」
    // 恰恰是因为里面跑着别的东西（SDK bridge 之类），不是空壳。
    const idle = this.host.isAttachable(sid) ? findIdleTmux(sessions, sid) : undefined;
    // 〔U4b · G1〕下面两支（就地 resume · 全新 resume）都要起一个新 claude 接那份记录 ⇒ 先问记录还在不在。
    //   上面那一支（attach 活会话）不问：它不起新进程。
    //   〔GP1〕问在各自 `withAccount` 解析出账号之后（`mods.configDir` 就是这次 resume 用的那棵树）。
    if (idle) {
      await withAccount(
        origin,
        accountName ?? null,
        async (mods) => {
          if (!(await this.recordStillThere(tab, mods.configDir))) return false; // 拦下 ⇒ 不记上次的账号
          await runRemoteResumeIntoExistingTmux(
            origin,
            sid,
            idle.name,
            await resolveResumeCommand(origin, behavior.resumeCommandRemote),
            mods,
          );
        },
        {
          sessionId: sid,
          // F04:useBase/显式选号 = 不跟随、不注入（与直连版 resumeTab 的基座逃生口对称，两后端
          // 一致；老会话住基座、别被 follow 注入全局账号 → #75）。
          follow: accountName || useBase ? undefined : { lastAccount: await this.readSessionPin(sid) },
        },
      );
      return;
    }
    // ② 目标会话不在任何 tmux（已结束 / 已漂移到别的 sid）→ 起**全新** resume。tmux 名从现有
    // 名里挑一个不撞的，避免复用被 /branch 漂移占着的 `<项目名>-cc`（那正是「resume 进 branch」老 bug）。
    // 🔴 `K-R96`：基名从 cwd 派生（可读），不再是 `<sid8>-cc`。
    // 〔FE1〕铸名只经 `tmux-name-mint.ts`；名单没问到 ⇒ 不铸、不起、说清（不拿空集去避让）。
    const name = mintFromListing(cwd, listingFromFetch(origin, fetched));
    if (name === null) {
      refuseUnmintable(origin, copyText("tmuxMint.unknown.notAsked"));
      return;
    }
    // account-ux U3:tmux 版归档 resume 也跟随账号(注入 configDir)。① attach 活会话分支不动(账号焊死)。
    await withAccount(
      origin,
      accountName ?? null,
      // runRemoteResumeTmux 现在返回 boolean（Phase G）；withAccount 的 run 要 Promise<void>，
      // 这条归档 resume 路径不消费成败（失败已由它自己 toast + 剪贴板回退），故丢弃返回值。
      async (mods) => {
        if (!(await this.recordStillThere(tab, mods.configDir))) return false; // 拦下 ⇒ 不记上次的账号
        await runRemoteResumeTmux(
          origin,
          sid,
          cwd,
          await resolveResumeCommand(origin, behavior.resumeCommandRemote),
          name,
          mods,
        );
      },
      {
        sessionId: sid,
        // audit-fixes F01(修 B1):pin 现读磁盘,不读内存镜像 accountLastByS（见 readSessionPin）。
        // F04:useBase/显式选号 = 不跟随、不注入（与直连版 resumeTab 的基座逃生口对称，两后端
        // 一致；老会话住基座、别被 follow 注入全局账号 → #75）。
        follow: accountName || useBase ? undefined : { lastAccount: await this.readSessionPin(sid) },
      },
    );
  }

  /**
   * ★ **`list_remote_tmux` 在 TabManager 里的唯一取数点**〔audit-0805 F14 第五刀，报告 I9′〕。
   *
   * # 为什么要收成一个
   *
   * 此前类内有**四处**各自 `invoke("list_remote_tmux")`，其中**三处顺手写了缓存、一处没写**
   * （`awaitExitFor` 的轮询 tick）—— 而那一处恰好是**唯一会反复取数的**：
   * 它每秒查一遍同一个 origin，却一次都不喂缓存。
   * ⇒ 报告 I9′ 说的「3 写 1 读」，真正的毛病不是读少，是**取数点与写缓存点没有绑在一起**：
   * 只要还能「取而不写」，下一个新增的取数点就会再漏一次。
   *
   * 收成一个之后，「取数」与「写缓存」**在语法上就是同一件事**，漏不了。
   * 判据 `tmux-cache-single-writer.vitest.ts` 钉住这一点（定框 **E3**：权威源恰好一个）。
   *
   * # 返回值三态，不许压成两态
   *
   * - `TmuxSession[]` —— 查到了，有会话
   * - `null` —— 查到了，**远端没装 tmux / 没有会话**（`NO_TMUX`）
   * - `undefined` —— **查询本身失败**（ssh 抖动）
   *
   * ⚠ 后两者必须分开：`null` 是**确定的答案**（会写进缓存），`undefined` 是**没有答案**
   * （不写缓存 —— 免得一次 ssh 抖动把 8s 内的重试全抑制掉，D-Sug3）。
   * 把它们压成一个 `null` 会让「远端确实没有会话」和「我没问到」变得无法区分。
   */
  async fetchTmuxFresh(
    origin: string,
  ): Promise<TmuxSession[] | null | undefined> {
    try {
      // ★ P3 刀 2 UI：本机走自己的读口 —— 它读的是后端推来的快照，不走 SSH
      //（`<local>` 拿去查远端配置只会报「未找到远端配置」，与真实原因毫无关系）。
      // 这就是 `C1`「差别只允许出现在传输这一跳」在读面上的样子：同一个返回类型、同一批消费者。
      const sessions =
        isLocalOrigin(origin)
          ? await commands.list_local_tmux()
          : await commands.list_remote_tmux({ origin });
      // 只缓存确定结果（成功列表 / NO_TMUX=null）；瞬时 ssh 失败不缓存，免 8s 内抑制重试（D-Sug3）。
      this.tmuxCache.set(origin, { ts: Date.now(), sessions });
      return sessions;
    } catch {
      return undefined;
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
   *  〔`A3` 第二波〕本机会话（`origin === null`）也走这一条，origin 取 `<local>`。 */
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
    // 〔`A3` 第二波〕本机会话的 origin 是 `<local>`：下面每一跳（tmux 快照 / send-keys / kill /
    // 账号清单 / 信任预检）都按 origin 分流，本机走得通；resume 那一跳在 `restartWithAccount` 里分。
    const origin = tab.origin;
    const cwd = tab.cwd ?? "";
    const behavior = await getBehavior();
    // 解析该会话当前 tmux 名，一律新查（对齐 resumeTabTmux：attach/重启对新鲜度最敏感，防据陈旧快照误伤）。
    const sessions = (await this.fetchTmuxFresh(origin)) ?? null;
    // F04（R10）：破坏性重启必须精确命中**恰好一个**同 sid 的活会话——`findClaudeTmuxMatches`
    // 不折叠成第一个。`matches.length===0` 沿用旧"无法定位"文案；`matches.length>1` 是新增的
    // 拒绝分支：错误的那次操作代价不可逆（可能杀掉了对的那个、留下错的那个继续跑），与
    // resumeTabTmux"警告+继续"的分级不同——分级理由见 F04 计划 §2 取舍④。
    const matches = findClaudeTmuxMatches(sessions, sid);
    if (matches.length > 1) {
      showActionFailureToast(
        copyText("tabSessionActions.restart.refusedTitle"),
        copyText("tabSessionActions.restart.dupes", { n: matches.length }),
        { level: "info", durationMs: 8000 },
      );
      return false;
    }
    const live = matches[0];
    // A5 阻塞修（D 审计）：破坏性重启**必须**精确命中 @ccm_sid。无 @ccm_sid 的降级远端此前会走
    // `findClaudeTmux` 的 cwd 回退（可能抓到同目录**别的** claude）→ kill 错会话 + 对目标 sid 起
    // 新进程 = 双进程 / jsonl 双写（§5.2 要防的严重态）。`findClaudeTmuxMatches` 只精确匹配、
    // 不含 cwd 回退，故 `matches` 为空即代表"未精确命中"，天然对齐这条守卫（不猜）。
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

  /** F79(#38)：杀死远端 tmux 会话——二次确认后 kill-session。变灰由 #60-A 对账兜（不主动 archive，守 §24）。
   *  @param viaCwd findClaudeTmux 是否走了 cwd 回退命中（无 @ccm_sid）——此时会话名是按目录猜的、可能
   *  不是本 tab 的会话（可能杀到同目录别的 Claude）。破坏性操作，回退命中时在确认框里加强 caveat
   *  （比 attach 的 toast 更强，因为在用户必须点的确认里）。守 F74c「保留回退+显式提示」的取舍。 */
  killRemoteTmux(
    origin: string,
    tmuxName: string,
    viaCwd: boolean,
    opts?: { confirm?: ConfirmFn; idle?: boolean; sid?: string },
  ): void {
    const caveat = viaCwd
      ? // 〔U2 · 按 `terms.json` ＋ CP1 台账改词〕不说标记、不派「重装 ccm 助手」；「可能杀到别的 Claude」这条后果必须留着。
        copyText("tabSessionActions.kill.cwdCaveat", { name: tmuxName })
      : "";
    // 可重连的 tab：claude 已退、只剩空 shell，文案别再说"正在运行的 Claude"；
    // 杀掉这个残留 tmux → tab 变成已结束 → 即可 Resume（给可重连一个出口，治 UX 审计 #1）。
    // 〔U4〕说到会话状态的句子住文案表 `sessionState.*`（原句说「转归档」「变灰」）。
    // ★ P3 刀 2 UI：本机也会走到这里 ⇒ 文案不能再写死「远端」。
    // 这不是措辞洁癖：一个说「将终止**远端**……」的确认框，用在本机会话上是**在说假话**，
    // 而它恰好是个不可恢复的破坏性动作的最后一道人工闸。
    const isLocal = isLocalOrigin(origin);
    const where = isLocal ? copyText("tabSessionActions.who.local") : copyText("tabSessionActions.who.remoteShort");
    const body = opts?.idle
      ? copyText("sessionState.killIdle.confirm")
      : copyText("tabSessionActions.kill.body", { where });
    // auto-e2e F-E4：可注入 confirm seam（对齐 account-restart.ts 的 `opts.confirm ?? askConfirm`）。
    // 〔W5-UI〕默认走应用内对话框（真 app 里 `window.confirm` 返回 Promise、恒真值 ⇒ 从前这里根本没问）。
    const confirmFn: ConfirmFn = opts?.confirm ?? askConfirm;
    const message = copyText("tabSessionActions.kill.confirm", { name: tmuxName, machine: isLocal ? copyText("tabSessionActions.who.local") : origin, body, caveat });
    const kill = async (): Promise<void> => {
      await killSession(origin, tmuxName);
      const who = isLocal ? copyText("tabSessionActions.who.local") : copyText("tabSessionActions.who.remote", { machine: origin });
      showActionFailureToast(
        copyText("tabSessionActions.kill.done"),
        opts?.idle
          ? copyText("sessionState.killIdle.done", { who, name: tmuxName })
          : copyText("sessionState.killLive.done", { who, name: tmuxName }),
        { level: "info", durationMs: 6000 },
      );
    };
    void (async () => {
      if (!(await confirmFn(message))) return;
      try {
        await kill();
      } catch (err) {
        // 〔RESYNC · V149〕关卡 2 拒的 ⇒ 提示带「对齐后重试」（只对这个会话重验 ＋ 重打，再过一次关卡）。
        if (isIdentityRefusal(err)) {
          offerResyncRetry(origin, opts?.sid, copyText("tabSessionActions.kill.failed"), saidOfControl(err), kill);
        } else {
          showActionFailureToast(copyText("tabSessionActions.kill.failed"), saidOfControl(err));
        }
      }
    })();
  }

  /** 打开指定 Tab 的 cwd。本地 → 系统文件管理器；远端 → 文件窗口进入该远端目录（F78）。无 cwd 忽略。 */
  async openTabCwd(sid: string): Promise<void> {
    const tab = this.host.tab(sid);
    if (!tab?.cwd) return;
    // F78：远端 Tab 的 cwd 是远端路径，本地 openPath 打不开——改成用该机配置开 SFTP 进入该目录
    // （Batch9-F29 曾从静默 no-op 改成 info 提示；现进一步真能浏览）。找不到该机配置才回退提示。
    if (isRemoteOrigin(tab.origin)) {
      const host = findHostByOrigin((await readRemoteConfig()).hosts, tab.origin);
      if (host && host.host.trim() !== "" && host.user.trim() !== "") {
        void openFileWindow(host, { dir: tab.cwd });
        return;
      }
      // 找到但缺 host/user = 配置不完整；没找到 = 未配置——分开措辞（审计建议）。
      const why = host
        ? copyText("tabSessionActions.openCwd.incomplete")
        : copyText("tabSessionActions.openCwd.noConfig");
      showActionFailureToast(
        copyText("tabSessionActions.openCwd.title"),
        copyText("tabSessionActions.openCwd.body", { machine: tab.origin, cwd: tab.cwd, why }),
        { level: "info" },
      );
      return;
    }
    try {
      await openPath(tab.cwd);
    } catch (e) {
      console.warn(`[tabs] openPath ${tab.cwd} failed:`, e);
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
 *   - "切到终端窗口超时"：极端情况下 Win32 调用卡住（〔S4〕正文按 CP1 裁词改：不说 invoke / Win32、不说毫秒数）
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
 * 〔`设计/80 §8.7` 步 4，第二波 T4〕**分派与归因整条在后端**（`bind.rs::resolve_remote_front`）：
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

