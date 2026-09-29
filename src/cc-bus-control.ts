/**
 * 〔C4e · 第四波 4C · `设计/05 §8` 步 5〕**界面直接说的 cc-bus 那几条帧命令** —— 查某个 agent 在不在线（`bus-list`）·
 * 发消息（`bus-send`）· 收掉（`bus-kill`）· 派生（`bus-spawn`）· 广播（`bus-broadcast`）。
 *
 * # 它顶掉了什么
 *
 * 此前设置页的 cc-bus 驾驶舱经 monitor 的五条 Tauri 命令做这几件事：`check_cc_bus_agent_online` / `cc_bus_send` /
 * `cc_bus_kill` / `cc_bus_spawn` / `cc_bus_broadcast`〔散文墓碑〕。monitor 在每一条上做的都只是：先核 id / 参数形状、
 * 转一条后端帧命令、把回值与失败说成人话 —— 广播则是 monitor 里的一个**组合**（列名单 ＋ 逐个发）。
 * ⇒ 按 `设计/05 §14.3`（业务解释只有一个家）迁：广播这个组合先收进后端（新帧命令 `bus-broadcast`），
 * 五件都由界面经 `chan.call` 直接问那台机器的后端，解释只剩本文件这一份，monitor 那五条命令与那一份解释一起删了。
 * **本机与远端同一条路**（本机由 `<local>` 那条长连接答）。
 *
 * 读面两条（名单 ＋ spawn 台账 · 收件箱）**不在这里**：它们今天是 monitor 里的 shell 读，不是帧命令（`bus-state` 那条
 * 具名读命令在，但名单那几格它答不全 —— 见后端 `control/cc_bus.rs::state_for_inbound` 头注），留给后面一批。
 *
 * # 本文件做的只有三件（都是调用方那一侧的事）
 *
 * 1. **先核入参**：正文非空 · 派生的形状（[`checkSpawnShape`]：选了 tool、目录非空）。过不了就一个字节都不发。
 *    〔DUP2 · 主会话 09-26 裁 J12〕agent id / 派生账号名的**形状**不在这里判了：规则只有一份（`shell_quote_core::bus_id_ok`），
 *    后端在把它交给 `cc-send` / `cc-kill` / `cc-spawn` 之前先判、判不过回 `bad_id`（`INVARIANTS §47` 那一格改写成「后端交给 `cc-send` 之前」，
 *    `设计/90 §3` 判据 2）；这里把那个码说成人话。
 * 2. **按形状收**：成品恰好是那几格、类型对 ⇒ 收；否则当成「两边版本对不上」抛，不猜
 *    （破坏性的收掉 / 派生：形状不认识 ⇒ **不知道动没动**，那句话明说别直接重来）。
 *    线上形状由跨语言金样钉着（后端产出 == 金样 · 本文件读同一份）。
 * 3. **说人话**：回值的几态逐态一句（发消息：在线 / 不在线 / 名字没登记过 / 问不到；收掉：真收了 / 只摘了陈旧登记 /
 *    两样都没发生；广播：发到几个 · 跳过几个 · 失败几个**分开说**）；拒绝码逐码一句（认不出的原样带出去）；
 *    通道三层走 `src/control-said.ts` 那一份。句子住文案表 `ccBus.*`。
 *
 * # 🔴 查在线：「问不到」**不是**「不在线」
 *
 * [`agentOnline`] 只回 `true` / `false` 两个确定的答案；问不到（没通道 · 后端太旧 · 那一趟失败 · 它不在名单里 ·
 * `live` 是 `null`）一律**抛**。调用方把抛出来的那一句渲染成「查不到」，这条路上没有地方能造出一盏灭灯。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 查在线 15 秒 · 发消息 / 收掉 / 广播 30 秒 · 派生 90 秒 —— 与它们上一个住址（monitor 那几个发送端）同值。
 */
import { copyText } from "./copy-table";
import { arrivedBody, expectArrival } from "./launch-arrival";
import { ControlError, exactKeys, isObj, machineName, settle, unreadable, type Refusals } from "./control-said";
import { chan } from "./comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

