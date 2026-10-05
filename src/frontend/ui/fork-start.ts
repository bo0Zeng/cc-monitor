/**
 * 分叉出新会话文件之后 —— **把它起起来**：那台推出的三格（`session-fork` 回复的 `launch`）里不知道的那几格问一次 → 调 `sessions-start`。
 *
 * 推断住那台后端（`control/fork_launch.rs`）；「新会话一定铸新终端名、不复用父会话的」也在那台（那一项带 `fresh_terminal` ＋ 源会话 `fork_of`，
 * 新名从源会话此刻的终端名铸）。
 * 这里只剩两条：**不知道就问、且只问一次**（知道的一格都不问）；**不知道又没答的绝不替用户填**（号落账号 0：不注入任何身份）。
 * 本模块只起新的，对原会话一个字都不碰。
 *
 * `ask` / `start` 注入进来（弹窗与起会话都是副作用），判断可以直接测。
 */

import { isLocalOrigin, type Origin } from "./ipc/origin";
import { chosenAccount } from "./launch-account";
import type { ForkLaunch } from "./session-writes";
import type { StartItem } from "./tab-batch-run";

/** 要问的一格（按呈现顺序）。 */
export type ForkAskSlot = "account" | "terminal" | "cwd";

/** 用户在追问小窗里给的答案。只覆盖不知道的那几格。 */
export interface ForkChoices {
  /** 号名；`null` = 账号 0（不设配置目录）。 */
  account?: string | null;
  /** 起在 tmux 里还是直连。 */
  useTmux?: boolean;
  cwd?: string;
}

/** 起会话交给那台的那一项（`sessions-start` 的 `items[0]`，带 `fresh_terminal` 与源会话 `fork_of`）与那一形。 */
export interface ForkStart {
  mode: "tmux" | "window";
  item: StartItem & { fresh_terminal: true; fork_of: string };
}

export interface ForkStartDeps {
  /** 弹一次追问小窗。返回 `null` = 用户取消（**什么都不起**）。 */
  ask: (launch: ForkLaunch, slots: ForkAskSlot[]) => Promise<ForkChoices | null>;
  /** 交那台起；回「真起来了吗」（失败那一路自己出声、回 `false`）。 */
  start: (s: ForkStart) => Promise<boolean>;
}

export interface ForkStartInput {
  /** 刚分叉出来的新会话 sid。 */
  newSessionId: string;
  /** 源会话 sid（那台从它此刻的终端名铸新名）。 */
  sourceSessionId: string;
  /** 哪台机器（本机 = `LOCAL_ORIGIN`）。 */
  origin: Origin;
  /** 那台推出的三格。 */
  launch: ForkLaunch;
}

/** `failed` 与 `cancelled` 必须分开：前者要报错，后者是用户自己收手、不该再弹任何东西。 */
export type ForkStartOutcome = "started" | "cancelled" | "failed";

/** 还要问的几格。本机那条路一律开终端（POSIX 上那台铸名建进 tmux、Windows 直路），终端那一格不问。 */
export function slotsToAsk(launch: ForkLaunch, origin: Origin): ForkAskSlot[] {
  const order: ForkAskSlot[] = ["account", "terminal", "cwd"];
  return order.filter((k) => launch[k].kind === "unknown" && !(k === "terminal" && isLocalOrigin(origin)));
}

/** 起那条分叉出来的会话：只在真有不知道的格时问一次。 */
export async function startForkedSession(input: ForkStartInput, deps: ForkStartDeps): Promise<ForkStartOutcome> {
  const { launch, origin } = input;
  const slots = slotsToAsk(launch, origin);
  let choices: ForkChoices = {};
  if (slots.length > 0) {
    const answered = await deps.ask(launch, slots);
    if (answered === null) return "cancelled";
    choices = answered;
  }
  // 每一格：知道就用知道的，不知道就用用户答的；号没答 ⇒ 账号 0（不拿任何号顶替）。
  const cwd = launch.cwd.kind === "known" ? launch.cwd.value : (choices.cwd ?? "");
  const account = chosenAccount(launch.account.kind === "known" ? launch.account.value : (choices.account ?? null));
  const inTerminal =
    launch.terminal.kind === "known" ? launch.terminal.value.host !== "none" : (choices.useTmux ?? false);
  const mode = !isLocalOrigin(origin) && inTerminal ? "tmux" : "window";
  const started = await deps.start({
    mode,
    item: { sid: input.newSessionId, cwd, account, fresh_terminal: true, fork_of: input.sourceSessionId },
  });
  return started ? "started" : "failed";
}
