/**
 * G6（branch-anywhere）：分叉之后**真的把会话起起来** —— 把 G3b-2 的编排器接到真依赖上。
 *
 * `fork-start.ts` 是纯编排（三条判断，依赖全注入、可直测）；本模块是它的**唯一生产接线**：
 * 把 `ask` / `startLocal` / `startRemote` 换成真弹窗、真 IPC、真 ssh。
 *
 * # 为什么这层单独存在，而不是直接写进两个调用点
 *
 * 调用点有两个（历史查看器 `views/session-viewer.ts` 与实时 tab `tabs.ts`），
 * 「分叉完怎么起」在两边**必须一模一样** —— 否则同一个 `⑂` 在两个地方行为不同，
 * 正是账本第 3 行要治的那种分裂。所以接线只有这一份。
 *
 * # 为什么本模块不 import `tabs.ts`
 *
 * `tabs.ts` 挂 `⑂`（→ `branch-button.ts`）→ 分叉成功 → 调本模块。本模块再回头 import
 * `tabs.ts` 就成环了。而「源会话在哪个 tmux 里」的判据（`findClaudeTmuxMatches`）
 * 原本正住在 `tabs.ts` 里 —— 所以 G6 把那一族判据搬进了叶子模块 `tmux-sessions.ts`，
 * 两边都从那里取。`tabs.ts` 原样 re-export，既有 import 面零改动。
 */

import { commands } from "./ipc/commands";
import { showActionFailureToast } from "./error-toast";
import { getBehavior } from "./behavior";
import { resolveResumeCommand } from "./remote-config";
import { validateLocalLaunch } from "./launch-requests";
import {
  fetchAccounts,
  fetchLocalAccounts,
  fetchSessionAccounts,
  isSelectable,
  type Account,
  type SessionAccount,
} from "./accounts";
import { findClaudeTmuxMatches, type TmuxSession } from "./tmux-sessions";
import { askForkLaunch, type ForkAccountOption } from "./fork-ask";
import { startForkedSession, type ForkStartDeps, type ForkStartOutcome } from "./fork-start";
import type { ForkLaunchInput } from "./fork-launch";
import { runRemoteResume, runRemoteResumeTmux } from "./remote-launch-run";

export interface ForkFlowInput {
  /** 远端 origin；`null` = 本机。 */
  origin: string | null;
  /** 刚分叉出来的新会话 sid。 */
  newSessionId: string;
  /** 源会话的事实（由调用方查好，见模块头注「为什么不 import tabs.ts」）。 */
  source: ForkLaunchInput;
  sourceTmuxName?: string | null;
  takenTmuxNames?: readonly string[];
}

/** 一次分叉要用到的「源会话事实」，喂给 `runForkFlow`。 */
export interface ForkSourceFacts {
  source: ForkLaunchInput;
  /** 源会话所在 tmux 名（新名要避开它）。 */
  sourceTmuxName: string | null;
  /** 远端已占用的全部 tmux 名（新名要避开它们）。 */
  takenTmuxNames: string[];
}

/**
 * 从两份**已经取回来的**远端快照推源会话事实。纯函数，故可直测。
 *
 * ★ 两个信号各答各的，**不许互相顶替**：
 * - **活没活着 / 在哪个 tmux 里** → tmux 清单（`@ccm_sid` 精确匹配，INVARIANTS §30）
 * - **属于哪个账号** → pidfile（`--session-accounts`）。tmux 清单里**没有**账号信息。
 *
 * 所以「tmux 里找到了、但账号查不到」是一个**真实且常见**的状态（账号功能没启用 /
 * cc-acct-iso 没部署）。此时必须落成 `liveConfigDir: undefined`（= 活着但不知道账号），
 * **不是** `null`（= 确认账号 0）—— 后者会让分叉静默起在账号 0 上。
 */
export function deriveForkSource(
  rows: readonly SessionAccount[] | null | undefined,
  sessions: TmuxSession[] | null | undefined,
  sid: string,
  cwd: string | null,
): ForkSourceFacts {
  const row = rows?.find((r) => r.sessionId === sid && r.alive);
  const matches = findClaudeTmuxMatches(sessions, sid);
  const tmuxName = matches[0]?.name ?? null;
  const live = Boolean(row) || matches.length > 0;
  return {
    source: {
      sourceIsLive: live,
      sourceCwd: cwd,
      // `row` 缺席时**故意留 undefined**（见函数头注），不要写成 `?? null`。
      liveConfigDir: row ? row.configDir : undefined,
      liveTmuxName: tmuxName,
    },
    sourceTmuxName: tmuxName,
    takenTmuxNames: sessions?.map((s) => s.name) ?? [],
  };
}