/** cc-monitor 自己在总线上的身份 —— 发消息 / 广播时用它，收信人才不会看到 `unknown`（回复仍无归宿，见 ROADMAP `U17`）。 */
export const MONITOR_BUS_ID = "cc-monitor";

const ONLINE_BUDGET_MS = 15_000;
const WRITE_BUDGET_MS = 30_000;
const SPAWN_BUDGET_MS = 90_000;

// 〔DUP2 · J12〕这里原来有 `isValidBusId`〔散文墓碑〕与 `refuseBadId`〔散文墓碑〕—— 与 monitor 读收件箱那一份同一条规则的 TS 副本，
// 发 / 收 / 查在线之前先核。今天规则只有一份、住共享 crate（`shell_quote_core::bus_id_ok`），后端入口先判（`bad_id`），
// 下面三张拒绝表各一句。查在线（`bus-list`）不把 id 交给任何人（只在名单里找）⇒ 不判：找不到就是「查不到」。

// ─── 查在线（`bus-list`）───

/** `bus-list` 名单里的一行（后端 `control/cc_bus.rs::join_identity` 那一形，键名一字不差）。 */
export interface BusAgent {
  id: string;
  target: string;
  unread: number;
  /** 三态：`true` 在 · `false` 不在 · `null` 问不到（不是不在）。 */
  live: boolean | null;
  ccm_sid: string | null;
}

/** `bus-list` 的成品 ⇒ 名单。形状不对 ⇒ 抛。 */
export function decodeAgents(origin: Origin, v: unknown): BusAgent[] {
  if (!isObj(v) || !exactKeys(v, ["agents"]) || !Array.isArray(v.agents)) {
    throw unreadable(origin, "bus-list", "is not exactly {agents} (one array)");
  }
  return v.agents.map((a): BusAgent => {
    if (
      !isObj(a) ||
      !exactKeys(a, ["id", "target", "unread", "live", "ccm_sid"]) ||
      typeof a.id !== "string" ||
      typeof a.target !== "string" ||
      typeof a.unread !== "number" ||
      !(a.live === null || typeof a.live === "boolean") ||
      !(a.ccm_sid === null || typeof a.ccm_sid === "string")
    ) {
      throw unreadable(origin, "bus-list", "has a row that is not the five fields");
    }
    return { id: a.id, target: a.target, unread: a.unread, live: a.live, ccm_sid: a.ccm_sid };
  });
}

function listRefusals(id: string): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "not_installed":
          return copyText("ccBus.online.notInstalled", { id, detail });
        case "timed_out":
          return copyText("ccBus.online.timedOut", { id, detail });
        default:
          return copyText("ccBus.online.otherCode", { id, code, detail });
      }
    },
    noReason: () => copyText("ccBus.online.noReason", { id }),
  };
}

/**
 * 问 `origin` 上的 agent `id` 此刻在不在线（名单里那份**核过身份**的三态）。
 * 只回确定的答案；问不到 ⇒ 抛 [`ControlError`]（「问不到」不是「不在线」，见头注）。
 * 这是刻意的一次往返：只在用户点某一行的「检查」时问，不默认全量查、不轮询。
 */
export async function agentOnline(origin: Origin, id: string): Promise<boolean> {
  const payload = jsonBody({});
  const budget = budgetWithin(ONLINE_BUDGET_MS);
  const v = await settle(origin, "bus-list", chan.call(origin, "bus-list", payload, budget), listRefusals(id));
  const hit = decodeAgents(origin, v).find((a) => a.id === id);
  if (hit === undefined || hit.live === null) {
    throw new ControlError(copyText("ccBus.online.unknown", { id, machine: machineName(origin) }), "not in the roster, or live is null");
  }
  return hit.live;
}

// ─── 发消息（`bus-send`）───

