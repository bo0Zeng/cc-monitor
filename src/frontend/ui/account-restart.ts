// 换号重启：确认之后交会话所在那台一条 `session-restart` 做完（查号 → 先压缩并等摘要 → 停旧 → 同一终端名用新号起 → 等报出），
// 界面只拼确认框、按回复说一句、起成了开一个终端窗口接上。那台做到一半界面关了 / 刷新了，那台照样做完。
//
// 用的号是用户在菜单里点名的那个（不跟随：换号重启的正题就是换成另一个号）；那台判它选不选得了。
// 换号重启的是标签页里的会话 ⇒ 流跟的那一家。
import { chan } from "../../comms/inward/chan";
import { ACTIVE_AGENT } from "./agent-profile";
import { askConfirm, type ConfirmFn } from "./ask-dialog";
import { ControlError, machineName, settle, type Refusals } from "./control-said";
import { budgetWithin, jsonBody, refusalOf } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { offerResyncRetry } from "./resync";
import { runRemoteAttach } from "./remote-launch-run";
import { type SessionAccount } from "./accounts";
import { fetchAccounts, checkTrust } from "./account-reads";
import { accountUnavailableOf } from "./launch-account";
import { ARRIVAL_BUDGET_MS } from "./launch-arrival";
import { startSettings } from "./tab-batch-run";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";

export interface RestartWithAccountOpts {
  origin: Origin;
  sessionId: string;
  cwd: string;
  /** 这个会话此刻所在的终端名（确认框里说清是哪个；那台照 sid 自己再找一次）。 */
  tmuxName: string;
  accountName: string;
  /** 是否先在旧号上请求压缩、等压缩完再停（默认 false）。 */
  compactFirst: boolean;
  /** 二次确认（默认应用内对话框；测试注入）。 */
  confirm?: ConfirmFn;
  /** 起失败（旧的已停）时「再起一次」：用点名的号在 tmux 里起这一个并接上。 */
  startAgain?: () => Promise<void>;
  /** 那台说「不在终端里」时选哪一句话用的会话账号行（`restartLocateFailureMessage`）。 */
  sessionAccount?: SessionAccount;
}

/** 等压缩摘要的期限（交那台）：压缩一段长会话要几分钟。 */
export const COMPACT_WITHIN_MS = 300_000;
/** 界面等这条命令的底数（两个等待之外）：罩住那台的定位 · 送压缩那一句 · 停旧 ＋ 起新三步。 */
const RESTART_BASE_MS = 60_000;

/** 那台答的那一形。 */
interface RestartReply {
  compact: "done" | "timed_out" | "skipped" | "unsupported" | "failed";
  started: "arrived" | "missed";
  terminal: string;
  account: unknown;
}

function decode(v: unknown): RestartReply {
  const r = v as Partial<RestartReply> | null;
  if (
    !r ||
    typeof r.terminal !== "string" ||
    !["done", "timed_out", "skipped", "unsupported", "failed"].includes(r.compact as string) ||
    !["arrived", "missed"].includes(r.started as string)
  ) {
    throw new ControlError(copyText("accountRestart.failed.unreadable"), "session-restart: unreadable reply");
  }
  return r as RestartReply;
}

/** 失败那一格（码 ＋ 按码定形的 `data`）。 */
function refusal(e: unknown): { code: string; data: Record<string, unknown> } | null {
  if (!(e instanceof ControlError) || e.error?.layer !== "peer" || e.error.why !== "refused") return null;
  const r = refusalOf(e.error.body);
  if (!r) return null;
  const data = r.data !== null && typeof r.data === "object" ? (r.data as Record<string, unknown>) : {};
  return { code: r.code, data };
}

/** 压缩那一步的结局 ⇒ 那一句里的一小段（没要求 ⇒ 空）。 */
function compactTag(c: RestartReply["compact"]): string {
  switch (c) {
    case "skipped":
      return "";
    case "done":
      return copyText("accountRestart.compactTag.done");
    case "timed_out":
      return copyText("accountRestart.compactTag.timedOut");
    case "unsupported":
      return copyText("accountRestart.compactTag.unsupported");
    case "failed":
      return copyText("accountRestart.compactTag.failed");
  }
}

/**
 * @returns 那台用新号起成了（等没等到它报出都算）才 `true`；没开动 / 停了没起 ⇒ `false`。
 */
