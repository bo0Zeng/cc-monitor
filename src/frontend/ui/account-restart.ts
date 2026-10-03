// A5：换号破坏性重启会话的编排（DESIGN §5）。活跃会话 → 杀旧进程 + 用新账号 resume 同一 sid。
// 本机会话也走这一条（`origin = <local>`），只在第⑤步换成本机那一跳。
// **破坏性**（中断当前回合）。失败语义严格照 §5.2：compact 失败/超时**不阻断**；kill 失败**必须中止**、
// 绝不续 resume（否则新旧两个进程抢同一会话）。在 A4 的账号解析/记账之上插入 [compact]→kill→resume。
// 依赖经 import（vitest 可 vi.mock）；confirm / awaitCompact 两个交互点可注入，便于纯逻辑单测。
//
// **为何另起、不复用 A4 的 `withAccount`**（D 架构审计裁定，防下轮重新纠结）：两者语义天然不兼容——
//   ① 不可选账号时：restart **中止**（破坏性重启绝不能退化用默认号）。withAccount 今天同样不起
//      （先前是「降级默认起」），但它的出口是「说清 ＋ 给一个显式选择再起」，restart 这里没有那一步。
//   ② 记 lastAccount 条件：withAccount run 后**无条件**记；restart **仅 kill+resume 全成后**才记
//      （kill 失败提前 return、绝不记，见 §5.2 + vitest ④）。硬合需给 withAccount 加 abort-vs-degrade /
//      条件记账 / run 前置 compact&kill 钩子三个开关，复杂度净增、收益为负。二者已共用 accounts.ts
//      **同一批原语**（fetchAccounts / accountConfigDir / recordLastAccount），无逻辑漂移。故维持分离。
// 换号重启的是标签页里的会话 ⇒ 流跟的那一家。
import { ACTIVE_AGENT } from "./agent-profile";
import { askConfirm, type ConfirmFn } from "./ask-dialog";
import { killSession, saidOfControl, sendKeys } from "./tmux-control";
import { isIdentityRefusal, offerResyncRetry } from "./resync";
import { runRemoteResumeTmuxAndWait } from "./remote-launch-run";
import { accountConfigDir, type SessionAccount } from "./accounts";
import { fetchAccounts, checkTrust } from "./account-reads";
import { getModelForAccount } from "./account-prefs";
import { recordLastAccount } from "./launch-account";
import { showActionFailureToast } from "./error-toast";
// 本机那一侧：`origin` 是 backend 的 `<local>`（账号面那个第二种写法已退役，只剩这一个）。
// 〔审计 F 🔴-5〕「是不是本机」只经 `ipc/origin.ts` 判，这里不再自己比常量。
import { isLocalOrigin } from "./ipc/origin";
// 本机那一跳走 resume 编排的唯一一份（原 `account-restart-local.ts` 并进去了）。
import { resumeLocalSessionAndWait } from "./local-resume";
import { copyText } from "./copy-table";

export interface RestartWithAccountOpts {
  origin: string;
  sessionId: string;
  cwd: string;
  /** 本工具的会话名（send-keys / kill 目标；后端的身份门对发按键与结束会话都只认 `*-cc`
   *  ——audit-fixes F02 后 kill 也对称加了守卫）。此前这里点的是 monitor 的 `tmux_send_keys` /〔散文墓碑〕
   *  `kill_remote_tmux`〔散文墓碑〕；今天两件经 `src/frontend/ui/tmux-control.ts` 直接问那台机器的后端，门只在后端。
   *
   *  ⚠ 这两句**原本都写反了**：写的是「`cc-<sid8>` 会话名」＋
   *  「白名单都只认 `cc-*`」。真实形状是 **`<X>-cc` 后缀**（S4b-3b，用户 2026-07-31 把
   *  `cc-` 前缀反转成 `-cc` 后缀），白名单认的是 `*-cc`（老的 `cc-*` 前缀仍兼容认，
   *  但那是**向后兼容**，不是今天产的形状 —— 判定住后端 `control/gate_rules.rs::is_ccm_tmux_name`）。
   *  ⚠ 而 `<X>` 今天也不是 `<sid8>` 了：`K-R96` 之后是 **`<项目名>`**
   *  （用户 `R55`：「要是可读的名字 / 不要id」）。sid 骑在 `@ccm_sid` 上，不在名字里。 */
  tmuxName: string;
  accountName: string;
  /** F34 远端 resume 命令（空 → 后端默认）。 */
  launcher: string;
  /** ③ 是否先在【旧账号】上 /compact（默认 false）。 */
  compactFirst: boolean;
  // —— 可注入点（默认走真实实现；测试注入 mock）——
  confirm?: ConfirmFn;
  /** 等 compact 完成：resolve(true)=检测到完成 / resolve(false)=超时放弃。省略 → 有界延时兜底。 */
  awaitCompact?: () => Promise<boolean>;
}

