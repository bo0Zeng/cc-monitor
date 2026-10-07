/**
 * **界面直接说的 tmux 控制类帧命令** —— 结束会话（`kill`）。（抓一屏走终端管理那一条 `terminal-preview`，在 `terminal-reads.ts`。）
 *
 * # 本文件做的只有三件（都是调用方那一侧的事）
 *
 * 1. **不判目标名**（`§34` Gate 1 在后端 `control/gate_rules.rs` 的 tmux 名那一族，TS 零）：会话名原样交给后端；
 *    空目标由后端入口拒（`bad_args`，`=:` 会被 tmux 读成「当前会话」那一格），本文件照那句「后端不接受这个会话名」
 *    带上后端原话说出来。**Gate 2 / 3（身份门 · 窗口门）只在后端 `control/gate.rs`**，本文件不写第二份。
 * 2. **按形状收**：成品恰好是那几格、类型对 ⇒ 收；多一格 / 缺一格 / 类型不对 ⇒ 当成「两边版本对不上」抛，不猜。
 *    结束是破坏性的，还要成品**明说做成了**（`killed` 为真）—— 形状不对或没说做成 ⇒ 当成「不知道做了没有」，**不当成功、也不换条路重做**。
 *    线上形状由跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 钉着（后端产出 == 金样 · 本文件读同一份）。
 * 3. **失败怎么说**（`§3.3.2`「说法归调用方」）：拒绝码 → 一句话（逐码分开，认不出的码原样带出去，不猜）；
 *    通道的三层错误 → 一句话（本机与远端的下一步不同，话就不一样）。句子住文案表 `tmuxControl.*`。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 结束 10 秒。
 */
import { copyText } from "./copy-table";
import { ControlError, machineName, refusalsByTable, saidOfControl, settle, unreadable, type Refusals } from "./control-said";
import { exactKeys, isObj } from "./ipc/decode";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

// 这一层与 `src/frontend/ui/cc-bus-control.ts` 说的是同一件事的那几样（`ControlError` · 通道三层的说法 · 成品形状核验 ·
//   `settle`）搬进了 `src/frontend/ui/control-said.ts`；本文件的调用方照旧从这里取那两样。
export { ControlError, saidOfControl };

/** 结束的期限（见头注）。 */
const CONTROL_BUDGET_MS = 10_000;

// ─── 结束会话 ───

/** 结束会话的拒绝码 ⇒ 一句话。身份门 / 窗口门两档说清拦下的原因（它们的下一步与「会话不在」完全不同）。 */
export function killRefusals(target: string): Refusals {
  return refusalsByTable(
    {
      bad_args: (detail) => copyText("tmuxControl.kill.badName", { target, detail }),
      no_tmux: (detail) => copyText("tmuxControl.kill.noTmux", { target, detail }),
      no_such_session: (detail) => copyText("tmuxControl.kill.noSuchSession", { target, detail }),
      wrong_owner: (detail) => copyText("tmuxControl.kill.wrongOwner", { target, detail }),
      too_many_windows: (detail) => copyText("tmuxControl.kill.tooManyWindows", { target, detail }),
      kill_failed: (detail) => copyText("tmuxControl.kill.failed", { target, detail }),
      child_timed_out: (detail) => copyText("tmuxControl.kill.childTimedOut", { target, detail }),
    },
    {
      other: (detail) => copyText("tmuxControl.kill.otherCode", { target, detail }),
      none: () => copyText("tmuxControl.kill.noReason", { target }),
    },
  );
}

/**
 * `kill` 的成品 ⇒ 做成了没有 ＋顺手从 cc-bus 名册注销的结局那一句（没有要说的 ⇒ `null`）。
 * 恰好 `{session, killed, bus}` 且 `killed === true` 才算结束了；形状对但 `killed` 不为真 ⇒ 「后端没确认结束」
 * （**不当成功**：破坏性动作在未知状态上不许往下走）。`bus` 那一格形状不对 ⇒ 整份读不懂。
 */
export function decodeKilled(origin: Origin, target: string, v: unknown): string | null {
  if (!isObj(v) || !exactKeys(v, ["session", "killed", "bus"]) || typeof v.session !== "string" || typeof v.killed !== "boolean") {
    throw unreadable(origin, "kill", "is not exactly {session, killed, bus}");
  }
  const bus = v.bus;
  if (
    !isObj(bus) ||
    !exactKeys(bus, ["removed", "failed", "unread"]) ||
    !Array.isArray(bus.removed) ||
    !bus.removed.every((x) => typeof x === "string") ||
    !Array.isArray(bus.failed) ||
    !bus.failed.every((f) => isObj(f) && exactKeys(f, ["id", "why"]) && typeof f.id === "string" && typeof f.why === "string") ||
    !(bus.unread === null || typeof bus.unread === "string")
  ) {
    throw unreadable(origin, "kill", "bus is not exactly {removed: string[], failed: {id, why}[], unread: string|null}");
  }
  if (!v.killed) {
    throw new ControlError(
      copyText("tmuxControl.kill.notConfirmed", { machine: machineName(origin), target }),
      "kill reply has killed=false but no refusal code",
    );
  }
  const said: string[] = [];
  if (bus.unread !== null) said.push(copyText("tmuxControl.kill.busUnread", { why: bus.unread }));
  if (bus.removed.length > 0) said.push(copyText("tmuxControl.kill.busRemoved", { ids: (bus.removed as string[]).join(copyText("tmuxControl.kill.listSep")) }));
  for (const f of bus.failed as { id: string; why: string }[]) said.push(copyText("tmuxControl.kill.busFailed", { id: f.id, why: f.why }));
  return said.length === 0 ? null : said.join("\n");
}

/**
 * 结束 `origin` 上的 tmux 会话 `target`（**破坏性**：调用方先二次确认）。身份门 · 窗口门在后端先过，
 * 后端对**句柄**下手（不是名字）。给了 `sid` ⇒ 结束挂着它的那个窗格（同会话里还有别的 claude 时不关整个会话）。
 * 失败 ⇒ 抛 [`ControlError`]；**没有第二条路可回落**。
 */
export async function killSession(origin: Origin, target: string, sid?: string): Promise<string | null> {
  const payload = jsonBody(sid === undefined ? { name: target } : { name: target, sid });
  const budget = budgetWithin(CONTROL_BUDGET_MS);
  const v = await settle(origin, "kill", chan.call(origin, "kill", payload, budget), killRefusals(target));
  return decodeKilled(origin, target, v);
}
