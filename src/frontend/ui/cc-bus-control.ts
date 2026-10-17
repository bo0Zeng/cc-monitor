/**
 * **界面直接说的 cc-bus 那几条帧命令** —— 查某个 agent 在不在线（`bus-list`）·
 * 发消息（`bus-send`）· 收掉（`bus-kill`）· 派生（`bus-spawn`）· 广播（`bus-broadcast`）。
 *
 * 五件都由界面经 `chan.call` 直接问那台机器的后端（广播的「列名单 ＋ 逐个发」在后端），本机与远端同一条路（本机由 `<local>` 那条长连接答）。
 * 驾驶舱的读面（`bus-state` · `bus-inbox`）在文件末尾。
 *
 * # 本文件做的只有三件（都是调用方那一侧的事）
 *
 * 1. **先核入参**：正文非空 · 派生的形状（[`checkSpawnShape`]：目录非空）。过不了就一个字节都不发。
 *    agent id / 派生账号名的形状不在这里判：规则只有一份（`shell_quote_core::bus_id_ok`），后端交给 `cc-send` / `cc-kill` / `cc-spawn` 之前先判、
 *    判不过回 `bad_id`（`INVARIANTS §47`），这里把那个码说成人话。
 * 2. **按类型问**：入参 · 回包都是后端类型生成的（`generated/`），回包形状由握手时的形状指纹管（对不上就不发、说「后端版本不同」），
 *    这里不逐格判。
 * 3. **说人话**：回值的几态逐态一句（发消息：在线 / 不在线 / 名字没登记过 / 问不到；收掉：真收了 / 只摘了陈旧登记 /
 *    两样都没发生；广播：发到几个 · 跳过几个 · 失败几个**分开说**）；拒绝码逐码一句（认不出的原样带出去）；
 *    通道三层走 `src/frontend/ui/control-said.ts` 那一份。句子住文案表 `ccBus.*`。
 *
 * # 🔴 查在线：「问不到」**不是**「不在线」
 *
 * [`agentOnline`] 只回 `true` / `false` 两个确定的答案；问不到（没通道 · 后端太旧 · 那一趟失败 · 它不在名单里 ·
 * `live` 是 `null`）一律**抛**。调用方把抛出来的那一句渲染成「查不到」，这条路上没有地方能造出一盏灭灯。
 *
 * # 期限（调用点显式给）
 *
 * 查在线 15 秒 · 发消息 / 收掉 / 广播 30 秒 · 派生 90 秒。
 */
import { copyText } from "./copy-table";
import { arrivedBody, expectArrival } from "./launch-arrival";
import { asSaid, ControlError, machineName, settleTyped, type Refusals } from "./control-said";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { BusSent } from "./generated/BusSent";
import type { BusKilled } from "./generated/BusKilled";
import type { BusSpawnedNow } from "./generated/BusSpawnedNow";
import type { BusBroadcast } from "./generated/BusBroadcast";
import type { BusState as BusStateWire } from "./generated/BusState";
import type { BusInbox as BusInboxWire } from "./generated/BusInbox";

/** cc-monitor 自己在总线上的身份 —— 发消息 / 广播时用它，收信人才不会看到 `unknown`（回复仍无归宿）。 */
export const MONITOR_BUS_ID = "cc-monitor";

const ONLINE_BUDGET_MS = 15_000;
const WRITE_BUDGET_MS = 30_000;
const SPAWN_BUDGET_MS = 90_000;

// id 的规则只有一份（`shell_quote_core::bus_id_ok`），后端入口先判（`bad_id`），下面三张拒绝表各一句。
// 查在线（`bus-list`）不把 id 交给任何人（只在名单里找）⇒ 不判：找不到就是「查不到」。

// ─── 查在线（`bus-list`）───

function listRefusals(id: string): Refusals {
  return asSaid(() => copyText("ccBus.online.noReason", { id }));
}

/**
 * 问 `origin` 上的 agent `id` 此刻在不在线（名单里那份**核过身份**的三态）。
 * 只回确定的答案；问不到 ⇒ 抛 [`ControlError`]（「问不到」不是「不在线」，见头注）。
 * 这是刻意的一次往返：只在用户点某一行的「检查」时问，不默认全量查、不轮询。
 */