/** compact 兜底等待上限（无注入检测器时）。超时按 §5.2 不阻断、继续重启。 */
export const DEFAULT_COMPACT_WAIT_MS = 90_000;

function delay(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/**
 * @returns 是否真的走完 kill+resume（true=已用新账号 resume；false=任一前置中止：账号不可选 /
 * 用户取消 / kill 失败）。account-ux U6 的批量对齐（随 F09 一并删除）曾据此汇总真实成败——
 * 唯一现存调用点（tabs.ts 右键菜单的 Restart flyout）忽略返回值，语义不受影响。
 */
export async function restartWithAccount(opts: RestartWithAccountOpts): Promise<boolean> {
  const { origin, sessionId, cwd, tmuxName, accountName, launcher } = opts;

  // ① 预检：解析 configDir（顺带校验可选）。不可选 → 明确提示、不动手（§5.2 ①）。
  const state = await fetchAccounts(origin);
  const configDir = accountConfigDir(state, accountName);
  if (!configDir) {
    showActionFailureToast(
      copyText("accountRestart.unselectable.title"),
      copyText("accountRestart.unselectable.body", { name: accountName }),
      { level: "info", durationMs: 6000 },
    );
    return false;
  }
  // trust 只警告不阻断（§5 ①）。
  let trustWarn = "";
  try {
    const t = await checkTrust(origin, configDir, cwd);
    if (t.available && t.known && !t.trusted) {
      trustWarn = copyText("accountRestart.confirm.trustWarn");
    }
  } catch {
    /* trust 查询失败不影响主流程 */
  }

  // ② 破坏性二次确认。
  // 默认走应用内对话框：真 app 里 `window.confirm` 是插件注入的 async 替身（返回 Promise，恒真值）。
  const confirmFn: ConfirmFn = opts.confirm ?? askConfirm;
  const msg =
    copyText("accountRestart.confirm.body", { name: accountName, tmuxName, compact: (opts.compactFirst
      ? copyText("accountRestart.confirm.compactNote")
      : ""), trust: trustWarn });
  if (!(await confirmFn(msg, { danger: true }))) return false;

  // ③ [可选] 在【旧账号】上 compact（换号前，命中旧缓存——§5.1）。失败/超时不阻断（§5.2）。
  if (opts.compactFirst) {
    showActionFailureToast(
      copyText("accountRestart.compact.running"),
      copyText("accountRestart.compact.sent"),
      { level: "info", durationMs: 8000 },
    );
    try {
      await sendKeys(origin, tmuxName, "/compact", sessionId);
      const done = opts.awaitCompact
        ? await opts.awaitCompact()
        : await delay(DEFAULT_COMPACT_WAIT_MS).then(() => false);
      if (!done) {
        showActionFailureToast(
          copyText("accountRestart.compact.timeoutTitle"),
          copyText("accountRestart.compact.timeout"),
          { level: "info", durationMs: 6000 },
        );
      }
    } catch (e) {
      showActionFailureToast(copyText("accountRestart.compact.skippedTitle"), copyText("accountRestart.compact.skipped", { e: saidOfControl(e) }), {
        level: "info",
        durationMs: 6000,
      });
      // 不中止，继续 ④。
    }
  }

  // ④ 结束旧会话（`tmux-control.ts::killSession`，关卡 2 在那台后端）。不再先发 `Escape` ＋ `/exit` 等它自己退：
  //   直接杀。**失败 → 中止不续 ⑤**（避免新旧两进程抢同一会话；§5.2 ④）。
  try {
    await killSession(origin, tmuxName, sessionId);
  } catch (e) {
    const said = copyText("accountRestart.aborted.body", { e: saidOfControl(e) });
    // 关卡 2 拒的 ⇒ 「对齐后重试」：只对这个会话重验 ＋ 重打，再从头走一遍（二次确认照问）。
    if (isIdentityRefusal(e)) {
      offerResyncRetry(origin, sessionId, copyText("accountRestart.aborted.title"), said, async () => {
        await restartWithAccount(opts);
      });
    } else {
      showActionFailureToast(copyText("accountRestart.aborted.title"), said, { level: "error", durationMs: 10000 });
    }
    return false;
  }

  // ⑤ 用新账号 resume（tmux 版，注入其 configDir）。失败走 runRemoteResumeTmux 既有剪贴板回退。
  // F05：顺带把 accountName 传给它——本函数本来就已知这个名字（opts.accountName），线通进
  // LaunchContext 供 CLI 渲染器吐 --account <名>（不改本文件的 accountConfigDir 解析逻辑本身，
  // 见 F05 计划 §2 第2条：account-restart.ts 与 withAccount 是并列路径，不强行合并）。
  // F07：同样补查一次该账号的模型偏好（withAccount 内部也做同一次查询——两条并列路径各自补
  // 一次，同 F05 对 accountName 的处理模式）。
  //
  // **本机那一跳**：编排上面五步两侧逐字共用（发按键 / 结束会话
  // 都按 origin 分流、`<local>` 走得通；账号清单与信任预检也按 origin 分流到本机后端），
  // 只有「resume」这一跳两侧起法不同：
  //
  // | | 远端 | 本机 |
  // |---|---|---|
  // | 起法 | `runRemoteResumeTmux`（渲一条 ssh 命令、开终端，失败回退剪贴板） | `local-resume.ts::resumeLocalSession`（本机后端直接起，**没有剪贴板那条回退**） |
  // | 账号怎么交 | `LaunchModifiers.configDir / accountName` | 载荷上的 `account`（用户点的那个具名号：目录 ＋ 名字，`K-R53`） |
  // | tmux 名 | 复用被 kill 让出来的旧名 | 同左 |
  // | 模型偏好 | 交（`modelOverride`） | **交不了** —— 本机那条的载荷里没有模型这一格（如实登记，不假装），所以这里不去查它 |
  //
  // ⚠ 账号**不走**跟随（`follow`）：换号重启的正题恰恰是换成另一个号，跟随会把用户的选择丢了。
  const isLocal = isLocalOrigin(origin);
  // 「全成」的定义换成**看见会话起来**：执行器等那台报出这条会话才回 `arrived`；
  //   没等到（`missed`，那一句主窗口已经说了）⇒ 不记账、不说「已用新账号重启」；没真发出去（`unsent`）⇒ 照旧说失败。
  const launched = isLocal
    ? await resumeLocalSessionAndWait({
        agent: ACTIVE_AGENT,
        sid: sessionId,
        cwd,
        account: { kind: "explicit", configDir, name: accountName },
        tmuxName,
        launcher,
        failureTitle: copyText("localResume.restart.failed"),
      })
    : await runRemoteResumeTmuxAndWait(origin, ACTIVE_AGENT, sessionId, cwd, launcher, tmuxName, {
        configDir,
        accountName,
        modelOverride: await getModelForAccount(origin, accountName),
      });

  // ⑥ 记 lastAccount（源②）+ 提示。
  // **只有真拉起来了才算成功**（Phase G 审计）：此前无条件记账+报成功,而第⑤步的失败是
  // 确定性的（F34 launcher 含双引号被 launch.rs 拒 / tmux 名不合白名单 / 缺 OpenSSH）——
  // 那种情况下会话已被 kill 却没起来,还被钉上"上次用账号 X 起"、被批量对齐计成成功。
  if (launched === "missed") return false;
  if (launched === "unsent") {
    showActionFailureToast(
      copyText("accountRestart.relaunch.failedTitle"),
      isLocal
        ? // 本机没有剪贴板那条回退 —— 不许照抄远端那句「到远端终端粘贴」。
          copyText("accountRestart.relaunch.failedBody")
        : copyText("accountRestart.relaunch.copied", { name: accountName }),
      { level: "error", durationMs: 12000 },
    );
    return false;
  }
  void recordLastAccount(sessionId, accountName);
  showActionFailureToast(
    copyText("accountRestart.done.title"),
    copyText("accountRestart.done.body", { name: accountName }),
    { level: "info", durationMs: 8000 },
  );
  return true;
}

// ═══════════════════════════════════════════════════════════════════════════
// 从 `accounts.ts` 搬来：换号重启定位不到会话时的那句话（起停域）。
// ═══════════════════════════════════════════════════════════════════════════

/**
 * `K-P5g`：**读回来的身份 token 的第一个生产消费者。**
 * 换号重启定位不到 tmux 时，用它在**两条互斥的成因**里选一条说给用户听。
 *
 * # 它买的是「决定」，不是「显示」
 *
 * `K-P5f` 把 `launchId` 从 `/proc/<pid>/environ` 一路读到了前端类型上，交回时自己逐字写着
 * 「有人读、没人用 —— 没有任何生产代码拿它做决定」。本函数是那最后一跳：
 * 它**不把 token 显示出来**（那是个内部 nonce，给用户看毫无意义），而是拿它**判一件
 * token 里没有的事**——这条会话是从哪条路起来的——再据此改口。
 *
 * # 为什么这件事今天只有 token 答得出
 *
 * `CCM_LAUNCH_ID` 全树**只有一处写**（`src/frontend/shell/src/history.rs` 里那个
 * `LAUNCH_ID_VAR`，`launcher_identity_registry` 那张棘轮表数着它，多一处就红）
 * ⇒ 进程环境里带着它，就说明这条会话是从本工具这条路起来的。
 * 而 `--session-accounts` 出参里其余每一格（`configDir` / `account` / `bare` /
 * `alive` / `cwd` / `pid`）**一格都答不了这个问题**：它们说的是「跑在哪个账号下、
 * 活没活着」，不是「谁把它拉起来的」。⇒ 掐掉这一格，下面那句话就只能回到
 * 「不在 tmux 里**或**不是本工具起的」这种**两条成因并排摆着**的说法。
 *
 * # ⚠ 它答不到的（照抄 `K-P5f` 已登记的那格残留洞，别在这里悄悄拓宽）
 *
 * `launchId` 是**继承型**环境变量：在一条本工具起的会话里手敲 `claude` 起出来的孩子
 * 也带着同一个 token。backend 挡得住「同一个 token 同时落在一条以上活会话上」
 * （那种涉事的全置 `null`），**挡不住父会话已经退出**的那一格。
 * ⇒ 本函数的「是」精确读作「**这个进程的环境里带着本工具铸的身份标记**」，
 * 不读作「一定是本工具直接拉起的」。文案也按这个强度写，不许写强。
 */
export function restartLocateFailureMessage(
  row: SessionAccount | undefined,
  opts: { local?: boolean } = {},
): {
  title: string;
  body: string;
} {
  // ⚠ `undefined`（老后端不出这个键）与 `null`（backend 说「不作数」）在这里是同一件事。
  const carriesOurLaunchMark = Boolean(row && row.alive && row.launchId);
  // 最后那句补救**只对远端成立**：本机归档 tab 的 Resume 不带账号选择
  // （走 `localLaunchAccountSync`，沿用这条会话上次的号），「把此会话切到账号 X」在本机不存在。
  // 对本机说那句话，是在指一条走不通的路。
  const tail = opts.local
    ? copyText("accounts.restartLocate.tailLocal")
    : copyText("accounts.restartLocate.tailRemote");
  if (carriesOurLaunchMark) {
    return {
      title: copyText("accounts.restartLocate.markLostTitle"),
      body:
        copyText("accounts.restartLocate.markLost", { tail }),
    };
  }
  return {
    title: copyText("accounts.restartLocate.title"),
    body:
      copyText("accounts.restartLocate.body", { tail }),
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// `K-P5h`：**拿身份 token 反查出新会话的 sid**（`KP5HD2` / `KP5HD3`）
// ═══════════════════════════════════════════════════════════════════════════
