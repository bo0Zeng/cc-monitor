/**
 * **界面直接说的 cc-bus 那几条帧命令** —— 名单 · 查某个 agent 在不在线（`bus-list`）· 发消息（`bus-send`）· 读收件箱（`bus-inbox`）。
 *
 * 都由界面经 `chan.call` 直接问那台机器的后端，本机与远端同一条路（本机由 `<local>` 那条长连接答）。
 * 起会话走「开会话」（`session-open`），「谁起的谁」读血缘 —— 总线这里不起、不收会话。
 *
 * # 本文件做的只有三件（都是调用方那一侧的事）
 *
 * 1. **先核入参**：正文非空。过不了就一个字节都不发。
 *    agent id 的形状不在这里判：规则只有一份（`shell_quote_core::bus_id_ok`），后端交给 `cc-send` / `cc-log` 之前先判、
 *    判不过回 `bad_id`（`INVARIANTS §47`），这里把那个码说成人话。
 * 2. **按类型问**：入参 · 回包都是后端类型生成的（`generated/`），回包形状由握手时的形状指纹管（对不上就不发、说「后端版本不同」），
 *    这里不逐格判。
 * 3. **说人话**：发消息的几态逐态一句（在线 / 不在线 / 名字没登记过 / 问不到）；拒绝码逐码一句（认不出的原样带出去）；
 *    通道三层走 `src/frontend/ui/control-said.ts` 那一份。句子住文案表 `ccBus.*`。
 *
 * # 🔴 查在线：「问不到」**不是**「不在线」
 *
 * [`agentOnline`] 只回 `true` / `false` 两个确定的答案；问不到（没通道 · 后端太旧 · 那一趟失败 · 它不在名单里 ·
 * `live` 是 `null`）一律**抛**。调用方把抛出来的那一句渲染成「查不到」，这条路上没有地方能造出一盏灭灯。
 *
 * # 期限（调用点显式给）
 *
 * 查在线 15 秒 · 发消息 · 读名单 · 读收件箱 30 秒。
 */
import { copyText } from "./copy-table";
import { ControlError, machineName, settleTyped } from "./control-said";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { BusSent } from "./generated/BusSent";
import type { BusList as BusListWire } from "./generated/BusList";
import type { BusInbox as BusInboxWire } from "./generated/BusInbox";

/** cc-monitor 自己在总线上的身份 —— 发消息时用它，收信人才不会看到 `unknown`（回复仍无归宿）。 */
export const MONITOR_BUS_ID = "cc-monitor";

const ONLINE_BUDGET_MS = 15_000;
const WRITE_BUDGET_MS = 30_000;
const READ_BUDGET_MS = 30_000;

// ─── 查在线（`bus-list`）───

/**
 * 问 `origin` 上的 agent `id` 此刻在不在线（名单里那份**核过身份**的三态）。
 * 只回确定的答案；问不到 ⇒ 抛 [`ControlError`]（「问不到」不是「不在线」，见头注）。
 * 这是刻意的一次往返：只在用户点某一行的「检查」时问，不默认全量查、不轮询。
 */
export async function agentOnline(origin: Origin, id: string): Promise<boolean> {
  const { agents } = await readRoster(origin, ONLINE_BUDGET_MS);
  const hit = agents.find((a) => a.id === id);
  if (hit === undefined || hit.live === null) {
    throw new ControlError(copyText("ccBus.online.unknown", { id, machine: machineName(origin) }), "not in the roster, or live is null");
  }
  return hit.live;
}

// ─── 发消息（`bus-send`）───

/**
 * `bus-send` 的成品 ⇒ 那一句。三态在线**不许在这一层被抹平**：「发出去了」与「发出去了但没人会读」对用户是两件事。
 */
export function saidOfDelivery(id: string, v: BusSent): string {
  if (!v.registered) return copyText("ccBus.send.ghost", { id });
  if (v.live === false) return copyText("ccBus.send.offline", { id });
  if (v.live === true) return copyText("ccBus.send.delivered", { id });
  return copyText("ccBus.send.unknownLive", { id });
}

/** 给 `origin` 上的 agent `id` 发一条消息（以 cc-monitor 的身份）。回那一句；失败 ⇒ 抛 [`ControlError`]。 */
export async function sendMessage(origin: Origin, id: string, text: string): Promise<string> {
  if (text.trim() === "") throw new ControlError(copyText("ccBus.send.empty"), "empty message, nothing sent");
  const sent = callTyped(origin, "bus-send", { to: id, text, from: MONITOR_BUS_ID }, budgetWithin(WRITE_BUDGET_MS));
  return saidOfDelivery(id, await settleTyped(origin, "bus-send", sent));
}

// ─── 驾驶舱读面（`bus-list` · `bus-inbox`）───
// 后端转调 cc-bus 的机器可读读命令（`cc-list --tsv` · `cc-log`）出成品；形状由握手指纹管。

/** 名单（每一行的 `live` / `ccm_sid` 对身份空间对账，「登记 ≠ 在线」）。 */
export type BusList = BusListWire;

/** 收件箱：消息 ＋ 坏行数 ＋ 截没截。 */
export type BusInbox = BusInboxWire;

/** 读 `origin` 那台的名单。失败 ⇒ 抛 [`ControlError`]（读不到 ≠ 一个都没有）。 */
export async function readRoster(origin: Origin, budgetMs: number = READ_BUDGET_MS): Promise<BusList> {
  return settleTyped(origin, "bus-list", callTyped(origin, "bus-list", {}, budgetWithin(budgetMs)));
}

/** 只读看 `origin` 上 agent `id` 收件箱的尾巴（不推已读位置）。失败 ⇒ 抛 [`ControlError`]。 */
export async function readInbox(origin: Origin, id: string): Promise<BusInbox> {
  return settleTyped(origin, "bus-inbox", callTyped(origin, "bus-inbox", { id }, budgetWithin(READ_BUDGET_MS)));
}