export async function agentOnline(origin: Origin, id: string): Promise<boolean> {
  const sent = callTyped(origin, "bus-list", {}, budgetWithin(ONLINE_BUDGET_MS));
  const { agents } = await settleTyped(origin, "bus-list", sent, listRefusals(id));
  const hit = agents.find((a) => a.id === id);
  if (hit === undefined || hit.live === null) {
    throw new ControlError(copyText("ccBus.online.unknown", { id, machine: machineName(origin) }), "not in the roster, or live is null");
  }
  return hit.live;
}

// ─── 发消息（`bus-send`）───

function sendRefusals(id: string): Refusals {
  return asSaid(() => copyText("ccBus.send.noReason", { id }));
}

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
  return saidOfDelivery(id, await settleTyped(origin, "bus-send", sent, sendRefusals(id)));
}

// ─── 收掉（`bus-kill`）───

function killRefusals(id: string): Refusals {
  return asSaid(() => copyText("ccBus.kill.noReason", { id }));
}

/** `bus-kill` 的成品 ⇒ 那一句。三态分开说：真收了 · 身份对不上**只摘了陈旧登记而会话没动** · 两样都没发生。 */
export function saidOfKill(id: string, v: BusKilled): string {
  if (v.killed) return copyText("ccBus.kill.killed", { id });
  if (v.stale_only) return copyText("ccBus.kill.staleOnly", { id });
  return copyText("ccBus.kill.nothing", { id });
}

/** 收掉 `origin` 上的 agent `id`（**破坏性，不可撤销** —— 调用方两步确认）。回那一句；失败 ⇒ 抛 [`ControlError`]。 */
export async function killAgent(origin: Origin, id: string): Promise<string> {
  const sent = callTyped(origin, "bus-kill", { id }, budgetWithin(WRITE_BUDGET_MS));
  return saidOfKill(id, await settleTyped(origin, "bus-kill", sent, killRefusals(id)));
}

// ─── 派生（`bus-spawn`）───

/** 派生那一趟要交给后端的几样。`account` 空串 / 缺 = **显式**用基座（发 `base:true`，不存在「什么都不传」这一档）。 */
export interface SpawnRequest {
  /** 派生哪一家（空 ⇒ 默认那一家）。认不认由那台后端按注册表判，认不出就拒、说出认得的几家。 */
  tool: string;
  dir: string;
  task: string;
  account?: string;
}

/**
 * 派生交给后端之前这一侧自己判的形状：`dir` 非空。
 * 不判 `tool`：空 ⇒ 默认那一家，认不认都归那台后端（注册表那一处）。账号名的字符集归后端（`bad_id`）。过不了 ⇒ 抛那一句。
 */
export function checkSpawnShape(req: SpawnRequest): void {
  if (req.dir.trim() === "") throw new ControlError(copyText("ccBus.spawn.needDir"), "empty working directory");
}

function spawnRefusals(): Refusals {
  return asSaid(() => copyText("ccBus.spawn.noReason"));
}

/**
 * `bus-spawn` 的成品 ⇒ 那一句。`id` 是 `null` 时**不许**说成「没起来」（后端从回显里认名字，认不出只说明换了说法，
 * 会话多半已经在跑 —— 说成失败，用户会重试、再起一个真 agent）。
 */
export function saidOfSpawn(v: BusSpawnedNow): string {
  const said = v.said.trim();
  return v.id === null ? copyText("ccBus.spawn.noName", { said }) : copyText("ccBus.spawn.done", { id: v.id, said });
}

