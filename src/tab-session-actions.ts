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
import { openPath } from "@tauri-apps/plugin-opener";
import {
  restartLocateFailureMessage,
  withAccount,
  type SessionAccount,
  localLaunchAccountSync,
  localLaunchAccountNameSync,
  recordLocalLaunchAccount,
  primeLocalLaunchAccounts,
} from "./accounts";
import { restartWithAccount, DEFAULT_EXIT_WAIT_MS } from "./account-restart";
import { validateLocalLaunch } from "./launch-requests";
import { showActionFailureToast } from "./error-toast";
import {
  runRemoteResume,
  runRemoteResumeTmux,
  runRemoteResumeIntoExistingTmux,
  runRemoteAttach,
} from "./remote-launch-run";
// 〔C4a · `设计/05 §8` 步 2〕本机 = `LOCAL_ORIGIN`（`"<local>"`，与 Rust `origin.rs::LOCAL` 跨语言对拍）；
// 「是不是本机」只经 `ipc/origin.ts` 判。`accounts.ts` 那个同名的 `"__local__"` 已退役 —— 全仓只剩一个本机表示。
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN } from "./ipc/origin";
import { commands } from "./ipc/commands";
import { mintSessionTmuxName } from "./remote-launch";
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
  claudeExited,
  type TmuxSession,
} from "./tmux-sessions";
import type { Tab } from "./tab-model";
import { copyText } from "./copy-table";

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
        title: tab.title,
        ...(screenX !== undefined && screenY !== undefined
          ? { x: screenX, y: screenY }
          : {}),
      });
    } catch (e) {
      showActionFailureToast("打开新窗口失败", String(e));
    }
  }

  /**
   * audit-fixes F01（修 B1，full-audit 阻塞）：resume 前**现读磁盘** pin，不读内存镜像
   * `accountLastByS`。后者是 tab 徽章的 10s 刷新数据源，在①启动首轮刷新前（空 Map）②
   * `list_last_accounts` 抛错被 main.ts 无条件覆写成空 ③刚显式钉 pin 后 10s 内还没轮询到，
   * 这三种窗口里读它 → `withAccount` 的不-clobber 守卫拿到假 priorPin=null → 把磁盘真实
   * pin 静默覆盖成全局当前账号。与 history.ts:1489 的「现读」同口径，三处 resume 一致。
   * 读不到（无 pin / 查询失败）→ undefined → withAccount 落全局账号/基座，与旧行为一致
   * （区别只是不再"错误地覆盖"既有 pin）。
   */
  private async readSessionPin(sid: string): Promise<string | undefined> {
    try {
      const map = await commands.list_last_accounts();
      return map?.[sid];
    } catch {
      return undefined;
    }
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
          // audit-fixes F07（I 建议）：显式选号解析不到（登出/目录消失且缓存恰过期）→ 提示而非静默
          // 落基座（对齐 history.ts:1502；此前 resumeTab 缺此回调，用户明点的"用账号 X resume"被无声吞掉）。
          onUnselectable: (n) =>
            showActionFailureToast(
              "账号不可用",
              `账号「${n}」当前不可选（未登录 / 非隔离 / 目录缺失），改用该会话上次的账号 / 当前账号 resume。`,
              { level: "info", durationMs: 6000 },
            ),
          // account-ux U3:未显式选号 → 跟随(lastAccount sticky → 当前账号 → 基座)。显式选号维持 A4。
          // audit-fixes F01(修 B1):pin 现读磁盘,不读内存镜像 accountLastByS（见 readSessionPin）。
          // F01 步骤2:useBase = 显式「用基座 resume」——不注入、不跟随(老会话住基座,别被 follow
          //   注入全局当前账号导致 claude --resume 在错数据目录找不到会话，即 #75 主因的逃生口)。
          follow: accountName || useBase ? undefined : { lastAccount: await this.readSessionPin(sid) },
        },
      );
      return;
    }
    try {
      // F06：走一遍本地 IR 构造，sid 校验先于 resume_history_session 这次 invoke（不代表本函数
      // 此前完全没有过 IPC——上面 `getBehavior()` 已经读过一次 config；构造失败与拉起失败分两个
      // catch，headline 对齐远端 `runRemoteResume` 的"无法构造 resume 命令"/"拉起失败"两分）。
      validateLocalLaunch({ kind: "resume", sid }, tab.cwd ?? "");
    } catch (err) {
      showActionFailureToast("无法构造 resume 命令", String(err));
      return;
    }
    // ★★ P3t-Y2b：本机 resume 也进 tmux（POSIX；Windows 那侧后端不读这个名字，`C12`）。
    //
    // 名字**必须**由 `mintSessionTmuxName` 铸 —— 它 = 基名 `<项目名>-cc` + `mintTmuxName` 的避让，
    // 而 `mintTmuxName` 是全仓唯一带撞名避让的铸造口（F13）。
    // Rust 侧刻意拒绝自己铸名：在那边补一个默认值就是 F13 修掉的坑第三次。
    //
    // ⚠ **这里第一版直接写了 `mintTmuxName(`${sid.slice(0,8)}-cc`, …)`** —— 那等于**又抄了一份
    // 基名规则**，正是 F13 收敛掉的那个重复（我在上一句里刚写完「唯一铸造口」）。
    // `session_name_registry` 当场判红（它数的就是「谁在产 `-cc` 基名」）。⇒ 改用现成的那个。
    // 🔴 `K-R96`（用户 09-12 `R55`「要是可读的名字 / 不要id」）：基名从 `<sid8>-cc`
    //    换成 `<项目名>-cc`（从 cwd 派生）⇒ 这里要给的是 **cwd**，不是 sid。
    //    sid 一格没丢：它骑在 `@ccm_sid` 上（后端建会话时 `set-option` 写），
    //    而本仓认会话从来就只问那个、不问名字前缀。
    //
    // `existing` 从 `commands.list_local_tmux()` 来（就在下面几行）。
    // 〔K-R19 订正 09-03〕这里原先写的是 `local_tmux_names()`，**全仓零定义**：
    // 真名从来就是 `list_local_tmux`（Rust 侧 `tmux.rs::list_local_tmux`）。
    // ⚠ 它回 `null` 表示**不知道**（本机后端通道
    // 没起 / 还没推过帧），不是「一个名字都没占」。不知道的时候**不铸名**、不传 `tmuxName`
    // ⇒ 后端诚实降级回旧路（不进容器）。硬要铸就是「不避让」，那正是 issue #76
    //「静默接进第一个会话，而用户以为开了新的」。
    // `D1 阻-1`：**不等待**地把账号快照踢一脚（等它就多一拍，见那个取值口的头注）。
    primeLocalLaunchAccounts();
    let tmuxName: string | null = null;
    try {
      const sessions = await commands.list_local_tmux();
      if (sessions)
        tmuxName = mintSessionTmuxName(tab.cwd ?? "", new Set(sessions.map((s) => s.name)));
    } catch {
      // 读不到就当不知道 —— 与上面同一条纪律，绝不退化成空集。
      tmuxName = null;
    }
    try {
      // ★★ `K-H2b` `D1 阻-1`：**账号这一格先前是空的** —— 这条是 tab 栏那条主路，
      //    而它一个账号都不传 ⇒ ① 起会话落到 shell rc 里那个默认号上（静默串号）；
      //    ② 中转那一格永远拼不出路由键（没有账号 id ⇒ 不注入）。
      //    取值口只有一个（`accounts.ts::localLaunchAccountSync`，就在下面几行调着）：
      //    〔K-R19 订正 09-03〕原先写的是 `resolveLocalLaunchAccount`，**全仓零定义**；
      //    钉这件事的那条判据（`commands.vitest.ts`）逐字写的就是 `localLaunchAccountSync`。
      //    resume 走那条会话上次的 pin，
      //    说不出就**缺席**（逐字节旧行为），绝不回落到「当前账号」——那是 #75 的形状。
      await commands.resume_history_session({
        sessionId: sid,
        cwd: tab.cwd ?? "",
        launcher: behavior.resumeCommandLocal || null,
        tmuxName,
        account: localLaunchAccountSync(sid),
      });
      // `D3 阻-2`：**本机这条路也要往 pin 里写** —— 在此之前 `recordLastAccount` 的两个
      //   生产调用点结构上只走远端 ⇒ 本机 `list_last_accounts` 恒空 ⇒ 上面那句「pin 优先」
      //   在本机永远走不到。⚠ 不等待（多一拍会撞那两条只放行一个微任务的 DOM 判据）。
      recordLocalLaunchAccount(sid, localLaunchAccountNameSync(sid));
    } catch (err) {
      showActionFailureToast("恢复失败", String(err));
    }
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
    // 查询失败（undefined）→ 当作没有会话，走下面 fresh 分支（沿用旧幂等 resume 名，退化不变砖）。
    const sessions = (await this.fetchTmuxFresh(origin)) ?? null;
    // ① 目标 sid 正活在某 tmux（@ccm_sid 命中）→ 直接 attach 它，回到活的后端，别重开一个。
    // F04（R10）：命中 ≥2 个时**仍 attach 到第一个**（resume 非破坏性、可撤销：重新点一次就能换
    // 目标，不像 kill 一旦选错代价不可逆），但诚实告知——不静默假装只有一个。分级理由见 F04
    // 计划 §2 取舍④。
    const matches = findClaudeTmuxMatches(sessions, sid);
    if (matches.length > 0) {
      if (matches.length > 1) {
        showActionFailureToast(
          "检测到多个同身份会话",
          `该会话身份（sid）同时活在 ${matches.length} 个 tmux 里，本次接入其中一个（${matches[0].name}）；建议手动到终端核实其余会话是否需要清理。`,
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
    if (idle) {
      await withAccount(
        origin,
        accountName ?? null,
        async (mods) => {
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
          // F09：显式选号解析不到 → 提示而非静默落基座（对齐 resumeTab 的同类回调）。
          onUnselectable: (n) =>
            showActionFailureToast(
              "账号不可用",
              `账号「${n}」当前不可选（未登录 / 非隔离 / 目录缺失），改用该会话上次的账号 / 当前账号 resume。`,
              { level: "info", durationMs: 6000 },
            ),
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
    const existing = new Set((sessions ?? []).map((s) => s.name));
    const name = mintSessionTmuxName(cwd, existing);
    // account-ux U3:tmux 版归档 resume 也跟随账号(注入 configDir)。① attach 活会话分支不动(账号焊死)。
    await withAccount(
      origin,
      accountName ?? null,
      // runRemoteResumeTmux 现在返回 boolean（Phase G）；withAccount 的 run 要 Promise<void>，
      // 这条归档 resume 路径不消费成败（失败已由它自己 toast + 剪贴板回退），故丢弃返回值。
      async (mods) => {
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
        // F09：同上——显式选号解析不到时提示而非静默落基座。
        onUnselectable: (n) =>
          showActionFailureToast(
            "账号不可用",
            `账号「${n}」当前不可选（未登录 / 非隔离 / 目录缺失），改用该会话上次的账号 / 当前账号 resume。`,
            { level: "info", durationMs: 6000 },
          ),
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
        origin === LOCAL_ORIGIN
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

  /**
   * A5+ 优雅退出等待器：轮询该 origin 的 tmux 列表，`claudeExited` 报「目标 sid 前台不再是 claude」
   * 即 resolve(true)；`timeoutMs`（默认 DEFAULT_EXIT_WAIT_MS=10s）到仍未退出 → resolve(false)（编排器
   * 据此降级 kill）。list 失败当「未知」跳过本轮（不误判已退出）。注入 `restartWithAccount.awaitExit`。
   */
  private awaitExitFor(
    origin: string,
    cwd: string,
    sid: string,
    timeoutMs = DEFAULT_EXIT_WAIT_MS,
    pollMs = 1000,
  ): () => Promise<boolean> {
    return () =>
      new Promise<boolean>((resolve) => {
        let stopped = false;
        let pollTimer: ReturnType<typeof setTimeout> | undefined;
        const stop = (v: boolean): void => {
          if (stopped) return;
          stopped = true;
          clearTimeout(timer);
          if (pollTimer) clearTimeout(pollTimer); // 清掉挂起的下一轮轮询，干净收尾
          resolve(v);
        };
        const timer = setTimeout(() => stop(false), timeoutMs);
        const tick = async (): Promise<void> => {
          if (stopped) return;
          // ★ F14：走唯一取数点 ⇒ **这一轮轮询顺带把缓存刷新了**。
          // 此前这里是四处取数点里唯一不写缓存的一处，而它恰好是唯一会反复取数的。
          const got = await this.fetchTmuxFresh(origin);
          const ok = got !== undefined; // 查询失败 → 本轮跳过（不误判已退出）
          const sessions = ok ? got : null;
          if (stopped) return;
          if (ok && claudeExited(sessions, sid, cwd)) {
            stop(true);
            return;
          }
          pollTimer = setTimeout(() => void tick(), pollMs);
        };
        void tick();
      });
  }

  /** A5：活跃会话换号重启——先解析该会话当前所在的 tmux 名（send-keys/kill 目标），再走
   *  `restartWithAccount` 编排（§5）。会话不在本工具 tmux（非本工具起/已漂移）→ 提示无法重启。
   *  〔`A3` 第二波〕本机会话（`origin === null`）也走这一条，origin 取 `<local>`。 */
  async restartTabWithAccount(
    sid: string,
    accountName: string,
    compactFirst: boolean,
    confirmFn?: (msg: string) => boolean,
  ): Promise<boolean> {
    const tab = this.host.tab(sid);
    if (!tab) return false;
    // D 审计（重要）：同一 sid 的并发重启会互相打架——A 已 kill+resume 起了新 claude，B 的
    // awaitExit 看到新 claude 仍在 → 超时降级 kill → 把刚起来的新会话又杀了再 resume 一遍
    // （还多弹一个终端窗口）。点击到弹确认之间有多个 await（getBehavior/list_remote_tmux/
    // fetchAccounts/checkTrust）且无反馈，双击很自然 → 在唯一入口（右键菜单的 Restart flyout；
    // ⇄ 按钮/批量对齐已随 F09 删除）上游拦住。
    if (this.restartingSids.has(sid)) {
      // F09 Phase D 审计（UX，重要）：⇄ 按钮删除前，命中这条守卫时 UI 上至少有"⇄ 立刻置灰"这个
      // 间接信号；现在右键菜单是唯一入口，点了却什么反应都没有（含最长 5 分钟的 compact 等待+
      // 10 秒退出等待窗口），用户大概率以为没点中、再点一次——给个明确提示，别让破坏性操作的
      // in-flight 防抖对用户完全不可见。
      showActionFailureToast(
        "正在重启中",
        "该会话上一次换号重启还没完成，请稍候再试。",
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
    confirmFn?: (msg: string) => boolean,
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
        "换号重启拒绝",
        `该会话身份（sid）同时活在 ${matches.length} 个 tmux 里，无法安全判定该重启哪一个——请到终端手动核实后再试。`,
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
      // 决定**：选哪一条成因、给哪一句补救。判据见 `accounts.ts::restartLocateFailureMessage`
      // 头注与 `accounts.vitest.ts`；本处的接线由 `tabs.vitest.ts` 那两条对照钉着。
      const msg = restartLocateFailureMessage(this.host.sessionAccount(sid), {
        local: origin === LOCAL_ORIGIN,
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
        origin === LOCAL_ORIGIN
          ? behavior.resumeCommandLocal
          : await resolveResumeCommand(origin, behavior.resumeCommandRemote),
      compactFirst,
      // `confirmFn` 保留为可选参数（批量对齐曾用 `() => true` 跳过逐会话确认，随 F09 一并删除）；
      // 唯一现存调用点（右键菜单的 Restart flyout）不传 → 仍走 restartWithAccount 自带的破坏性二次确认。
      confirm: confirmFn,
      // A5 step5：真检测器——onLine 见该 sid 的 compact 摘要行即 resolve，超时（5min）按 §5.2 续 kill。
      awaitCompact: this.awaitCompactFor(sid),
      // A5+ 优雅退出：轮询 tmux 前台不再是 claude 即 resolve，10s 超时按 §5.2 ④ 降级 kill。
      awaitExit: this.awaitExitFor(origin, cwd, sid),
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
    opts?: { confirm?: (message: string) => boolean; idle?: boolean },
  ): void {
    const caveat = viaCwd
      ? // 〔U2 · 按 `terms.json` ＋ CP1 台账改词〕不说标记、不派「重装 ccm 助手」；「可能杀到别的 Claude」这条后果必须留着。
        `\n\n⚠ 认不出这是哪个会话：「${tmuxName}」是按工作目录匹配到的，可能是同目录里另一个正在运行的 Claude。`
      : "";
    // 可重连的 tab：claude 已退、只剩空 shell，文案别再说"正在运行的 Claude"；
    // 杀掉这个残留 tmux → tab 变成已结束 → 即可 Resume（给可重连一个出口，治 UX 审计 #1）。
    // 〔U4〕说到会话状态的句子住文案表 `sessionState.*`（原句说「转归档」「变灰」）。
    // ★ P3 刀 2 UI：本机也会走到这里 ⇒ 文案不能再写死「远端」。
    // 这不是措辞洁癖：一个说「将终止**远端**……」的确认框，用在本机会话上是**在说假话**，
    // 而它恰好是个不可恢复的破坏性动作的最后一道人工闸。
    const isLocal = origin === LOCAL_ORIGIN;
    const where = isLocal ? "本机" : "远端";
    const body = opts?.idle
      ? copyText("sessionState.killIdle.confirm")
      : `将终止${where}这个 tmux 会话里正在运行的 Claude，未保存的交互会中断。`;
    // auto-e2e F-E4：可注入 confirm seam（对齐 account-restart.ts 的 `opts.confirm ?? window.confirm`）。
    // 默认（不传 opts）走 `window.confirm`，交互零变化——headless e2e/DEV 才注入 ()=>true/false。
    const confirmFn = opts?.confirm ?? ((m: string) => window.confirm(m));
    const ok = confirmFn(
      `杀死会话「${tmuxName}」（机器 ${isLocal ? "本机" : origin}）？\n\n${body}\n此操作不可恢复。${caveat}`,
    );
    if (!ok) return;
    void (async () => {
      try {
        await commands.kill_remote_tmux({ origin, target: tmuxName });
        const who = isLocal ? "本机" : `远端 [${origin}]`;
        showActionFailureToast(
          "已杀死会话",
          opts?.idle
            ? copyText("sessionState.killIdle.done", { who, name: tmuxName })
            : copyText("sessionState.killLive.done", { who, name: tmuxName }),
          { level: "info", durationMs: 6000 },
        );
      } catch (err) {
        showActionFailureToast("杀死会话失败", String(err));
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
        ? "该机的远端配置缺 host / user（在设置 → 连接 补全后可用）"
        : "未找到该机的远端配置（在设置 → 连接 添加后可用）";
      showActionFailureToast(
        "远端目录无法本地打开",
        `该会话在远端机器 [${tab.origin}]，工作目录 ${tab.cwd} 不在本机；${why}。`,
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
        () => reject(new Error("切到终端窗口超时")),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_terminal_to_front ${sessionId} failed:`, e);
    // P4.5: 改走统一 toast stack（去掉单例 #bring-terminal-toast 的"先到先被覆盖"问题）。
    showActionFailureToast("切到终端窗口失败", String(e?.message ?? e));
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
        () => reject(new Error("切到终端窗口超时")),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_remote_terminal_to_front ${sessionId} failed:`, e);
    showActionFailureToast("切到终端窗口失败", String(e?.message ?? e));
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

/** issue #23：红绿灯快照（启动 / F5 后拉一次做初始收敛）。 */
export function listSessionActivity(): Promise<
  { session_id: string; status: string | null; waiting_for: string | null }[]
> {
  return commands.list_session_activity();
}
