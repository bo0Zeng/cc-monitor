/**
 * 〔FE1 · 第四波 4D〕**本机 resume 一条会话 —— 编排只有这一份。**
 *
 * 守的要求：`设计/01 §5` D1「**一个判定只有一个家**」（「同一条规则有两份实现，它们就会漂；而漂开的后果是静默的错，不是报错」）。
 *
 * # 它替掉了哪四份
 *
 * | 入口 | 账号 | tmux 名 |
 * |---|---|---|
 * | tab 栏右键 Resume（`tab-session-actions.ts::resumeTab`） | 跟随：这条会话上次的号 → 当前号 | 铸 |
 * | 历史页 ↺（`views/history.ts::runResume`） | 跟随 | 铸 |
 * | 分叉之后起新会话（`fork-flow.ts` 的 `startLocal`） | 用户在小窗里点的（账号 0 / 具名） | 铸 |
 * | 本机换号重启的 resume 那一跳（`account-restart.ts`） | 用户在菜单里点的具名号 | 复用被 kill 让出来的旧名 |
 *
 * 四份各写一遍「校验 sid → 读行为设置 → 铸名 → `resume_history_session` → 记 pin」，
 * 注释里记着四次「这里修了、那里漏了」（#75 · #76 · `D1 阻-1` · `D3 阻-2`）。
 *
 * # 失败怎么说
 *
 * 本函数**自己出声**并回 `false`，调用方据此收尾（换号重启说「旧会话已结束、新会话没起来」；
 * 分叉回 `failed`，不再叠成功提示）。构造失败（sid 不合法）与拉起失败（后端回错）分两句，
 * 与远端 `runRemoteResume` 的两分对齐。
 *
 * # 时序（量出来的，别加 `await`）
 *
 * 取账号那一跳是**同步**读快照（`accounts.ts::localLaunchAccountSync` 头注：两条 DOM 判据只放行一个微任务）；
 * 本函数的 `await` 只有「行为设置（调用方给了就不读）→ tmux 名单（给了名字就不问）→ 拉起」这几拍。
 */
import { commands } from "./ipc/commands";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { explicitLocalAccountWire, localFollowPlan, primeLocalLaunchAccounts, recordLocalLaunchAccount, refuseUnavailableAccount, type LocalAccountWire } from "./launch-account";
import { validateLocalLaunch } from "./launch-requests";
import { getBehavior } from "./behavior";
import { mintFromListing, readTmuxListing } from "./tmux-name-mint";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";

/** 这次本机 resume 用哪个号。 */
export type LocalResumeAccount =
  /** 跟随：这条会话上次用的号（pin）→ 当前号；说不出就缺席（逐字节旧行为）。起成了记 pin。 */
  | { kind: "follow" }
  /**
   * 用户显式选的：`configDir: null` = 账号 0（显式 `base`，不是省略）；
   * `name: null` = 说不出名字（分叉继承的是源会话的目录）。
   */
  | { kind: "explicit"; configDir: string | null; name: string | null };

export interface LocalResumeRequest {
  sid: string;
  cwd: string;
  account: LocalResumeAccount;
  /** 给了就用它（换号重启：复用被 kill 让出来的旧名）；缺席 ⇒ 问本机 tmux 名单现铸一个。 */
  tmuxName?: string;
  /** 设置里的本机 resume 命令（调用方已经读过就给；空串 = 没配 ⇒ 后端用默认）。缺席 ⇒ 现读。 */
  launcher?: string;
  /** 拉起失败那句提示的标题；缺席 ⇒ 「恢复失败」。 */
  failureTitle?: string;
  /**
   * 账号定下来之后、拉起之前问一句（〔GP1〕tab 栏那条：「记录还在不在」要问**这次要用的那个账号根**）。
   * 收到的是这次要用的 `CLAUDE_CONFIG_DIR`（`undefined` = 基座 / 没表态）；回 `false` ⇒ 不起（它自己已经说过了）。
   */
  preflight?: (configDir: string | undefined) => Promise<boolean>;
}

/**
 * 在本机把一条会话 resume 起来。
 *
 * @returns 真拉起来了才 `true`；失败时已经出过声。
 */
export async function resumeLocalSession(req: LocalResumeRequest): Promise<boolean> {
  // `D1 阻-1`：**不等待**地把账号快照踢一脚（等它就多一拍）。只有跟随那一态读快照。
  if (req.account.kind === "follow") primeLocalLaunchAccounts();
  try {
    // F06：sid 校验先于任何 IPC 往返。
    validateLocalLaunch({ kind: "resume", sid: req.sid }, req.cwd);
  } catch (err) {
    showActionFailureToast(copyText("localResume.build.failed"), String(err));
    return false;
  }
  // 🔴 〔FE1 · D-h〕跟随时，这条会话的 pin 那个号选不了 ⇒ **不起**：说清、给「用当前账号」的显式选择
  //   （点了就以**显式**选号再起一次，起成了记 pin —— 与远端 `withAccount` 显式那一支同语义）。
  //   先前这一形落成「缺席」⇒ 落 shell rc 里的默认号，不说一个字（E7 的本机那一形）。
  const plan = req.account.kind === "follow" ? localFollowPlan(req.sid) : null;
  if (plan?.kind === "pinGone") {
    const alt = plan.alternative;
    refuseUnavailableAccount({
      machine: LOCAL_ORIGIN,
      name: plan.pin,
      pinned: true,
      listKnown: plan.listKnown,
      alternative: alt?.name ?? null,
      choose: async () => {
        const ok = await resumeLocalSession({
          ...req,
          account: { kind: "explicit", configDir: alt?.configDir ?? null, name: alt?.name ?? null },
        });
        if (ok && alt) recordLocalLaunchAccount(req.sid, alt.name);
      },
    });
    return false;
  }
  if (req.preflight) {
    const configDir =
      req.account.kind === "follow"
        ? plan?.kind === "named"
          ? plan.configDir
          : undefined
        : (req.account.configDir ?? undefined);
    if (!(await req.preflight(configDir))) return false;
  }
  try {
    const launcher = req.launcher ?? (await getBehavior()).resumeCommandLocal;
    // ★★ `K-R46`：名字要算出来传下去 —— 后端**故意**拒绝自己铸名（`history.rs` 的 `NO_TMUX_NAME`），
    //    不传 ⇒ 后端如实走不进容器的旧路。名单不知道 ⇒ `null`（绝不退化成空集，#76）。
    const tmuxName = req.tmuxName ?? mintFromListing(req.cwd, await readTmuxListing(LOCAL_ORIGIN));
    const accountWire: LocalAccountWire | undefined =
      req.account.kind === "follow"
        ? plan?.kind === "named"
          ? plan.wire
          : undefined
        : explicitLocalAccountWire(req.account.configDir, req.account.name);
    await commands.resume_history_session({
      sessionId: req.sid,
      cwd: req.cwd,
      launcher: launcher.trim() === "" ? null : launcher,
      tmuxName,
      account: accountWire,
    });
    // `D3 阻-2`：本机这条路也往 pin 里写（跟随那一态；显式那一态由调用方按自己的语义记 ——
    //   换号重启只在 kill ＋ resume 全成之后才记，分叉是新会话、不记）。⚠ 不等待。
    if (plan?.kind === "named") recordLocalLaunchAccount(req.sid, plan.name);
    return true;
  } catch (err) {
    showActionFailureToast(req.failureTitle ?? copyText("localResume.launch.failed"), String(err), {
      level: "error",
      durationMs: 10000,
    });
    return false;
  }
}