/** 在 `origin` 上派生一个协作 agent。**会起一个真实 agent 进程（消耗额度）** —— 调用方先让用户确认。 */
export async function spawnAgent(origin: Origin, req: SpawnRequest): Promise<string> {
  checkSpawnShape(req);
  const account = req.account !== undefined && req.account !== "" ? { account: req.account } : { base: true };
  const sent = callTyped(origin, "bus-spawn", { tool: req.tool, dir: req.dir, task: req.task, ...account }, budgetWithin(SPAWN_BUDGET_MS));
  const v = await settleTyped(origin, "bus-spawn", sent, spawnRefusals());
  const said = saidOfSpawn(v);
  // cc-spawn 回了不等于 agent 起来了（pane 里当场退回 shell 那一形它照样回 0）：
  //   等那台报出一条在这个目录里新起的会话再说；没见到就抓那个 tmux 会话的一屏当原话。
  const id = v.id;
  expectArrival({
    origin,
    match: { cwd: req.dir },
    tmuxName: id,
    arrived: { title: copyText("ccBus.spawn.arrived"), body: arrivedBody(origin) },
    slot: { cwd: req.dir },
  });
  return said;
}

// ─── 广播（`bus-broadcast`）───

function broadcastRefusals(): Refusals {
  return asSaid(() => copyText("ccBus.broadcast.noReason"));
}

/**
 * `bus-broadcast` 的成品 ⇒ 那一句。三个数**分开说**（发到几个 · 跳过几个不在线的 · 失败几个），
 * 问不到谁在线时明说「全发了」—— 合成一个「已向 N 个 agent 发出广播」正是老路的病（那个 N 把幽灵也算进去）。
 */
export function saidOfBroadcast(v: BusBroadcast): string {
  const failed = v.failed.map((f) => `${f.id}（${f.error}）`);
  const sent = v.sent;
  const skipped = v.skipped_offline;
  if (v.liveness_unknown) {
    return failed.length === 0
      ? copyText("ccBus.broadcast.doneUnknown", { sent })
      : copyText("ccBus.broadcast.doneUnknownFailed", { sent, failed: failed.length, who: failed.join(copyText("ccBus.broadcast.listSep")) });
  }
  return failed.length === 0
    ? copyText("ccBus.broadcast.done", { sent, skipped })
    : copyText("ccBus.broadcast.doneFailed", { sent, skipped, failed: failed.length, who: failed.join(copyText("ccBus.broadcast.listSep")) });
}

/** 向 `origin` 上**在线**的 agent 广播一条（以 cc-monitor 的身份；挑人规则住后端）。回那一句；失败 ⇒ 抛 [`ControlError`]。 */
export async function broadcast(origin: Origin, text: string): Promise<string> {
  if (text.trim() === "") throw new ControlError(copyText("ccBus.broadcast.empty"), "empty broadcast text, nothing sent");
  const sent = callTyped(origin, "bus-broadcast", { text, from: MONITOR_BUS_ID }, budgetWithin(WRITE_BUDGET_MS));
  return saidOfBroadcast(await settleTyped(origin, "bus-broadcast", sent, broadcastRefusals()));
}

// ─── 驾驶舱读面（`bus-state` · `bus-inbox`）───
// 后端转调 cc-bus 的机器可读读命令（`cc-list --tsv` · `cc-agents --tsv` · `cc-log`）出成品；形状由握手指纹管。

const READ_BUDGET_MS = 30_000;

/** 名册 ＋ 派生台账 ＋ 坏行数（名册一行的 `live` / `ccm_sid` 与 `bus-list` 同一套：对身份空间对账，「登记 ≠ 在线」）。 */
export type BusState = BusStateWire;

/** 收件箱：消息 ＋ 坏行数 ＋ 截没截。 */
export type BusInbox = BusInboxWire;

function readRefusals(): Refusals {
  return asSaid(() => copyText("ccBus.read.noReason"));
}

/** 读 `origin` 那台的名册 ＋ 派生台账（登记时间 · 派生时间 · 坏行数）。失败 ⇒ 抛 [`ControlError`]（读不到 ≠ 一个都没有）。 */
export async function readState(origin: Origin): Promise<BusState> {
  return settleTyped(origin, "bus-state", callTyped(origin, "bus-state", {}, budgetWithin(READ_BUDGET_MS)), readRefusals());
}

/** 只读看 `origin` 上 agent `id` 收件箱的尾巴（不推已读位置）。失败 ⇒ 抛 [`ControlError`]。 */
export async function readInbox(origin: Origin, id: string): Promise<BusInbox> {
  return settleTyped(origin, "bus-inbox", callTyped(origin, "bus-inbox", { id }, budgetWithin(READ_BUDGET_MS)), readRefusals());
}