function sendRefusals(id: string): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "invalid_args":
          return copyText("ccBus.send.invalidArgs", { id, detail });
        case "bad_id":
          return copyText("ccBus.send.badId", { detail });
        case "not_installed":
          return copyText("ccBus.send.notInstalled", { id, detail });
        case "rejected":
          return copyText("ccBus.send.rejected", { id, detail });
        case "timed_out":
          return copyText("ccBus.send.timedOut", { id, detail });
        case "too_long":
          return copyText("ccBus.send.tooLong", { id, detail });
        default:
          return copyText("ccBus.send.otherCode", { id, code, detail });
      }
    },
    noReason: () => copyText("ccBus.send.noReason", { id }),
  };
}

/**
 * `bus-send` 的成品 ⇒ 那一句。三态在线**不许在这一层被抹平**：「发出去了」与「发出去了但没人会读」对用户是两件事。
 * 形状不对 ⇒ 抛。
 */
export function saidOfDelivery(origin: Origin, id: string, v: unknown): string {
  if (
    !isObj(v) ||
    !exactKeys(v, ["to", "sent", "registered", "live", "from"]) ||
    v.sent !== true ||
    typeof v.registered !== "boolean" ||
    !(v.live === null || typeof v.live === "boolean") ||
    !(v.from === null || typeof v.from === "string")
  ) {
    throw unreadable(origin, "bus-send", "is not the five fields (or sent is not true)");
  }
  if (!v.registered) return copyText("ccBus.send.ghost", { id });
  if (v.live === false) return copyText("ccBus.send.offline", { id });
  if (v.live === true) return copyText("ccBus.send.delivered", { id });
  return copyText("ccBus.send.unknownLive", { id });
}

/** 给 `origin` 上的 agent `id` 发一条消息（以 cc-monitor 的身份）。回那一句；失败 ⇒ 抛 [`ControlError`]。 */
export async function sendMessage(origin: Origin, id: string, text: string): Promise<string> {
  if (text.trim() === "") throw new ControlError(copyText("ccBus.send.empty"), "empty message, nothing sent");
  const payload = jsonBody({ to: id, text, from: MONITOR_BUS_ID });
  const budget = budgetWithin(WRITE_BUDGET_MS);
  const v = await settle(origin, "bus-send", chan.call(origin, "bus-send", payload, budget), sendRefusals(id));
  return saidOfDelivery(origin, id, v);
}

// ─── 收掉（`bus-kill`）───

function killRefusals(id: string): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "invalid_args":
          return copyText("ccBus.kill.invalidArgs", { id, detail });
        case "bad_id":
          return copyText("ccBus.kill.badId", { detail });
        case "not_installed":
          return copyText("ccBus.kill.notInstalled", { id, detail });
        case "timed_out":
          return copyText("ccBus.kill.timedOut", { id, detail });
        default:
          return copyText("ccBus.kill.otherCode", { id, code, detail });
      }
    },
    noReason: () => copyText("ccBus.kill.noReason", { id }),
  };
}

/**
 * `bus-kill` 的成品 ⇒ 那一句。三态分开说：真收了 · 身份对不上**只摘了陈旧登记而会话没动** · 两样都没发生。
 * 形状不认识 ⇒ **不知道它动没动**（破坏性动作上不许说成「没收掉」—— 下一步就是在未知状态上再做一次）。
 */
export function saidOfKill(origin: Origin, id: string, v: unknown): string {
  if (!isObj(v) || !exactKeys(v, ["id", "killed", "stale_only"]) || typeof v.killed !== "boolean" || typeof v.stale_only !== "boolean") {
    throw new ControlError(copyText("ccBus.kill.unsure", { id, machine: machineName(origin) }), "bus-kill reply is not exactly {id, killed, stale_only}");
  }
  if (v.killed) return copyText("ccBus.kill.killed", { id });
  if (v.stale_only) return copyText("ccBus.kill.staleOnly", { id });
  return copyText("ccBus.kill.nothing", { id });
}