/**
 * 取源会话事实。取数失败一律降级成「不知道」（⇒ 弹窗问一次），**绝不**降级成一个具体值。
 *
 * **本机没有对侧探针**：daemon 的 `--session-accounts` 是远端专属，本机侧至今没有
 * 「某 sid 现在跑在哪个账号下」的查询（`local_accounts.rs` 只枚举账号，不认会话）。
 * 所以本机一律按「查不出来」处理 —— 问一次，而不是拿当前账号顶替。
 */
export async function collectForkSource(
  origin: string | null,
  sid: string,
  cwd: string | null,
): Promise<ForkSourceFacts> {
  if (origin === null) {
    return {
      source: { sourceIsLive: false, sourceCwd: cwd },
      sourceTmuxName: null,
      takenTmuxNames: [],
    };
  }
  const [rows, sessions] = await Promise.all([
    fetchSessionAccounts(origin).catch(() => [] as SessionAccount[]),
    commands.list_remote_tmux({ origin }).catch(() => null),
  ]);
  return deriveForkSource(rows, sessions, sid, cwd);
}

/**
 * 列可选账号喂给追问小窗。查不到（账号功能没启用 / 远端不可达）→ **空清单**，
 * 小窗仍然弹、仍然能选「账号 0」—— 账号列不出来不该把整条分叉路堵死。
 */
async function listForkAccounts(origin: string | null): Promise<ForkAccountOption[]> {
  try {
    const state = origin === null ? await fetchLocalAccounts() : await fetchAccounts(origin);
    return state.accounts
      .filter((a: Account) => isSelectable(a) && a.configDir !== null)
      .map((a: Account) => ({ name: a.name, configDir: a.configDir }));
  } catch {
    return [];
  }
}

/** 生产依赖。抽出来是为了让 `runForkFlow` 只剩「组装 + 转交」一句话。 */
function productionDeps(input: ForkFlowInput): ForkStartDeps {
  return {
    ask: async (facts, slots) =>
      askForkLaunch({
        facts,
        slots,
        accounts: await listForkAccounts(input.origin),
        // 远端会话惯例住在 tmux 里（断线能 attach 回来）；本机那条路根本不问 tmux
        // （`fork-start.ts` 已把这一格摘掉），所以这里给 false 也走不到。
        defaultUseTmux: input.origin !== null,
      }),

    startLocal: async (a) => {
      // F06 纪律（从被顶掉的 `session-viewer.resumeBranch` 原样搬来）：**sid 校验先于任何
      // IPC 往返**。抛出去由 `runForkFlow` 的 catch 变成 toast，绝不拿一个残缺 sid 去拉终端。
      validateLocalLaunch({ kind: "resume", sid: a.sessionId }, a.cwd);
      const behavior = await getBehavior();
      await commands.resume_history_session({
        sessionId: a.sessionId,
        cwd: a.cwd,
        launcher: behavior.resumeCommandLocal || null,
        configDir: a.configDir,
      });
    },

    startRemote: async (a) => {
      const behavior = await getBehavior();
      const launcher = await resolveResumeCommand(a.origin, behavior.resumeCommandRemote);
      // `configDir: null` = 账号 0 = 什么都不注入；`mods.configDir` 收 `string | undefined`，
      // 所以 null 要落成 undefined，**不能落成空串**（空值 ≠ 未设，见 accounts.ts Z01）。
      const mods = { configDir: a.configDir ?? undefined };
      if (a.tmuxName) {
        await runRemoteResumeTmux(a.origin, a.sessionId, a.cwd, launcher, a.tmuxName, mods);
      } else {
        await runRemoteResume(a.origin, a.sessionId, a.cwd, launcher, mods);
      }
    },
  };
}

/**
 * 起那条刚分叉出来的会话。失败**必须可见** —— 编排器里任何一步抛出来都变成 toast，
 * 绝不静默（`runRemoteResume*` 自己那两条失败路径已经各带 toast + 剪贴板回退）。
 */
export async function runForkFlow(input: ForkFlowInput): Promise<ForkStartOutcome> {
  try {
    return await startForkedSession(
      {
        newSessionId: input.newSessionId,
        origin: input.origin,
        source: input.source,
        sourceTmuxName: input.sourceTmuxName,
        takenTmuxNames: input.takenTmuxNames,
      },
      productionDeps(input),
    );
  } catch (err) {
    showActionFailureToast("起分叉会话失败", String(err));
    return "cancelled";
  }
}
