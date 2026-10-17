/**
 * 〔「后端持有全部 SSH」〕「测试连接」**走通道，本机常驻后端拨一次**：
 * `callTyped(<local>, "remote-probe", {ticket, machine, jump})`。
 *
 * 〔「进度不许倒退」〕**边拨边推**：调用之前先订 `subscribe(<local>, "probe-progress/<ticket>")`，
 * 后端每走一步推一格（{@link ProbeCell}：写好的那一句 ＋ 语气 ＋ 四段），最后一格带结局。界面边收边画（`onCell`）。
 * 期限的值归本文件（{@link PROBE_BUDGET_MS}，随请求信封交过去）；执行在通信层与后端，到点后端收掉那一趟、结局那一格说等了多久停在哪一步。
 *
 * 交过去的是设置页表单里那台（**可能还没保存**）的配置 ＋ 已保存的那一份（表单空着指纹时，同一个 host 才继承）＋ 跳板那一台；
 * 拨号请求在后端组（`src/backend/ssh-dial/machine.rs`）。本机后端不在 ⇒ 通道那一层报（D11，不回落）。
 */
import { chan, type Item } from "../../comms/inward/chan";
import { budgetWithin, callTyped, ReplyUnreadable } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./backend-policy";
import type { ProbeCell } from "./generated/ProbeCell";
import { hostKey, type RemoteHostConfig } from "./remote-config";
import { saidOf } from "./kit/detail";

export type { ProbeCell };

/** 那条进度流（与 Rust `event_replay.rs::PROBE_PROGRESS_KIND` 同一个串；后面跟 `/<ticket>`）。流名刻意不叫命令名（命令是 `remote-probe`）。 */
export const PROBE_PROGRESS_KIND = "probe-progress";

function bad(): never {
  throw new ReplyUnreadable(LOCAL_ORIGIN, "remoteProbe reply shape");
}

/** 这一问的期限（握手 ＋ hello ＋ 一次往返，秒级）。 */
const PROBE_BUDGET_MS = 15_000;
/** 进度流一开始给的 credit：握手那几行（每个地址至多三行 ＋ 鉴权 ＋ 开通道）＋ 两段 ＋ 结局，远不到这个数。 */
const PROBE_WINDOW = 256;

/**
 * 测一台机器。`saved` = 已保存的全部机器（从里面找跳板那一台）。`onCell` 每收到结局之前的一格调一次（边收边画）。
 * 回结局那一格；问不到 ⇒ 抛一句人话。
 */
export async function probeMachine(
  machine: RemoteHostConfig,
  saved: readonly RemoteHostConfig[],
  onCell: (cell: ProbeCell) => void,
): Promise<ProbeCell> {
  const jumpName = machine.jump.trim();
  const jump = jumpName ? (saved.find((h) => hostKey(h) === jumpName) ?? null) : null;
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
          cell = JSON.parse(it.body) as ProbeCell;
        } catch (e) {
          broken = new ReplyUnreadable(LOCAL_ORIGIN, e instanceof Error ? e.message : String(e));
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
