// A5：换号破坏性重启会话的编排（DESIGN §5）。活跃远端会话 → 杀旧进程 + 用新账号 resume 同一 sid。
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
import { invoke } from "@tauri-apps/api/core";
import { runRemoteResumeTmux } from "./remote-launch-run";
import { fetchAccounts, accountConfigDir, recordLastAccount, checkTrust } from "./accounts";
import { showActionFailureToast } from "./error-toast";

export interface RestartWithAccountOpts {
  origin: string;
  sessionId: string;
  cwd: string;
  /** 本工具的 `cc-<sid8>` 会话名（send-keys / kill 目标；后端白名单只认 cc-*）。 */
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
 * 用户取消 / kill 失败）。account-ux U6 的批量对齐据此汇总真实成败——**不改变任何既有语义**，
 * 老调用点（右键菜单两条）忽略返回值即可。
 */
export async function restartWithAccount(opts: RestartWithAccountOpts): Promise<boolean> {
  const { origin, sessionId, cwd, tmuxName, accountName, launcher } = opts;

  // ① 预检：解析 configDir（顺带校验可选）。不可选 → 明确提示、不动手（§5.2 ①）。
  const state = await fetchAccounts(origin);
  const configDir = accountConfigDir(state, accountName);
  if (!configDir) {
    showActionFailureToast(
      "账号不可用",
      `账号「${accountName}」当前不可选（未登录 / 非隔离 / 目录缺失），无法用它重启。`,
      { level: "info", durationMs: 6000 },
    );
    return false;
  }
  // trust 只警告不阻断（§5 ①）。
  let trustWarn = "";
  try {
    const t = await checkTrust(origin, configDir, cwd);
    if (t.available && t.known && !t.trusted) {
      trustWarn = "\n注意：该账号尚未信任此目录，CC 可能在弹出的终端里询问是否信任。";
    }
  } catch {
    /* trust 查询失败不影响主流程 */
  }

  // ② 破坏性二次确认。
  const confirmFn = opts.confirm ?? ((m: string) => window.confirm(m));
  const msg =
    `用账号「${accountName}」重启此会话？\n\n` +
    `会中断当前回合：先请求会话优雅退出（最多等 ~10s），再结束旧进程（tmux 会话 ${tmuxName}），` +
    `然后用新账号 resume 同一会话。` +
    (opts.compactFirst
      ? "\n将先在【旧账号】上 /compact（命中旧缓存更省），可能耗时数分钟。"
      : "") +
    trustWarn;
  if (!confirmFn(msg)) return false;

  // ③ [可选] 在【旧账号】上 compact（换号前，命中旧缓存——§5.1）。失败/超时不阻断（§5.2）。
  if (opts.compactFirst) {
    showActionFailureToast(
      "正在压缩上下文…",
      "已在旧账号上发送 /compact（命中旧缓存更省），完成后换号重启。",
      { level: "info", durationMs: 8000 },
    );
    try {
      await invoke("tmux_send_keys", { origin, target: tmuxName, keys: "/compact" });
      const done = opts.awaitCompact
        ? await opts.awaitCompact()
        : await delay(DEFAULT_COMPACT_WAIT_MS).then(() => false);
      if (!done) {
        showActionFailureToast(
          "压缩可能未完成",
          "等待超时——仍继续换号重启（compact 是优化非必需）。",
          { level: "info", durationMs: 6000 },
        );
      }
    } catch (e) {
      showActionFailureToast("压缩未执行", `${String(e)}——跳过，继续换号重启。`, {
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
    await invoke("tmux_send_keys", { origin, target: tmuxName, keys: "Escape", enter: false });
    await delay(EXIT_INTERRUPT_GAP_MS);
    await invoke("tmux_send_keys", { origin, target: tmuxName, keys: "/exit", enter: true });
    const exited = opts.awaitExit
      ? await opts.awaitExit()
      : await delay(DEFAULT_EXIT_WAIT_MS).then(() => false);
    if (!exited) {
      showActionFailureToast(
        "优雅退出超时",
        "等待会话自行退出超时——改为强制结束（当前回合已中断）。",
        { level: "info", durationMs: 6000 },
      );
    }
  } catch (e) {
    // send-keys 发不出去（会话已没了 / tmux 异常等）——不中止，交给 ④c kill 收场。
    showActionFailureToast("优雅退出未完成", `${String(e)}——改为强制结束旧会话。`, {
      level: "info",
      durationMs: 5000,
    });
  }
  try {
    await invoke("kill_remote_tmux", { origin, target: tmuxName });
  } catch (e) {
    showActionFailureToast(
      "重启已中止",
      `结束旧会话失败：${String(e)}。未继续 resume（避免新旧两个进程抢同一会话）。`,
      { level: "error", durationMs: 10000 },
    );
    return false;
  }

  // ⑤ 用新账号 resume（tmux 版，注入其 configDir）。失败走 runRemoteResumeTmux 既有剪贴板回退。
  const launched = await runRemoteResumeTmux(origin, sessionId, cwd, launcher, tmuxName, configDir);

  // ⑥ 记 lastAccount（源②）+ 提示。
  // **只有真拉起来了才算成功**（Phase G 审计）：此前无条件记账+报成功,而第⑤步的失败是
  // 确定性的（F34 launcher 含双引号被 launch.rs 拒 / tmux 名不合白名单 / 缺 OpenSSH）——
  // 那种情况下会话已被 kill 却没起来,还被钉上"上次用账号 X 起"、被批量对齐计成成功。
  if (!launched) {
    showActionFailureToast(
      "旧会话已结束，但新会话未能自动拉起",
      `已用「${accountName}」的命令回退到剪贴板——请到远端终端粘贴执行，会话内容不会丢（jsonl 续写）。` +
        `未记账本次账号归属。`,
      { level: "error", durationMs: 12000 },
    );
    return false;
  }
  void recordLastAccount(sessionId, accountName);
  showActionFailureToast(
    "已用新账号重启",
    `已用「${accountName}」重启此会话；若 CC 询问是否信任该目录，请在弹出的终端里确认。`,
    { level: "info", durationMs: 8000 },
  );
  return true;
}