/** 收掉 `origin` 上的 agent `id`（**破坏性，不可撤销** —— 调用方两步确认）。回那一句；失败 ⇒ 抛 [`ControlError`]。 */
export async function killAgent(origin: Origin, id: string): Promise<string> {
  const payload = jsonBody({ id });
  const budget = budgetWithin(WRITE_BUDGET_MS);
  const v = await settle(origin, "bus-kill", chan.call(origin, "bus-kill", payload, budget), killRefusals(id));
  return saidOfKill(origin, id, v);
}

// ─── 派生（`bus-spawn`）───

/** 派生那一趟要交给后端的几样。`account` 空串 / 缺 = **显式**用基座（发 `base:true`，不存在「什么都不传」这一档）。 */
export interface SpawnRequest {
  tool: string;
  dir: string;
  task: string;
  account?: string;
}

/**
 * 派生交给后端之前这一侧自己判的形状：`tool` / `dir` 非空。
 * ⚠ **刻意不白名单 `tool`**：认不认归 `cc-spawn`（后端那侧同一条）。过不了 ⇒ 抛那一句。
 * 〔DUP2 · J12〕账号名的字符集原来也在这里判（`isValidBusId`〔散文墓碑〕）；今天归后端（`bad_id`，同一个 `bus_id_ok`）。
 */
export function checkSpawnShape(req: SpawnRequest): void {
  if (req.tool.trim() === "") throw new ControlError(copyText("ccBus.spawn.noTool"), "no tool chosen");
  if (req.dir.trim() === "") throw new ControlError(copyText("ccBus.spawn.noDir"), "empty working directory");
}

function spawnRefusals(): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "invalid_args":
          return copyText("ccBus.spawn.invalidArgs", { detail });
        case "bad_id":
          return copyText("ccBus.spawn.badId", { detail });
        case "not_installed":
          return copyText("ccBus.spawn.notInstalled", { detail });
        case "timed_out":
          return copyText("ccBus.spawn.timedOut", { detail });
        default:
          return copyText("ccBus.spawn.otherCode", { code, detail });
      }
    },
    noReason: () => copyText("ccBus.spawn.noReason"),
  };
}

/**
 * `bus-spawn` 的成品 ⇒ 那一句。`id` 是 `null` 时**不许**说成「没起来」（后端从回显里认名字，认不出只说明换了说法，
 * 会话多半已经在跑 —— 说成失败，用户会重试、再起一个真 agent）。形状不认识 ⇒ **不知道起没起**。
 */
export function saidOfSpawn(v: unknown): string {
  if (
    !isObj(v) ||
    !exactKeys(v, ["spawned", "id", "said"]) ||
    v.spawned !== true ||
    !(v.id === null || typeof v.id === "string") ||
    typeof v.said !== "string"
  ) {
    throw new ControlError(copyText("ccBus.spawn.unsure"), "bus-spawn reply is not exactly {spawned, id, said} (or spawned is not true)");
  }
  const said = v.said.trim();
  return v.id === null ? copyText("ccBus.spawn.noName", { said }) : copyText("ccBus.spawn.done", { id: v.id, said });
}

/** 在 `origin` 上派生一个协作 agent。**会起一个真实 agent 进程（消耗额度）** —— 调用方先让用户确认。 */
export async function spawnAgent(origin: Origin, req: SpawnRequest): Promise<string> {
  checkSpawnShape(req);
  const args: Record<string, unknown> = { tool: req.tool, dir: req.dir, task: req.task };
  if (req.account !== undefined && req.account !== "") args.account = req.account;
  else args.base = true;
  const payload = jsonBody(args);
  const budget = budgetWithin(SPAWN_BUDGET_MS);
  const v = await settle(origin, "bus-spawn", chan.call(origin, "bus-spawn", payload, budget), spawnRefusals());
  const said = saidOfSpawn(v);
  // 〔FIX3 · `设计/99 §2.2 ②`〕cc-spawn 回了不等于 agent 起来了（pane 里当场退回 shell 那一形它照样回 0）：
  //   等那台报出一条在这个目录里新起的会话再说；没见到就抓那个 tmux 会话的一屏当原话。
  const id = isObj(v) && typeof v.id === "string" ? v.id : null;
  expectArrival({
    origin,
    match: { cwd: req.dir },
    tmuxName: id,
    arrived: { title: copyText("ccBus.spawn.arrived"), body: arrivedBody(origin) },
  });
  return said;
}

