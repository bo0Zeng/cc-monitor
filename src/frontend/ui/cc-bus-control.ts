/**
 * **界面直接说的 cc-bus 那几条帧命令** —— 名单 · 查某个 agent 在不在线（`bus-list`）· 读收件箱（`bus-inbox`）。
 *
 * 都由界面经 `chan.call` 直接问那台机器的后端，本机与远端同一条路（本机由 `<local>` 那条长连接答）。
 * 起会话走「开会话」（`session-open`），「谁起的谁」读血缘 —— 总线这里不起、不收会话。
 * 界面不发信：人要说话就在会话里说；agent 之间发信走 `cc-bus send`（信封由后端 `bus-envelope` 那一处核）。
 *
 * # 本文件做的只有两件（都是调用方那一侧的事）
 *
 * 1. **按类型问**：入参 · 回包都是后端类型生成的（`generated/`），回包形状由握手时的形状指纹管（对不上就不发、说「后端版本不同」），
 *    这里不逐格判。agent id 的形状不在这里判：规则只有一份（`shell_quote_core::bus_id_ok`），后端交给 `cc-bus log` 之前先判、
 *    判不过回 `bad_id`（`INVARIANTS §47`）。
 * 2. **说人话**：拒绝码逐码一句（认不出的原样带出去）；通道三层走 `src/frontend/ui/control-said.ts` 那一份。句子住文案表 `ccBus.*`。
 *
 * # 🔴 查在线：「问不到」**不是**「不在线」
 *
 * [`agentOnline`] 只回 `true` / `false` 两个确定的答案；问不到（没通道 · 后端太旧 · 那一趟失败 · 它不在名单里 ·
 * `live` 是 `null`）一律**抛**。调用方把抛出来的那一句渲染成「查不到」，这条路上没有地方能造出一盏灭灯。
 *
 * # 期限（调用点显式给）
 *
 * 查在线 15 秒 · 读名单 · 读收件箱 30 秒。
 */
import { copyText } from "./copy-table";
import { ControlError, machineName, settleTyped } from "./control-said";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { BusList as BusListWire } from "./generated/BusList";
import type { BusInbox as BusInboxWire } from "./generated/BusInbox";

const ONLINE_BUDGET_MS = 15_000;
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

// ─── 驾驶舱读面（`bus-list` · `bus-inbox`）───
// 后端转调 cc-bus 的机器可读读命令（`cc-bus list --tsv` · `cc-bus log`）出成品；形状由握手指纹管。

/** 名单（每一行一个收件箱；`title` · `live` 由后端判，`live` 判不了是 null）。 */
export type BusList = BusListWire;

/** 收件箱：信封 ＋ 坏行数 ＋ 截没截。 */
export type BusInbox = BusInboxWire;

/** 读 `origin` 那台的名单。失败 ⇒ 抛 [`ControlError`]（读不到 ≠ 一个都没有）。 */
export async function readRoster(origin: Origin, budgetMs: number = READ_BUDGET_MS): Promise<BusList> {
  return settleTyped(origin, "bus-list", callTyped(origin, "bus-list", {}, budgetWithin(budgetMs)));
}

/** 只读看 `origin` 上 agent `id` 收件箱的尾巴（不推已读位置）。失败 ⇒ 抛 [`ControlError`]。 */
export async function readInbox(origin: Origin, id: string): Promise<BusInbox> {
  return settleTyped(origin, "bus-inbox", callTyped(origin, "bus-inbox", { id }, budgetWithin(READ_BUDGET_MS)));
}
