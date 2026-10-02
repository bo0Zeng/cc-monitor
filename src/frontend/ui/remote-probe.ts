/**
 * 〔「后端持有全部 SSH」〕「测试连接」**走通道，本机常驻后端拨一次**：
 * `chan.call(<local>, "remote-probe", {ticket, machine, saved, jump})`。
 *
 * 〔「进度不许倒退」〕阶段**边拨边推**：调用之前先订 `subscribe(<local>, "probe-progress/<ticket>")`，
 * 后端每走完一段推一格 —— 握手那几行（`{stage}`，与 `ConnectStage` 同形）→ `{reached: "ssh"}` → `{reached: "hello"}` →
 * `{reached: "control"}`（ping 往返了）—— 结局是最后一格（`{end}`）。界面边收边画（`onStage`）。
 * 期限归发起方（DL1）：这一问的预算 15 s 由本文件给；到点没等到结局 ⇒ 抛 {@link ProbeStalled}，带「停在哪一段」（最后收到的那一格）。
 *
 * 交过去的是设置页表单里那台（**可能还没保存**）的配置 ＋ 已保存的那一份（表单空着指纹时，同一个 host 才继承）＋ 跳板那一台；
 * 拨号请求在后端组（`src/backend/dial/machine.rs`）。按形状严格收；本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { chan, ChanError, type Item } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, saidOf } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import { copyText } from "./copy-table";
import type { ConnectStage } from "./generated/ConnectStage";
import { hostKey, type RemoteHostConfig } from "./remote-config";

/** 测试连接的结局。 */
export interface ConnTestResult {
  /** SSH 连接 + 鉴权是否成功。 */
  sshOk: boolean;
  /** 握手时观察到的 server host key 指纹（`SHA256:...`）。失败时恒 `null`（免得把失配的 key 固化）。 */
  fingerprint: string | null;
  /** 竞速胜出的地址（`host:port`）。 */
  endpoint: string | null;
  /** 那台后端回了 hello 没有。 */
  backendOk: boolean;
  /** 那台后端的三格（版本 · 能用几项 / 做不到几项 · 往返毫秒），后端按文案表拼好的一句（不再是 `v=.. build=..` 日志行）。 */
  backendHello: string | null;
  /** 这台说做不到的那几类：码 ＋ 这一类几项（后端按 hello 的 `unavailable` 分好；空 = 没有 / 没回 hello）。人话在界面说（`control-said.ts::unavailableReason`）。 */
  backendGaps: { code: string; count: number }[];
  /** 人读的总体状态 / 失败原因。 */
  message: string;
}

/** 那条进度流（与 Rust `event_replay.rs::PROBE_PROGRESS_KIND` 同一个串；后面跟 `/<ticket>`）。流名刻意不叫命令名（命令是 `remote-probe`）。 */
export const PROBE_PROGRESS_KIND = "probe-progress";

/** 走完了的那几段（`{reached}` 格）。 */
export type ProbeReached = "ssh" | "hello" | "control";

/** 没等到结局时停在哪一段：还没收到任何一格 · 握手中（最后一行阶段）· 等那台后端的 hello · 控制通道往返（ping）。 */
export type ProbeStop =
  | { at: "start" }
  | { at: "handshake"; last: ConnectStage }
  | { at: "hello" }
  | { at: "control" };

