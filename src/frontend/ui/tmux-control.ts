/**
 * 界面直接说的 tmux 控制帧命令：结束会话（`kill`）。抓一屏走终端管理那一条 `terminal-preview`（`terminal-reads.ts`）。
 * 1. 不判目标名：会话名原样交后端，空目标由后端入口拒（`bad_args`），这里带上后端原话说出来；身份门 · 窗口门只在后端 `control/gate.rs`。
 * 2. 按形状收：多一格 / 缺一格 / 类型不对 ⇒ 当「两边版本对不上」抛，不猜。结束是破坏性的，还要成品明说做成了（`killed` 为真），
 *    否则当「不知道做了没有」——不当成功、也不换条路重做。线上形状由金样 `tests/__fixtures__/tmux-control.golden.json` 钉着。
 * 3. 失败怎么说：拒绝码逐码一句（认不出的原样带出去）；通道三层错误各一句（本机与远端的下一步不同）。句子住文案表 `tmuxControl.*`。
 * 期限：结束 10 秒。
 */
import { copyText } from "./copy-table";
import { asSaid, ControlError, machineName, saidOfControl, settle, unreadable, type Refusals } from "./control-said";
import { exactKeys, isObj } from "./ipc/decode";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

// 与 `cc-bus-control.ts` 共用的那几样（`ControlError` · 通道三层的说法 · 成品形状核验 · `settle`）住 `control-said.ts`。
export { ControlError, saidOfControl };

/** 结束成了之后顺手注销 cc-bus 那几行（后端写好的句子 ＋ 复制详情；没什么可说 ⇒ `said` 是 `null`）。 */
export interface KillNote {
  said: string | null;
  detail: string;
}

/** 结束的期限（见头注）。 */
const CONTROL_BUDGET_MS = 10_000;

// ─── 结束会话 ───

/** 结束会话的拒绝码 ⇒ 一句话。身份门 / 窗口门两档说清拦下的原因（它们的下一步与「会话不在」完全不同）。 */
export function killRefusals(target: string): Refusals {
  return asSaid(() => copyText("tmuxControl.kill.noReason", { target }));
}

/**
 * `kill` 的成品 ⇒ 做成了没有 ＋顺手从 cc-bus 名册注销的结局那一句（没有要说的 ⇒ `null`）。
 * 恰好 `{session, killed, bus}` 且 `killed === true` 才算结束了；形状对但 `killed` 不为真 ⇒ 「后端没确认结束」
 * （**不当成功**：破坏性动作在未知状态上不许往下走）。`bus` 那一格形状不对 ⇒ 整份读不懂。
 */
export function decodeKilled(origin: Origin, target: string, v: unknown): KillNote {
  if (!isObj(v) || !exactKeys(v, ["session", "killed", "bus"]) || typeof v.session !== "string" || typeof v.killed !== "boolean") {
    throw unreadable(origin, "kill", "is not exactly {session, killed, bus}");
  }
  const bus = v.bus;
  if (
    !isObj(bus) ||
    !exactKeys(bus, ["removed", "failed", "unread", "said", "detail"]) ||
    !(bus.said === null || typeof bus.said === "string") ||
    typeof bus.detail !== "string"
  ) {
    throw unreadable(origin, "kill", "bus is not exactly {removed, failed, unread, said: string|null, detail: string}");
  }
  if (!v.killed) {
    throw new ControlError(copyText("tmuxControl.kill.notConfirmed", { machine: machineName(origin), target }), "");
  }
  return { said: bus.said as string | null, detail: bus.detail as string };
}

/**
 * 结束 `origin` 上的 tmux 会话 `target`（**破坏性**：调用方先二次确认）。身份门 · 窗口门在后端先过，
 * 后端对**句柄**下手（不是名字）。给了 `sid` ⇒ 结束挂着它的那个窗格（同会话里还有别的 claude 时不关整个会话）。
 * 失败 ⇒ 抛 [`ControlError`]；**没有第二条路可回落**。
 */
export async function killSession(origin: Origin, target: string, sid?: string): Promise<KillNote> {
  const payload = jsonBody(sid === undefined ? { name: target } : { name: target, sid });
  const budget = budgetWithin(CONTROL_BUDGET_MS);
  const v = await settle(origin, "kill", chan.call(origin, "kill", payload, budget), killRefusals(target));
  return decodeKilled(origin, target, v);
}
