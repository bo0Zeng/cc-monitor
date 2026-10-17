/**
 * **界面直接说控制类帧命令时共用的那一薄层**：等结局、回包认不出怎么抛、机器怎么称呼、置灰那一句从哪取。
 *
 * 一次动作没做成，给人看的那一句由出错那一端写好：通道没成 · 被拒都在 `ChanError.message`（monitor 写；被拒时就是那台后端那一句），
 * 界面原样取（`kit/detail.ts::failSaid` · `saidOf`），不按层、不按码另写。界面只说它自己知道的那一件：回包认不出（[`ReplyUnreadable`]）。
 *
 * ⚠ 本文件**不说 `chan.call`**：`frame_query_tests` 按 `chan.call(` 的字面量操作名数前端经通道说哪几条、
 * `comm_boundary_registry` 的 `X6` 按调用点核期限 —— 那一下必须留在各调用点、操作名写字面量；
 * 这里的 [`settleTyped`] 只接一个已经发出去的 `Promise`。
 */
import { copyText } from "./copy-table";
import { chan, ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, machineName, ReplyUnreadable } from "./ipc/chan-caller";
import { failToast, type ToastAction } from "./kit/toast";
import type { Origin } from "./ipc/origin";

export { machineName };

/**
 * 界面自己判出来的没做成（空消息不发 · 名单里没有它 …）：`message` 是那一句，`detail` 是复制详情（没有 ⇒ 空串）。
 * 通道上的失败不用它（那是 `ChanError`，那一句 monitor 写）。
 */
export class ControlError extends Error {
  constructor(
    said: string,
    readonly detail = "",
  ) {
    super(said);
    this.name = "ControlError";
  }
}

/** 应答形状不对 ⇒ 抛（哪一格不对只进日志；那句话不猜版本）。 */
export function unreadable(origin: Origin, op: string, what: string): ReplyUnreadable {
  return new ReplyUnreadable(origin, `${op} reply ${what}`);
}

/**
 * 等一发 **typed** 帧命令的结局（`callTyped`：回包已按类型解好、形状由握手指纹核过），拿成品。
 * 失败 ⇒ 原样抛（通道上的是 `ChanError`，那一句 monitor 写好；结果未知那一形另带［刷新］）；回的不是 JSON ⇒ 说「读不懂」。
 * `refused`：被拒那一句前面要接上界面才知道的主语时（「刷新被拒 · {said}」那一形）给；只换被拒那一形的那一句，详情照带。
 */
export async function settleTyped<T>(origin: Origin, op: string, sent: Promise<T>, refused?: (said: string) => string): Promise<T> {
  try {
    return await sent;
  } catch (e) {
    if (e instanceof SyntaxError) throw unreadable(origin, op, "is not JSON");
    if (refused && e instanceof ChanError && e.error.layer === "peer" && e.error.why === "refused") {
      throw new ChanError(e.error, refused(e.message), e.detail);
    }
    // 结果未知（那一下可能已经到了那台）⇒ 带上［刷新］，失败 toast 照出（`kit/toast.ts::failToast`）。
    if (e instanceof ChanError && e.error.layer === "hop" && e.error.reach !== "NotSent") throw Object.assign(e, { action: refreshAction(origin) });
    throw e;
  }
}

/** 结果未知 ⇒［刷新］：让那台对齐一次，列表照它补齐，看得出那一下做成了没有。 */
function refreshAction(origin: Origin): ToastAction {
  return {
    label: copyText("control.channel.refresh"),
    run: () => void resync(origin).catch((e: unknown) => failToast(copyText("resync.retry.failed"), e)),
  };
}

/** 期限：后端要等每份 watcher 对完表、打完标（起几次 tmux）。 */
const RESYNC_BUDGET_MS = 20_000;

/** 一次对齐的差异（后端 `resync` 的成品）。 */
export interface Resynced {
  added: number;
  removed: number;
  retagged: number;
  /** 在跟的会话这趟从游标补读出几行（补出来的行照常经流到达）。 */
  caughtUp: number;
  watchers: number;
}

/**
 * **手动对齐**：问那台机器的后端 `resync`（本机与远端同一条）；给了 `sid` 就只对那一个会话。失败 ⇒ 抛（那一句 monitor 写好；被拒前面接「刷新被拒」）。
 * 住这一层：结果未知那一句的［刷新］就是它（落点与按钮见 `resync.ts`）。
 */
export async function resync(origin: Origin, sid?: string): Promise<Resynced> {
  const budget = budgetWithin(RESYNC_BUDGET_MS);
  const req = sid === undefined ? {} : { sid };
  // 回包里那两格能力事实 monitor 在通道那一跳已经收进 `Offer`（唯一住处），界面不读。
  const { added, removed, retagged, caught_up: caughtUp, watchers } = await settleTyped(
    origin,
    "resync",
    callTyped(origin, "resync", req, budget),
    (said) => copyText("resync.call.refused", { said }),
  );
  return { added, removed, retagged, caughtUp, watchers };
}

/**
 * `origin` 那台握手时说过做不到 `op` ⇒ 界面置灰时说的那一句；做得到 / 没把握 ⇒ `null`。
 * 事实只住 monitor 那份 `Offer`（本侧拿的是拷贝，`chan.cachedOffer`）；还没问过那台 ⇒ 去问（不等），这一回照常画。
 */
export function unavailableSaid(origin: Origin, op: string): string | null {
  const offer = chan.cachedOffer(origin);
  if (offer === undefined) void chan.offer(origin);
  return offer?.unavailable.find(([o]) => o === op)?.[2] ?? null;
}