/** 到点没等到结局：`stop` = 最后收到的那一格说的那一段；`message` = 通道那一层怎么说的（按层，`saidOf`）。 */
export class ProbeStalled extends Error {
  constructor(
    readonly stop: ProbeStop,
    said: string,
  ) {
    super(said);
    this.name = "ProbeStalled";
  }
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Obj, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const nullableStr = (v: unknown): v is string | null => v === null || typeof v === "string";

function bad(): never {
  throw new Error(copyText("remoteProbe.reply.badShape"));
}

/** 结局那一格的体。严格收。 */
export function decodeProbe(v: unknown): ConnTestResult {
  if (
    !isObj(v) ||
    !sameKeys(v, ["sshOk", "fingerprint", "endpoint", "backendOk", "backendHello", "backendGaps", "message"]) ||
    typeof v.sshOk !== "boolean" ||
    !nullableStr(v.fingerprint) ||
    !nullableStr(v.endpoint) ||
    typeof v.backendOk !== "boolean" ||
    !nullableStr(v.backendHello) ||
    !Array.isArray(v.backendGaps) ||
    !v.backendGaps.every(
      (g) => isObj(g) && sameKeys(g, ["code", "count"]) && typeof g.code === "string" && Number.isInteger(g.count),
    ) ||
    typeof v.message !== "string"
  ) {
    bad();
  }
  return {
    sshOk: v.sshOk,
    fingerprint: v.fingerprint,
    endpoint: v.endpoint,
    backendOk: v.backendOk,
    backendHello: v.backendHello,
    backendGaps: v.backendGaps as { code: string; count: number }[],
    message: v.message,
  };
}

/** 进度流的一格。 */
export type ProbeCell = { stage: ConnectStage } | { reached: ProbeReached } | { end: ConnTestResult };

/** 严格收一格：恰好一个键；阶段行只查是对象、带 `kind`（各形的字段由画它的那一处读）。 */
export function decodeCell(v: unknown): ProbeCell {
  if (!isObj(v) || Object.keys(v).length !== 1) bad();
  if ("stage" in v) {
    const st = v.stage;
    if (!isObj(st) || typeof st.kind !== "string") bad();
    return { stage: st as unknown as ConnectStage };
  }
  if ("reached" in v) {
    const r = v.reached;
    if (r !== "ssh" && r !== "hello" && r !== "control") bad();
    return { reached: r };
  }
  if ("end" in v) return { end: decodeProbe(v.end) };
  bad();
}

/**
 * 期限（值归发起方）：原来 monitor 那两段等待（hello 8 s ＋ ping 5 s）的量级 ⇒ 15 s；后端零定时器，到点由宿主那侧撤单。
 * 到点时说得出停在哪一段：进度格边拨边推，最后收到的那一格就是答案。
 */
const PROBE_BUDGET_MS = 15_000;
/** 进度流一开始给的 credit：握手那几行（每个地址至多三行 ＋ 鉴权 ＋ 开通道）＋ 三段 ＋ 结局，远不到这个数。 */
const PROBE_WINDOW = 256;

/** 本机后端比这一问老（不认这条命令）时的那句话。 */
const OLD_BACKEND = copyText("remoteProbe.backend.tooOld");

/**
 * 测一台机器。`saved` = 已保存的全部机器（从里面找「已保存的那一份」与跳板那一台）。`onStage` 每收到一行握手阶段调一次（边收边画）。
 * 问不到 ⇒ 抛一句人话；到点没等到结局 ⇒ 抛 {@link ProbeStalled}。
 */
export async function probeMachine(
  machine: RemoteHostConfig,
  saved: readonly RemoteHostConfig[],
  onStage: (st: ConnectStage) => void,
): Promise<ConnTestResult> {
  const mine = saved.find((h) => hostKey(h) === hostKey(machine)) ?? null;
  const jumpName = machine.jump.trim();
  const jump = jumpName ? (saved.find((h) => hostKey(h) === jumpName) ?? null) : null;
  // 一次性的票：进度流的名字（后端只当不透明的串回填）。
  const ticket = crypto.randomUUID();

  let stop: ProbeStop = { at: "start" };
  let end: ConnTestResult | null = null;
  let broken: Error | null = null;
  let arrived: () => void = () => {};
  const ended = new Promise<void>((res) => {
    arrived = res;
  });
  const onItems = (items: Item[]): void => {
    for (const it of items) {
      if (end !== null || broken !== null) return;
      if (it.t === "frame") {
        let cell: ProbeCell;
        try {
          cell = decodeCell(JSON.parse(it.body));
        } catch (e) {
          broken = e instanceof Error ? e : new Error(String(e));
          arrived();
          return;
        }
        if ("stage" in cell) {
          stop = { at: "handshake", last: cell.stage };
          onStage(cell.stage);
        } else if ("reached" in cell) {
          stop = cell.reached === "ssh" ? { at: "hello" } : { at: "control" };
        } else {
          end = cell.end;
          arrived();
        }
      } else if (it.t === "gap" || it.t === "closed") {
        // 进度格不许丢（丢了就说不清停在哪）；流关了而结局没到 ⇒ 两端契约对不上。
        broken = new Error(copyText("remoteProbe.reply.badShape"));
        arrived();
      }
    }
  };

  const sub = await chan.subscribe(LOCAL_ORIGIN, `${PROBE_PROGRESS_KIND}/${ticket}`, null, PROBE_WINDOW, onItems);
  try {
    try {
      const body = jsonBody({ ticket, machine, saved: mine, jump });
      const budget = budgetWithin(PROBE_BUDGET_MS);
      await chan.call(LOCAL_ORIGIN, "remote-probe", body, budget);
    } catch (e) {
      if (end !== null) return end;
      if (e instanceof ChanError && e.error.layer === "hop" && e.error.why === "Overrun") {
        throw new ProbeStalled(stop, saidOf(e, OLD_BACKEND));
      }
      throw new Error(saidOf(e, OLD_BACKEND));
    }
    // 结局那一格排在应答之前发（同一条应答通道），但两条路到界面的先后不保证 ⇒ 等它到。
    await ended;
    if (broken !== null) throw broken;
    if (end === null) bad();
    return end;
  } finally {
    sub.stop();
  }
}
