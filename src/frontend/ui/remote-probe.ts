/**
 * 〔「后端持有全部 SSH」〕「测试连接」**走通道，本机常驻后端拨一次**：
 * `callTyped(<local>, "remote-probe", {ticket, machine, jump})`。
 *
 * 〔「进度不许倒退」〕阶段**边拨边推**：调用之前先订 `subscribe(<local>, "probe-progress/<ticket>")`，
 * 后端每走完一段推一格 —— 握手那几行（`{stage}`，与 `ConnectStage` 同形）→ `{reached: "ssh"}` → `{reached: "hello"}` →
 * `{reached: "control"}`（ping 往返了）—— 结局是最后一格（`{end}`）。界面边收边画（`onStage`）。
 * 期限归发起方（DL1）：这一问的预算 15 s 由本文件给；到点没等到结局 ⇒ 抛 {@link ProbeStalled}，带「停在哪一段」（最后收到的那一格）。
 *
 * 交过去的是设置页表单里那台（**可能还没保存**）的配置 ＋ 已保存的那一份（表单空着指纹时，同一个 host 才继承）＋ 跳板那一台；
 * 拨号请求在后端组（`src/backend/ssh-dial/machine.rs`）。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { chan, ChanError, type Item } from "../../comms/inward/chan";
import { budgetWithin, callTyped, ReplyUnreadable } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import type { ConnectStage } from "./generated/ConnectStage";
import type { ConnTestResult } from "./generated/ConnTestResult";
import { hostKey, type RemoteHostConfig } from "./remote-config";
import { saidOf } from "./kit/detail";

export type { ConnTestResult };

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

/** 到点没等到结局：`stop` = 最后收到的那一格说的那一段；`message` = 通道没成那一句（monitor 写）。 */
export class ProbeStalled extends Error {
  constructor(
    readonly stop: ProbeStop,
    said: string,
  ) {
    super(said);
    this.name = "ProbeStalled";
  }
}

function bad(): never {
  throw new ReplyUnreadable(LOCAL_ORIGIN, "remoteProbe reply shape");
}

/** 进度流的一格（本机后端与界面同一个构建，照这个形状用、不再逐格核）。 */
type ProbeCell = { stage: ConnectStage } | { reached: ProbeReached } | { end: ConnTestResult };

/**
 * 期限（值归发起方）：原来 monitor 那两段等待（hello 8 s ＋ ping 5 s）的量级 ⇒ 15 s；后端零定时器，到点由宿主那侧撤单。
 * 到点时说得出停在哪一段：进度格边拨边推，最后收到的那一格就是答案。
 */
const PROBE_BUDGET_MS = 15_000;
/** 进度流一开始给的 credit：握手那几行（每个地址至多三行 ＋ 鉴权 ＋ 开通道）＋ 三段 ＋ 结局，远不到这个数。 */
const PROBE_WINDOW = 256;


/**
 * 测一台机器。`saved` = 已保存的全部机器（从里面找跳板那一台）。`onStage` 每收到一行握手阶段调一次（边收边画）。
 * 问不到 ⇒ 抛一句人话；到点没等到结局 ⇒ 抛 {@link ProbeStalled}。
 */
export async function probeMachine(
  machine: RemoteHostConfig,
  saved: readonly RemoteHostConfig[],
  onStage: (st: ConnectStage) => void,
): Promise<ConnTestResult> {
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
          cell = JSON.parse(it.body) as ProbeCell;
        } catch (e) {
          broken = new ReplyUnreadable(LOCAL_ORIGIN, e instanceof Error ? e.message : String(e));
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
        broken = new ReplyUnreadable(LOCAL_ORIGIN, "remoteProbe reply shape");
        arrived();
      }
    }
  };

  const sub = await chan.subscribe(LOCAL_ORIGIN, `${PROBE_PROGRESS_KIND}/${ticket}`, null, PROBE_WINDOW, onItems);
  try {
    try {
      await callTyped(LOCAL_ORIGIN, "remote-probe", { ticket, machine, jump }, budgetWithin(PROBE_BUDGET_MS));
    } catch (e) {
      if (end !== null) return end;
      if (e instanceof ChanError && e.error.layer === "hop" && e.error.why === "Overrun") {
        throw new ProbeStalled(stop, saidOf(e));
      }
      throw new Error(saidOf(e));
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
