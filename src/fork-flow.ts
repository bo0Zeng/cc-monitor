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

import { isLocalOrigin, isRemoteOrigin, type Origin } from "./ipc/origin";
import { showActionFailureToast } from "./error-toast";
import { getBehavior } from "./behavior";
import { resolveResumeCommand } from "./remote-config";
import { isSelectable, type Account, type SessionAccount } from "./accounts";
import { fetchAccounts, fetchLocalAccounts, fetchSessionAccounts } from "./account-reads";
import { findClaudeTmuxMatches, type TmuxSession } from "./tmux-sessions";
import { resumeLocalSession } from "./local-resume";
import { readTmuxListing } from "./tmux-name-mint";
import { askForkLaunch, type ForkAccountOption } from "./fork-ask";
import { startForkedSession, type ForkStartDeps, type ForkStartOutcome } from "./fork-start";
import type { ForkLaunchInput } from "./fork-launch";
import { runRemoteResume, runRemoteResumeTmux } from "./remote-launch-run";
import { copyText } from "./copy-table";

export interface ForkFlowInput {
  /** 哪台机器（本机 = `LOCAL_ORIGIN`）。 */
  origin: Origin;
  /** 刚分叉出来的**新**会话 sid。 */
  newSessionId: string;
  /**
   * **源**会话 sid。查事实查的是它 —— 新会话此刻还没起，查它必定「查不到」，
   * 那样每次分叉都会白弹一次窗。两个调用点各有各的拿法（实时 tab 手里就有 `tab.sessionId`；
   * 历史查看器只有路径，而历史会话的文件名就是 sid），**那不是重复，是两种不同的输入**。
   */
  sourceSessionId: string;
  /** 源会话的工作目录（新会话的起始目录）。 */
  cwd: string | null;
}

/** 一次分叉要用到的「源会话事实」，喂给 `runForkFlow`。 */
export interface ForkSourceFacts {
  source: ForkLaunchInput;
  /** 源会话所在 tmux 名（新名要避开它）。 */
  sourceTmuxName: string | null;
  /**
   * 远端已占用的全部 tmux 名（新名要避开它们）。
   * 〔FE1〕**`null` = 名单没问到**（不是「一个都没占」）⇒ 选了 tmux 就不起、说清（`fork-start.ts`）。
   * 先前这里 `?? []`：远端不可达时拿空集铸 `-fork-cc`，与 #76 同形。
   */
  takenTmuxNames: string[] | null;
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
    // 〔FE1〕`null` / `undefined` = 名单没取到 ⇒ `null`，不压成空集（见字段头注）。
    takenTmuxNames: sessions ? sessions.map((s) => s.name) : null,
  };
}

/**
 * 取源会话事实。取数失败一律降级成「不知道」（⇒ 弹窗问一次），**绝不**降级成一个具体值。
 *
 * ~~**本机没有对侧探针**~~（〔E79〕之后有了；〔C4a〕与远端同一条路）：backend 的 `--session-accounts` 是远端专属，本机侧至今没有
 * 「某 sid 现在跑在哪个账号下」的查询（`local_accounts.rs` 只枚举账号，不认会话）。
 * 所以本机一律按「查不出来」处理 —— 问一次，而不是拿当前账号顶替。
 */
export async function collectForkSource(
  origin: Origin,
  sid: string,
  cwd: string | null,
): Promise<ForkSourceFacts> {
  if (isLocalOrigin(origin)) {
    // E79：本机侧**现在有对侧探针了**（`--session-accounts`，Linux 才有 ——
    // 要读 `/proc/<pid>/environ`）。此前这里硬编码「查不出来」，于是分叉一个**正跑着的**
    // 本机会话也要白弹一次追问小窗，而那个 pidfile 就在本机、monitor 明明够得着。
    //
    // 平台答不出时（Windows）后端会明说答不出 ⇒ 这里照旧落「不知道」，走追问那条路。
    // **「查不出来」与「查了但没有」在这里是同一个结论，但理由不同**（〔C4a〕两种都回空行集）。
    // 〔C4a〕经通道问本机后端（与远端同一个 `fetchSessionAccounts`；`force`：分叉要此刻的读数）。
    //   查不到 ⇒ 空行集 ⇒ 「不知道」，不猜。
    const rows: SessionAccount[] = await fetchSessionAccounts(origin, true);
    // 本机这条路不进 tmux（`fork-start.ts` 已把 tmux 那一格摘掉），所以只用账号那一半。
    const facts = deriveForkSource(rows, null, sid, cwd);
    return {
      source: { ...facts.source, liveTmuxName: null },
      sourceTmuxName: null,
      takenTmuxNames: [],
    };
  }
  // 〔FE1〕tmux 名单只经 `tmux-name-mint.ts::readTmuxListing` 取（本机远端同一个家）：
  //   没问到 ⇒ `unknown` ⇒ 这里交 `null`；远端没装 tmux ⇒ 一张确定的空表。
  const [rows, listing] = await Promise.all([
    fetchSessionAccounts(origin).catch(() => [] as SessionAccount[]),
    readTmuxListing(origin),
  ]);
  return deriveForkSource(
    rows,
    listing.kind === "known" ? [...listing.sessions] : null,
    sid,
    cwd,
  );
}