// ─── 广播（`bus-broadcast`）───

function broadcastRefusals(): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "invalid_args":
          return copyText("ccBus.broadcast.invalidArgs", { detail });
        case "not_installed":
          return copyText("ccBus.broadcast.notInstalled", { detail });
        case "timed_out":
          return copyText("ccBus.broadcast.timedOut", { detail });
        // 〔DUP3〕给的 `from` 形状不对（后端交给 `cc-send` 之前先判，一个人都没发）；后端原话说是哪个值。
        case "bad_id":
          return copyText("ccBus.broadcast.badId", { detail });
        default:
          return copyText("ccBus.broadcast.otherCode", { code, detail });
      }
    },
    noReason: () => copyText("ccBus.broadcast.noReason"),
  };
}

/**
 * `bus-broadcast` 的成品 ⇒ 那一句。三个数**分开说**（发到几个 · 跳过几个不在线的 · 失败几个），
 * 问不到谁在线时明说「全发了」—— 合成一个「已向 N 个 agent 发出广播」正是老路的病（那个 N 把幽灵也算进去）。
 */
export function saidOfBroadcast(origin: Origin, v: unknown): string {
  if (
    !isObj(v) ||
    !exactKeys(v, ["sent", "skipped_offline", "liveness_unknown", "failed"]) ||
    typeof v.sent !== "number" ||
    typeof v.skipped_offline !== "number" ||
    typeof v.liveness_unknown !== "boolean" ||
    !Array.isArray(v.failed)
  ) {
    throw unreadable(origin, "bus-broadcast", "is not the four fields");
  }
  const failed = v.failed.map((f) => {
    if (!isObj(f) || !exactKeys(f, ["id", "error", "detail"]) || typeof f.id !== "string" || typeof f.error !== "string") {
      throw unreadable(origin, "bus-broadcast", "has a failed entry that is not the three fields");
    }
    return `${f.id}（${f.error}）`;
  });
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
  const payload = jsonBody({ text, from: MONITOR_BUS_ID });
  const budget = budgetWithin(WRITE_BUDGET_MS);
  const v = await settle(origin, "bus-broadcast", chan.call(origin, "bus-broadcast", payload, budget), broadcastRefusals());
  return saidOfBroadcast(origin, v);
}

// ─── 驾驶舱读面（`bus-state` · `bus-inbox`）〔SH1 · V136〕───
// 后端转调 cc-bus 新加的机器可读读命令（`cc-list --tsv` · `cc-agents --tsv` · `cc-log`），出成品；这里按形状严格收
// （金样 `tests/__fixtures__/cc-bus-read.golden.json`），monitor 那一整套 shell 读删了。

const READ_BUDGET_MS = 30_000;

/** 名册一行：`live` / `ccm_sid` 与 `bus-list` 同一套（对身份空间对账，「登记 ≠ 在线」）。 */
export interface BusRosterRow {
  id: string;
  target: string;
  registered_at: string;
  unread: number;
  live: boolean | null;
  ccm_sid: string | null;
}

/** 派生台账一行：`live` 三态，`null` = 核不了（不是不在）。 */
export interface BusSpawnedRow {
  id: string;
  dir: string;
  spawned_at: string;
  task: string;
  live: boolean | null;
}

export interface BusState {
  agents: BusRosterRow[];
  spawned: BusSpawnedRow[];
  skipped: number;
}

export interface BusMessage {
  from: string;
  ts: string;
  text: string;
  class: string;
}

export interface BusInbox {
  messages: BusMessage[];
  skipped: number;
  truncated: boolean;
}

const str = (x: unknown): x is string => typeof x === "string";
const tri = (x: unknown): x is boolean | null => x === null || typeof x === "boolean";