export async function restartWithAccount(opts: RestartWithAccountOpts): Promise<boolean> {
  const { origin, sessionId, cwd, tmuxName, accountName } = opts;

  // 信任预检：只警告不阻断（后端判）。要那个号的目录 —— 从账号清单那一行原样取，选不选得了由那台判。
  let trustWarn = "";
  try {
    const row = (await fetchAccounts(origin)).accounts.find((a) => a.name === accountName);
    if (row?.configDir) {
      const t = await checkTrust(origin, row.configDir, cwd);
      if (t.available && t.known && !t.trusted) trustWarn = copyText("accountRestart.confirm.trustWarn");
    }
  } catch {
    /* 读不到不影响主流程 */
  }

  // 破坏性二次确认（真 app 里 `window.confirm` 是插件注入的 async 替身，恒真值 ⇒ 走应用内对话框）。
  const confirmFn: ConfirmFn = opts.confirm ?? askConfirm;
  const msg = copyText("accountRestart.confirm.body", {
    name: accountName,
    tmuxName,
    compact: opts.compactFirst ? copyText("accountRestart.confirm.compactNote") : "",
    trust: trustWarn,
  });
  if (!(await confirmFn(msg, { danger: true }))) return false;

  // 中途只说这一句；结局在最后说一次。
  showActionFailureToast(
    copyText("accountRestart.running.title"),
    copyText("accountRestart.running.body", {
      name: accountName,
      tmuxName,
      compact: opts.compactFirst ? copyText("accountRestart.running.compactTag") : "",
    }),
    { level: "info", durationMs: 8000 },
  );

  const body = jsonBody({
    sid: sessionId,
    cwd,
    account: accountName,
    compact_first: opts.compactFirst,
    compact_within_ms: COMPACT_WITHIN_MS,
    arrive_within_ms: ARRIVAL_BUDGET_MS,
    local: isLocalOrigin(origin),
    ...(await startSettings(origin)),
  });
  const budget = budgetWithin(COMPACT_WITHIN_MS + ARRIVAL_BUDGET_MS + RESTART_BASE_MS);
  const refusals: Refusals = {
    byCode: (_code, detail) => detail,
    noReason: () => copyText("accountRestart.failed.unreadable"),
  };
  let reply: RestartReply;
  try {
    reply = decode(await settle(origin, "session-restart", chan.call(origin, "session-restart", body, budget), refusals));
  } catch (e) {
    sayFailure(opts, e);
    return false;
  }

  // 起成了 ⇒ 开一个终端接上（本机 / 远端同「接回」那一条），结局在这里说一次。
  await runRemoteAttach(origin, ACTIVE_AGENT, reply.terminal, { quiet: true });
  const compact = compactTag(reply.compact);
  if (reply.started === "arrived") {
    showActionFailureToast(
      copyText("accountRestart.done.title"),
      copyText("accountRestart.done.body", { name: accountName, terminal: reply.terminal, compact }),
      { level: "info", durationMs: 8000 },
    );
  } else {
    showActionFailureToast(
      copyText("accountRestart.missed.title"),
      copyText("accountRestart.missed.body", {
        name: accountName,
        terminal: reply.terminal,
        secs: String(Math.round(ARRIVAL_BUDGET_MS / 1000)),
        compact,
      }),
      { level: "error", durationMs: 15000 },
    );
  }
  return true;
}

/** 失败的那一句（按码）。 */
function sayFailure(opts: RestartWithAccountOpts, e: unknown): void {
  const { origin, sessionId, accountName } = opts;
  const said = e instanceof Error ? e.message : String(e);
  const r = refusal(e);
  if (accountUnavailableOf(e)) {
    showActionFailureToast(
      copyText("accountRestart.unselectable.title"),
      copyText("accountRestart.unselectable.body", { name: accountName }),
      { level: "info", durationMs: 6000 },
    );
    return;
  }
  switch (r?.code) {
    case "ambiguous": {
      const names = Array.isArray(r.data.names) ? r.data.names : [];
      showActionFailureToast(
        copyText("tabSessionActions.restart.refusedTitle"),
        copyText("tabSessionActions.restart.dupes", { n: names.length }),
        { level: "info", durationMs: 8000 },
      );
      return;
    }
    case "not_in_terminal": {
      const m = restartLocateFailureMessage(opts.sessionAccount, { local: isLocalOrigin(origin) });
      showActionFailureToast(m.title, m.body, { level: "info", durationMs: 8000 });
      return;
    }
    case "stop_failed": {
      const body = copyText("accountRestart.aborted.body", { e: said });
      // 身份门拒的 ⇒ 「对齐后重试」：只对这个会话重验 ＋ 重打，再从头走一遍（二次确认照问）。
      if (r.data.why === "wrong_owner") {
        offerResyncRetry(origin, sessionId, copyText("accountRestart.aborted.title"), body, async () => {
          await restartWithAccount(opts);
        });
      } else {
        showActionFailureToast(copyText("accountRestart.aborted.title"), body, { level: "error", durationMs: 10000 });
      }
      return;
    }
    case "start_failed": {
      const terminal = typeof r.data.terminal === "string" ? r.data.terminal : "";
      showActionFailureToast(
        copyText("accountRestart.startFailed.title"),
        copyText("accountRestart.startFailed.body", { machine: machineName(origin), terminal, detail: said, name: accountName }),
        { level: "error", durationMs: 20000, onClick: opts.startAgain ? () => void opts.startAgain?.() : undefined },
      );
      return;
    }
    default:
      showActionFailureToast(copyText("accountRestart.failed.title"), said, { level: "error", durationMs: 10000 });
  }
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
  // （跟随这条会话上次的号），「把此会话切到账号 X」在本机不存在。
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
