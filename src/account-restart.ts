// A5：换号破坏性重启会话的编排（DESIGN §5）。活跃会话 → 杀旧进程 + 用新账号 resume 同一 sid。
// 〔`A3` 第二波〕本机会话也走这一条（`origin = <local>`），只在第⑤步换成本机那一跳。
// **破坏性**（中断当前回合）。失败语义严格照 §5.2：compact 失败/超时**不阻断**；kill 失败**必须中止**、
// 绝不续 resume（否则新旧两个进程抢同一会话）。在 A4 的账号解析/记账之上插入 [compact]→kill→resume。
// 依赖经 import（vitest 可 vi.mock）；confirm / awaitCompact 两个交互点可注入，便于纯逻辑单测。
//
// **为何另起、不复用 A4 的 `withAccount`**（D 架构审计裁定，防下轮重新纠结）：两者语义天然不兼容——
//   ① 不可选账号时：withAccount **降级默认起**；restart **中止**（破坏性重启绝不能退化用默认号）。
//   ② 记 lastAccount 条件：withAccount run 后**无条件**记；restart **仅 kill+resume 全成后**才记
//      （kill 失败提前 return、绝不记，见 §5.2 + vitest ④）。硬合需给 withAccount 加 abort-vs-degrade /
//      条件记账 / run 前置 compact&kill 钩子三个开关，复杂度净增、收益为负。二者已共用 accounts.ts
//      **同一批原语**（fetchAccounts / accountConfigDir / recordLastAccount），无逻辑漂移。故维持分离。
import { commands } from "./ipc/commands";
import { runRemoteResumeTmux } from "./remote-launch-run";
import { fetchAccounts, accountConfigDir, recordLastAccount, checkTrust, getModelForAccount } from "./accounts";
import { showActionFailureToast } from "./error-toast";
// 〔`A3` 第二波〕本机那一侧：`origin` 是 backend 的 `<local>`（〔C4b〕账号面那个第二种写法已退役，只剩这一个）。
import { LOCAL_ORIGIN } from "./backend-policy";
import { runLocalRestartResume } from "./account-restart-local";
import { copyText } from "./copy-table";

export interface RestartWithAccountOpts {
  origin: string;
  sessionId: string;
  cwd: string;
  /** 本工具的会话名（send-keys / kill 目标；后端 `tmux_send_keys` 与
   *  `kill_remote_tmux` 白名单都只认 `*-cc`——audit-fixes F02 后 kill 也对称加了守卫）。
   *
   *  ⚠ 〔`K-R96` 09-12 订正〕这两句**原本都写反了**：写的是「`cc-<sid8>` 会话名」＋
   *  「白名单都只认 `cc-*`」。真实形状是 **`<X>-cc` 后缀**（S4b-3b，用户 2026-07-31 把
   *  `cc-` 前缀反转成 `-cc` 后缀），白名单认的是 `*-cc`（老的 `cc-*` 前缀仍兼容认，
   *  但那是**向后兼容**，不是今天产的形状 —— 判定住 `gate-core::is_ccm_tmux_name`）。
   *  ⚠ 而 `<X>` 今天也不是 `<sid8>` 了：`K-R96` 之后是 **`<项目名>`**
   *  （用户 `R55`：「要是可读的名字 / 不要id」）。sid 骑在 `@ccm_sid` 上，不在名字里。 */
  tmuxName: string;
  accountName: string;
  /** F34 远端 resume 命令（空 → 后端默认）。 */
  launcher: string;
  /** ③ 是否先在【旧账号】上 /compact（默认 false，用户拍板）。 */
  compactFirst: boolean;
  // —— 可注入点（默认走真实实现；测试注入 mock）——
  confirm?: (message: string) => boolean;
  /** 等 compact 完成：resolve(true)=检测到完成 / resolve(false)=超时放弃。省略 → 有界延时兜底。 */
  awaitCompact?: () => Promise<boolean>;
  /**
   * A5+ 优雅退出：等旧 CC 真正退出。resolve(true)=检测到已退出（前台不再是 claude）/
   * resolve(false)=超时放弃。省略 → 有界延时兜底（等满 `DEFAULT_EXIT_WAIT_MS`）。
   */
  awaitExit?: () => Promise<boolean>;
}

/** compact 兜底等待上限（无注入检测器时）。超时按 §5.2 不阻断、继续重启。 */
export const DEFAULT_COMPACT_WAIT_MS = 90_000;

/** A5+ 优雅退出等待上限（DESIGN §5 ④「等 M 秒，默认 10s」）。超时 → 降级 kill。 */
export const DEFAULT_EXIT_WAIT_MS = 10_000;