/** `bus-state` 的成品 ⇒ 名册 ＋ 台账 ＋ 坏行数。形状不对 ⇒ 抛（「两端契约对不上」，不猜）。 */
export function decodeState(origin: Origin, v: unknown): BusState {
  if (!isObj(v) || !exactKeys(v, ["agents", "spawned", "skipped"]) || !Array.isArray(v.agents) || !Array.isArray(v.spawned) || typeof v.skipped !== "number") {
    throw unreadable(origin, "bus-state", "is not exactly {agents, spawned, skipped}");
  }
  const agents = v.agents.map((a): BusRosterRow => {
    if (!isObj(a) || !exactKeys(a, ["id", "target", "registered_at", "unread", "live", "ccm_sid"]) || !str(a.id) || !str(a.target) || !str(a.registered_at) || typeof a.unread !== "number" || !tri(a.live) || !(a.ccm_sid === null || str(a.ccm_sid))) {
      throw unreadable(origin, "bus-state", "has a roster row that is not the six fields");
    }
    return { id: a.id, target: a.target, registered_at: a.registered_at, unread: a.unread, live: a.live, ccm_sid: a.ccm_sid };
  });
  const spawned = v.spawned.map((s): BusSpawnedRow => {
    if (!isObj(s) || !exactKeys(s, ["id", "dir", "spawned_at", "task", "live"]) || !str(s.id) || !str(s.dir) || !str(s.spawned_at) || !str(s.task) || !tri(s.live)) {
      throw unreadable(origin, "bus-state", "has a spawned row that is not the five fields");
    }
    return { id: s.id, dir: s.dir, spawned_at: s.spawned_at, task: s.task, live: s.live };
  });
  return { agents, spawned, skipped: v.skipped };
}

/** `bus-inbox` 的成品 ⇒ 消息 ＋ 坏行数 ＋ 截没截。形状不对 ⇒ 抛。 */
export function decodeInbox(origin: Origin, v: unknown): BusInbox {
  if (!isObj(v) || !exactKeys(v, ["messages", "skipped", "truncated"]) || !Array.isArray(v.messages) || typeof v.skipped !== "number" || typeof v.truncated !== "boolean") {
    throw unreadable(origin, "bus-inbox", "is not exactly {messages, skipped, truncated}");
  }
  const messages = v.messages.map((m): BusMessage => {
    if (!isObj(m) || !exactKeys(m, ["from", "ts", "text", "class"]) || !str(m.from) || !str(m.ts) || !str(m.text) || !str(m.class)) {
      throw unreadable(origin, "bus-inbox", "has a message that is not the four fields");
    }
    return { from: m.from, ts: m.ts, text: m.text, class: m.class };
  });
  return { messages, skipped: v.skipped, truncated: v.truncated };
}

function readRefusals(): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "not_installed":
          return copyText("ccBus.read.notInstalled", { detail });
        case "timed_out":
          return copyText("ccBus.read.timedOut", { detail });
        case "bad_id":
          return copyText("ccBus.read.badId", { detail });
        default:
          return copyText("ccBus.read.otherCode", { code, detail });
      }
    },
    noReason: () => copyText("ccBus.read.noReason"),
  };
}

/** 读 `origin` 那台的名册 ＋ 派生台账（登记时间 · 派生时间 · 坏行数）。失败 ⇒ 抛 [`ControlError`]（读不到 ≠ 一个都没有）。 */
export async function readState(origin: Origin): Promise<BusState> {
  const payload = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  const v = await settle(origin, "bus-state", chan.call(origin, "bus-state", payload, budget), readRefusals());
  return decodeState(origin, v);
}

/** 只读看 `origin` 上 agent `id` 收件箱的尾巴（不推已读位置）。失败 ⇒ 抛 [`ControlError`]。 */
export async function readInbox(origin: Origin, id: string): Promise<BusInbox> {
  const payload = jsonBody({ id });
  const budget = budgetWithin(READ_BUDGET_MS);
  const v = await settle(origin, "bus-inbox", chan.call(origin, "bus-inbox", payload, budget), readRefusals());
  return decodeInbox(origin, v);
}