/**
 * 列可选账号喂给追问小窗。查不到（账号功能没启用 / 远端不可达）→ **空清单**，
 * 小窗仍然弹、仍然能选「账号 0」—— 账号列不出来不该把整条分叉路堵死。
 */
async function listForkAccounts(origin: Origin): Promise<ForkAccountOption[]> {
  try {
    const state = isLocalOrigin(origin) ? await fetchLocalAccounts() : await fetchAccounts(origin);
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
        defaultUseTmux: isRemoteOrigin(input.origin),
      }),

    // 〔FE1〕本机那一跳走 resume 编排的唯一一份（`local-resume.ts`）：sid 校验先于任何 IPC（F06）、
    //   名字现铸（`K-R46`：后端故意不铸，不传 ⇒ 不进容器；这条路上尤其贵 —— 分叉是全仓唯一说得出
    //   「账号 0」的生产路，而 POSIX 后端只有那一态渲染得出容器）、账号是**用户在小窗里显式选的**：
    //   账号 0 ⇒ 显式 `base`（不是省略：省略 = 没表态 = 被 shell rc 里的默认号顶掉），具名 ⇒ 名字说得出才带（`K-R53`）。
    //   ⚠ 这里铸的是**新会话自己**的 `<项目名>-cc`（避让本机现有名字），与远端那条「避开源会话的名字」
    //     （`fork-start.ts::forkTmuxName`）不是一回事。失败它自己出声，这里只回布尔（与 `startRemote` 同形）。
    startLocal: (a) =>
      resumeLocalSession({
        sid: a.sessionId,
        cwd: a.cwd,
        account: { kind: "explicit", configDir: a.configDir, name: a.accountName },
        failureTitle: copyText("localResume.fork.failed"),
      }),

    startRemote: async (a) => {
      const behavior = await getBehavior();
      const launcher = await resolveResumeCommand(a.origin, behavior.resumeCommandRemote);
      // `configDir: null` = 账号 0 = 什么都不注入；`mods.configDir` 收 `string | undefined`，
      // 所以 null 要落成 undefined，**不能落成空串**（空值 ≠ 未设，见 accounts.ts Z01）。
      const mods = { configDir: a.configDir ?? undefined };
      // ★ 返回值必须往上传：那两条路失败时**不抛**，只弹自己的 toast 并回 false。
      return a.tmuxName
        ? runRemoteResumeTmux(a.origin, a.sessionId, a.cwd, launcher, a.tmuxName, mods)
        : runRemoteResume(a.origin, a.sessionId, a.cwd, launcher, mods);
    },
  };
}

/**
 * **分叉之后的全部事情**：查源会话事实 → 编排（该问就问一次）→ 起 → 反馈。
 *
 * # E78：为什么连成功 toast 都收进来
 *
 * 此前 `collectForkSource` → `runForkFlow` → 成功 toast 这三步由**两个调用点各写一遍**
 * （约 15 行，连文案都是逐字重复的双写点、无守卫）。Phase G 审计点名：本模块自称
 * 「唯一生产接线」而真正共享的只有中段 —— **名不副实的抽象比没有抽象更坏**，
 * 因为它让人以为改一处就够了。
 *
 * ⇒ 现在调用点只剩「我是谁 + 分叉结果」，`⑂` 在两处的行为**在结构上**不可能分裂。
 *
 * 失败**必须可见**：编排里任何一步抛出来都变成 toast，绝不静默
 * （`runRemoteResume*` 自己那两条失败路径已经各带 toast + 剪贴板回退，所以那边只回 false）。
 */
export async function runForkFlow(input: ForkFlowInput): Promise<ForkStartOutcome> {
  try {
    const facts = await collectForkSource(input.origin, input.sourceSessionId, input.cwd);
    const outcome = await startForkedSession(
      {
        newSessionId: input.newSessionId,
        origin: input.origin,
        source: facts.source,
        sourceTmuxName: facts.sourceTmuxName,
        takenTmuxNames: facts.takenTmuxNames,
      },
      productionDeps(input),
    );
    if (outcome === "started") {
      showActionFailureToast(
        copyText("forkFlow.done.title"),
        copyText("forkFlow.done.body", { id: input.newSessionId.slice(0, 8) }),
        { level: "info", durationMs: 8000 },
      );
    }
    // `cancelled`（用户自己收手）与 `failed`（已经弹过失败 toast）都不再叠一个反馈。
    return outcome;
  } catch (err) {
    // 抛出来的（本机 sid 校验失败、IPC reject…）在这里变成 toast；返回 `failed` 而不是
    // `cancelled` —— 调用方据此区分「出错了」与「用户自己收手」。
    showActionFailureToast(copyText("forkFlow.runForkFlow.failed"), String(err));
    return "failed";
  }
}