/** Esc 打断当前回合后、键入 `/exit` 前的间隔（让打断先生效）。 */
const EXIT_INTERRUPT_GAP_MS = 300;

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
  const confirmFn = opts.confirm ?? ((m: string) => window.confirm(m));
  const msg =
    copyText("accountRestart.confirm.body", { name: accountName, tmuxName, compact: (opts.compactFirst
      ? copyText("accountRestart.confirm.compactNote")
      : ""), trust: trustWarn });
  if (!confirmFn(msg)) return false;

  // ③ [可选] 在【旧账号】上 compact（换号前，命中旧缓存——§5.1）。失败/超时不阻断（§5.2）。
  if (opts.compactFirst) {
    showActionFailureToast(
      copyText("accountRestart.compact.running"),
      copyText("accountRestart.compact.sent"),
      { level: "info", durationMs: 8000 },
    );
    try {
      await commands.tmux_send_keys({ origin, target: tmuxName, keys: "/compact" });
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
      showActionFailureToast(copyText("accountRestart.compact.skippedTitle"), copyText("accountRestart.compact.skipped", { e: String(e) }), {
        level: "info",
        durationMs: 6000,
      });
      // 不中止，继续 ④。
    }
  }

  // ④ 结束旧进程（DESIGN §5 ④ / §5.2 ④）。分优雅退出 + 兜底 kill 两段：
  //   ④a 优雅退出（best-effort，让 CC flush jsonl / 释放锁再走）：先 Esc 打断当前回合（**不带尾
  //      Enter**——否则可能误提交输入框里的队列文本），短暂间隔后键入 `/exit`（文档化的干净退出）。
  //      send-keys 发不出去**不中止**——落到 ④c 的 kill 兜底。
  //   ④b 有界等 CC 真的退出（awaitExit：轮询该 tmux 前台是否不再是 claude）；超时 → §5.2 ④ 降级 kill。
  //   ④c kill_remote_tmux：**清场**（会话跑的是交互 shell，CC 退出后 shell 仍占着会话名，会让 ⑤ 的
  //      `new-session -d ... 2>/dev/null && send-keys` 短路成只 attach 到没有 claude 的旧 shell）+ 优雅
  //      退出超时时的**兜底 SIGKILL**。**失败 → 中止不续 ⑤**（避免新旧两进程抢同一会话；§5.2 ④ 语义不变）。
  try {
    // ⚠ **F12 修一条 F04c 引入的回归**：`Escape` 走后端的新 mode `send-keys-raw`，
    // 旧版本后端不认它 ⇒ `invalid_args` ⇒ 按 `backend_route` 的规则判 `Refused`（**不回落**）
    // ⇒ 这里 throw。而它与下面的 `/exit` 原本共用一个 `try` ⇒ **`/exit` 整段被跳过**，
    // 直接落到 ④c 强杀 —— CC 没机会 flush jsonl / 释放锁。
    // 那与 ④a 逐字写着的「send-keys 发不出去**不中止**」直接矛盾：**代码与它自己声明的意图不符**
    // （Phase G 的 `/full-audit` 逮到的）。⇒ `Escape` 是 best-effort，它失败**不许**影响 `/exit`。
    try {
      await commands.tmux_send_keys({ origin, target: tmuxName, keys: "Escape", enter: false });
    } catch (e) {
      console.debug(`[F12] Escape（打断当前回合）发不出去，继续走 /exit：${String(e)}`);
    }
    await delay(EXIT_INTERRUPT_GAP_MS);
    await commands.tmux_send_keys({ origin, target: tmuxName, keys: "/exit", enter: true });
    const exited = opts.awaitExit
      ? await opts.awaitExit()
      : await delay(DEFAULT_EXIT_WAIT_MS).then(() => false);
    if (!exited) {
      showActionFailureToast(
        copyText("accountRestart.exit.timeoutTitle"),
        copyText("accountRestart.exit.timeout"),
        { level: "info", durationMs: 6000 },
      );
    }
  } catch (e) {
    // send-keys 发不出去（会话已没了 / tmux 异常等）——不中止，交给 ④c kill 收场。
    showActionFailureToast(copyText("accountRestart.exit.failedTitle"), copyText("accountRestart.exit.failed", { e: String(e) }), {
      level: "info",
      durationMs: 5000,
    });
  }
  try {
    await commands.kill_remote_tmux({ origin, target: tmuxName });
  } catch (e) {
    showActionFailureToast(
      copyText("accountRestart.aborted.title"),
      copyText("accountRestart.aborted.body", { e: String(e) }),
      { level: "error", durationMs: 10000 },
    );
    return false;
  }

  // ⑤ 用新账号 resume（tmux 版，注入其 configDir）。失败走 runRemoteResumeTmux 既有剪贴板回退。
  // F05：顺带把 accountName 传给它——本函数本来就已知这个名字（opts.accountName），线通进
  // LaunchContext 供 CLI 渲染器吐 --account <名>（不改本文件的 accountConfigDir 解析逻辑本身，
  // 见 F05 计划 §2 第2条：account-restart.ts 与 withAccount 是并列路径，不强行合并）。
  // F07：同样补查一次该账号的模型偏好（withAccount 内部也做同一次查询——两条并列路径各自补
  // 一次，同 F05 对 accountName 的处理模式）。
  //
  // 〔`A3` 第二波〕**本机那一跳**：编排上面五步两侧逐字共用（`tmux_send_keys` / `kill_remote_tmux`
  // 都按 origin 分流、`<local>` 走得通；账号清单与信任预检也按 origin 分流到本机后端），
  // 只有「resume」这一跳两侧起法不同 —— 见 `account-restart-local.ts` 头注那张表。
  // ⚠ 本机那一跳**交不了模型偏好**（本机载荷里没有那一格），所以这里不去查它。
  const isLocal = origin === LOCAL_ORIGIN;
  const launched = isLocal
    ? await runLocalRestartResume({ sessionId, cwd, launcher, tmuxName, configDir, accountName })
    : await runRemoteResumeTmux(origin, sessionId, cwd, launcher, tmuxName, {
        configDir,
        accountName,
        modelOverride: await getModelForAccount(accountName),
      });

  // ⑥ 记 lastAccount（源②）+ 提示。
  // **只有真拉起来了才算成功**（Phase G 审计）：此前无条件记账+报成功,而第⑤步的失败是
  // 确定性的（F34 launcher 含双引号被 launch.rs 拒 / tmux 名不合白名单 / 缺 OpenSSH）——
  // 那种情况下会话已被 kill 却没起来,还被钉上"上次用账号 X 起"、被批量对齐计成成功。
  if (!launched) {
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
