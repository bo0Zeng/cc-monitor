/**
 * **这个会话此刻在那台哪个 tmux 会话里** —— 问那台后端（`sessions-where`），界面不判。
 *
 * 判定只在后端一处（带着它的 `@ccm_sid`、前台是不是 agent；没打上标记的不按目录猜）：单个菜单亮哪几项、
 * 批量停 / 起、换号重启找旧会话都读这一份答案（分叉找源会话那一问在那台后端做）。本文件只做调用方那一侧：发 · 收 · 失败说成一句。
 * 界面不拿名单自己筛、不按目录猜。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { machineName, settleTyped } from "./control-said";
import type { StandingWord } from "./generated/StandingWord";
import type { Whereabouts } from "./generated/Whereabouts";
import type { Origin } from "./ipc/origin";
import type { Run } from "./generated/Run";
import { copyText } from "./copy-table";

/**
 * 一个会话的样子：`running` 恰好一个在跑（`names[0]`）· `ambiguous` 在跑的不止一个（按名单顺序）·
 * `idle` 没有在跑的、有带着它的空 tmux（`names[0]`）· `none` 没有哪个会话带着它 · `no_tmux` 那台没装 tmux。
 * `terminals` 与 `names` 同序同数：各自的终端句柄（预览按它指，不按 tmux 名）。`run` ＝ 这台恢复默认放在哪（恢复菜单的勾画在它上面）。
 */
export interface Standing {
  kind: StandingWord;
  names: string[];
  terminals: string[];
  run: Run;
}

/** 期限：那台问一次 tmux 名单就答。 */
const STANDING_BUDGET_MS = 10_000;

/** 整批被那台拒了：那一句前面带上是哪台（批量的提示里机器只有界面知道）。停 · 起 · 问样子共用。 */
export function batchRefused(origin: Origin): (said: string) => string {
  return (said) => copyText("tabBatch.why.batchRefused", { machine: machineName(origin), said });
}

/** 应答 ⇒ 逐个样子（类型从 Rust 生成，形状由握手指纹核过）。 */
export function standingsFrom(reply: Whereabouts): Standing[] {
  return reply.results.map((r) => ({ kind: r.standing, names: r.names, terminals: r.terminals.map((t) => t.terminal), run: reply.run }));
}

/** 那台上这几个会话各自的样子（与 `sids` 同序）。问不到 ⇒ 抛（那一句已经说好）。 */
export async function standingsOf(origin: Origin, sids: readonly string[]): Promise<Standing[]> {
  const budget = budgetWithin(STANDING_BUDGET_MS);
  const v = await settleTyped(origin, "sessions-where", callTyped(origin, "sessions-where", { sids: [...sids] }, budget), batchRefused(origin));
  return standingsFrom(v);
}

/** 一个会话的样子；问不到 ⇒ `undefined`（不知道，不是「不在」）。 */
export async function standingOf(origin: Origin, sid: string): Promise<Standing | undefined> {
  try {
    return (await standingsOf(origin, [sid]))[0];
  } catch {
    return undefined;
  }
}
