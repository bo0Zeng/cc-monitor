/**
 * 〔「后端持有全部 SSH」〕「测试连接」**走通道，本机常驻后端拨一次**：
 * `callTyped(<local>, "remote-probe", {ticket, machine})`。
 *
 * 〔「进度不许倒退」〕**边拨边推**：调用之前先订 `subscribe(<local>, "probe-progress/<ticket>")`，
 * 后端每走一步推一格（{@link ProbeCell}：写好的那一句 ＋ 语气 ＋ 四段），最后一格带结局。界面边收边画（`onCell`）。
 * 期限的值归本文件（{@link PROBE_BUDGET_MS}，随请求信封交过去）；执行在通信层与后端，到点后端收掉那一趟、结局那一格说等了多久停在哪一步。
 *
 * 交过去的只是设置页表单里那台（**可能还没保存**）的配置；它的跳板由本机后端照它的机器表按名字找，
 * 拨号请求在后端组（`src/backend/dial/mod.rs::request_draft`）。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { chan, type Item } from "../../comms/inward/chan";
import type { Host } from "./generated/Host";
import { budgetWithin, callTyped, ReplyUnreadable } from "./ipc/chan-caller";
import { decodeJson } from "./ipc/decode";
import { LOCAL } from "./ipc/origin";
import type { ProbeCell } from "./generated/ProbeCell";
import { PROBE_PROGRESS_KIND } from "./generated/shell-names";
import { saidOf } from "./kit/detail";

export type { ProbeCell };

function bad(): never {
  throw new ReplyUnreadable(LOCAL, "remoteProbe reply shape");
}

/** 这一问的期限（握手 ＋ hello ＋ 一次往返，秒级）。 */
const PROBE_BUDGET_MS = 15_000;
/** 进度流一开始给的 credit：握手那几行（每个地址至多三行 ＋ 鉴权 ＋ 开通道）＋ 两段 ＋ 结局，远不到这个数。 */
const PROBE_WINDOW = 256;

/**
 * 测一台机器（跳板照本机后端的机器表找）。`onCell` 每收到结局之前的一格调一次（边收边画）。
 * 回结局那一格；问不到 ⇒ 抛一句人话。
 */
export async function probeMachine(
  machine: Host,
  onCell: (cell: ProbeCell) => void,
): Promise<ProbeCell> {
  // 一次性的票：进度流的名字（后端只当不透明的串回填）。
  const ticket = crypto.randomUUID();

  let end: ProbeCell | null = null;
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
          cell = decodeJson(it.body) as ProbeCell;
        } catch (e) {
          broken = new ReplyUnreadable(LOCAL, e instanceof Error ? e.message : String(e));
          arrived();
          return;
        }
        if (cell.end === null) {
          onCell(cell);
        } else {
          end = cell;
          arrived();
        }
      } else if (it.t === "gap" || it.t === "closed") {
        // 进度格不许丢；流关了而结局没到 ⇒ 两端契约对不上。
        broken = new ReplyUnreadable(LOCAL, "remoteProbe reply shape");
        arrived();
      }
    }
  };

  const sub = await chan.subscribe(LOCAL, `${PROBE_PROGRESS_KIND}/${ticket}`, null, PROBE_WINDOW, onItems);
  try {
    try {
      await callTyped(LOCAL, "remote-probe", { ticket, machine }, budgetWithin(PROBE_BUDGET_MS));
    } catch (e) {
      if (end !== null) return end;
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
