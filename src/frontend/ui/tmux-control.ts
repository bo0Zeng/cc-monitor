/**
 * 界面直接说的 tmux 控制帧命令：结束会话（`kill`）。抓一屏走终端管理那一条 `terminal-preview`（`terminal-reads.ts`）。
 * 1. 不判目标名：会话名原样交后端，空目标由后端入口拒（`bad_args`），这里带上后端原话说出来；身份门 · 窗口门只在后端 `control/gate.rs`。
 * 2. typed 命令（入参 · 回包从 Rust 类型生成，`generated/TypedCommands.ts`）：形状由握手指纹管。结束是破坏性的，还要成品明说做成了（`killed` 为真），
 *    否则当「不知道做了没有」——不当成功、也不换条路重做。线上形状由金样 `tests/__fixtures__/tmux-control.golden.json` 钉着。
 * 3. 失败怎么说：拒绝码逐码一句（认不出的原样带出去）；通道三层错误各一句（本机与远端的下一步不同）。句子住文案表 `tmuxControl.*`。
 * 期限：结束 10 秒。
 */
import { copyText } from "./copy-table";
import { ControlError, machineName, settleTyped } from "./control-said";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { Killed } from "./generated/Killed";

/** 结束成了之后顺手注销 cc-bus 那几行（后端写好的句子 ＋ 复制详情；没什么可说 ⇒ `said` 是 `null`）。 */
export interface KillNote {
  said: string | null;
  detail: string;
}

/** 结束的期限（见头注）。 */
const CONTROL_BUDGET_MS = 10_000;

// ─── 结束会话 ───

/** 结束会话的拒绝码 ⇒ 一句话。身份门 / 窗口门两档说清拦下的原因（它们的下一步与「会话不在」完全不同）。 */
/**
 * `kill` 的回包 ⇒ 做成了没有 ＋顺手从 cc-bus 名册注销的结局那一句（没有要说的 ⇒ `null`）。
 * `killed` 不为真 ⇒ 「后端没确认结束」（**不当成功**：破坏性动作在未知状态上不许往下走）。
 */
export function killedOf(origin: Origin, target: string, v: Killed): KillNote {
  if ((v.killed as boolean) !== true) {
    throw new ControlError(copyText("tmuxControl.kill.notConfirmed", { machine: machineName(origin), target }), "");
  }
  return { said: v.bus.said, detail: v.bus.detail };
}

/**
 * 结束 `origin` 上的 tmux 会话 `target`（**破坏性**：调用方先二次确认）。身份门 · 窗口门在后端先过，
 * 后端对**句柄**下手（不是名字）。给了 `sid` ⇒ 结束挂着它的那个窗格（同会话里还有别的 claude 时不关整个会话）。
 * 失败 ⇒ 抛 [`ControlError`]；**没有第二条路可回落**。
 */
export async function killSession(origin: Origin, target: string, sid?: string): Promise<KillNote> {
  const req = sid === undefined ? { name: target } : { name: target, sid };
  const budget = budgetWithin(CONTROL_BUDGET_MS);
  return killedOf(origin, target, await settleTyped(origin, "kill", callTyped(origin, "kill", req, budget)));
}
