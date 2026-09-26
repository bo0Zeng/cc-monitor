/**
 * 〔FE1 · 第四波 4D〕**起会话时那个 tmux 会话名从哪来 —— 本机与远端同一个家。**
 *
 * 守的要求：`设计/01 §5` D1「**一个判定只有一个家**」（「同一条规则有两份实现，它们就会漂；而漂开的后果是静默的错，不是报错」）
 * ＋ D4「**一条都不许静默忽略**」。
 *
 * # 为什么收成一个
 *
 * 「列出那台机器现有的 tmux 名 → 从 cwd 派生基名 → 撞名避让」此前抄了五份：本机两份（`ipc/local-tmux-name.ts`
 * ＋ `tab-session-actions.ts` 里一份内联），远端三份（`remote-launch-run.ts::runNewSessionRemote` ·
 * `settings/machine-card.ts` 开新 Claude · `tab-session-actions.ts::resumeTabTmuxInner` 全新那一支）。
 * **降级口径两种**：本机「列不出 ⇒ 不铸」，远端「列不出 ⇒ 空集铸名、不避让」。后者正是 issue #76 的形状 ——
 * 同一个 cwd 派生出同一个名字，撞上远端 `create-or-attach` 的幂等闸，**静默接进第一个会话，而用户以为开了新的**。
 * 注释里记着四次「这里修了、那里漏了」。
 *
 * # 规则只有一条：列不出 ⇒ 不铸名
 *
 * 「列不出」与「零会话」是两件事，不许压成一个空集：
 *
 * | 读口 | 回什么 | 读作 |
 * |---|---|---|
 * | 本机 `list_local_tmux` | 列表 | 知道 |
 * | 本机 `list_local_tmux` | `null` / 抛 | **不知道**（本机后端还没报过这份快照） |
 * | 远端 `list_remote_tmux` | 列表（含空表） | 知道 |
 * | 远端 `list_remote_tmux` | `null` | 知道：那台**没装 tmux**（`NO_TMUX`）⇒ 一个名字都没占 |
 * | 远端 `list_remote_tmux` | 抛 | **不知道**（没连上 / 回话脏，`tmux.rs::list_remote_tmux` 头注的三档） |
 *
 * 「不铸名」之后怎么办归调用方：本机把 `null` 交给后端（它有一条如实的「不进容器」旧路）；
 * 远端**没有**不进 tmux 的起法 ⇒ [`refuseUnmintable`]：不起，说清是哪台、为什么。
 *
 * ⚠ 算法本身（基名 `<项目名>-cc` ＋ `-2` / `-3` 避让）仍住 `remote-launch.ts`（`deriveTmuxName` ·
 * `mintTmuxName` · `mintSessionTmuxName`，纯函数、`node` 单测）；本文件只管「拿什么集合去避让」这一格。
 * 分叉那条（`fork-launch.ts::forkTmuxName`，基名 `-fork-cc`）用的是同一份 [`readTmuxListing`]。
 */
import { commands } from "./ipc/commands";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { mintSessionTmuxName } from "./remote-launch";
import type { TmuxSession } from "./tmux-sessions";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";

/** 那台机器此刻有哪些 tmux 会话 —— 知道，或者说得出为什么不知道。 */
export type TmuxListing =
  | { kind: "known"; sessions: readonly TmuxSession[] }
  | { kind: "unknown"; why: string };

/** 问一次那台机器的 tmux 名单。**不抛**：问不到就是 `unknown`，带着原因。 */
export async function readTmuxListing(origin: Origin): Promise<TmuxListing> {
  try {
    const sessions = isLocalOrigin(origin)
      ? await commands.list_local_tmux()
      : await commands.list_remote_tmux({ origin });
    return listingFromFetch(origin, sessions);
  } catch (e) {
    return { kind: "unknown", why: String(e) };
  }
}

/**
 * 把一个**已经取回来**的答案读成 [`TmuxListing`]（给自己已经问过一次的调用方：
 * `tab-session-actions.ts::fetchTmuxFresh` 是 TabManager 里唯一的取数点，别为铸名再问一次）。
 *
 * `undefined` = 调用方那一问抛了（没问到）。`null` 的意思**按机器分**（见头注表）：本机 = 不知道，远端 = 没装 tmux。
 */
export function listingFromFetch(
  origin: Origin,
  sessions: readonly TmuxSession[] | null | undefined,
): TmuxListing {
  if (sessions === undefined) return { kind: "unknown", why: copyText("tmuxMint.unknown.notAsked") };
  if (sessions === null) {
    return isLocalOrigin(origin)
      ? { kind: "unknown", why: copyText("tmuxMint.unknown.localNotYet") }
      : { kind: "known", sessions: [] };
  }
  return { kind: "known", sessions };
}

/** 名单里被占了的那些名字；`unknown` ⇒ `null`（**不是**空集）。 */
export function takenNamesOf(listing: TmuxListing): string[] | null {
  return listing.kind === "known" ? listing.sessions.map((s) => s.name) : null;
}

/** 给一个 cwd 铸一个不撞现有名字的会话名；名单不知道 ⇒ `null`（绝不退化成空集）。 */
export function mintFromListing(cwd: string, listing: TmuxListing): string | null {
  const taken = takenNamesOf(listing);
  return taken === null ? null : mintSessionTmuxName(cwd, new Set(taken));
}

export type MintOutcome = { ok: true; name: string } | { ok: false; why: string };

/** 问名单 ＋ 铸名，一步。 */
export async function mintFreshTmuxName(origin: Origin, cwd: string): Promise<MintOutcome> {
  const listing = await readTmuxListing(origin);
  const name = mintFromListing(cwd, listing);
  return name !== null
    ? { ok: true, name }
    : { ok: false, why: listing.kind === "unknown" ? listing.why : "" };
}

/** 远端铸不出名字 ⇒ 不起，说清是哪台、为什么（D4）。 */
export function refuseUnmintable(origin: Origin, why: string): void {
  showActionFailureToast(
    copyText("tmuxMint.refused.title"),
    copyText("tmuxMint.refused.body", { machine: origin, reason: why }),
    { level: "error", durationMs: 10000 },
  );
}
