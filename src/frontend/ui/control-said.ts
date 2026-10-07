/**
 * **界面直接说控制类帧命令时共用的那一薄层**：一次动作没做成怎么抛、通道三层怎么说人话、
 * 成品形状怎么核、机器怎么称呼。
 *
 * 调用方：`src/frontend/ui/tmux-control.ts`（抓屏 · 结束会话 · 发按键 · 就地 resume）· `src/frontend/ui/cc-bus-control.ts`
 * （cc-bus 查在线 · 发消息 · 收掉 · 派生 · 广播）· `src/frontend/ui/account-ops.ts`（账号库那几条命令）。它们各自的「拒绝码 → 一句话」按动作分表、住各自那一份；
 * **这里只放两边说的是同一件事的那几句**（这台机器够不够得着、答没答、答的读不读得懂）—— 写两份就会各自漂。
 *
 * ⚠ 本文件**不说 `chan.call`**：`frame_query_tests` 按 `chan.call(` 的字面量操作名数前端经通道说哪几条、
 * `comm_boundary_registry` 的 `X6` 按调用点核期限 —— 那一下必须留在各调用点、操作名写字面量；
 * 这里的 [`settle`] 只接一个已经发出去的 `Promise`。
 */
import { copyText } from "./copy-table";
import { chan, ChanError, unavailableCode, type CallError } from "../../comms/inward/chan";
import { machineName, peerVersionSaid, readJson, refusalOf } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";

export { machineName };

/**
 * 一次控制动作没做成。`message` 就是给人看的那一句（已经说成人话）；`detail` 只进日志；
 * `error` 是通道那一跳分好层的结局（失败出在通道上时才有）—— 就地 resume 据它判能不能回落。
 * 调用方拿 [`saidOfControl`] 取那一句，不自己拼。
 */
export class ControlError extends Error {
  readonly detail: string;
  readonly error: CallError | undefined;
  constructor(said: string, detail: string, error?: CallError) {
    super(said);
    this.name = "ControlError";
    this.detail = detail;
    this.error = error;
  }
}

/** 一次控制动作失败 ⇒ 给人看的那一句。本层抛的是 [`ControlError`]；别的（调用方自己的错）原样取 `message`。 */
export function saidOfControl(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);

/** 成品的键集合恰好是 `keys`（多一格 / 缺一格都不收）。 */
export function exactKeys(v: Record<string, unknown>, keys: readonly string[]): boolean {
  const got = Object.keys(v).sort();
  const want = [...keys].sort();
  return got.length === want.length && got.every((k, i) => k === want[i]);
}

/** 应答形状不对 ⇒ 抛（哪一格不对只进 `detail`；那句话按码取，不猜版本）。 */
export function unreadable(origin: Origin, op: string, what: string): ControlError {
  return new ControlError(peerVersionSaid("reply_unreadable", origin), `${op} reply ${what}`);
}

/**
 * 通道三层里「对端说不行」之外的那几层 ⇒ 一句话（说的都是「这台机器够不够得着、答没答」）。
 * `peer/refused` 不在这里 —— 那一层的话按动作分，调用方传进来。
 */
export function saidOfTransport(origin: Origin, err: CallError): string {
  switch (err.layer) {
    case "hop":
      if (err.reach === "NotSent") {
        return isLocalOrigin(origin)
          ? copyText("control.channel.localDown")
          : copyText("control.channel.remoteDown", { machine: origin });
      }
      return copyText("control.channel.unsure", { machine: machineName(origin) });
    case "peer":
      // `unsupported`：那台后端事前就说不认这条命令（比这条动作老）。`refused` 由调用方先接走，走不到这里。
      return peerVersionSaid("backend_old", origin);
    case "ours":
      if (err.why !== "Cancelled") return copyText("control.channel.broken");
      // 那台对这一条不认撤 ⇒ 说它可能还在跑。
      return err.runsOn === true ? copyText("control.channel.cancelledRunsOn") : copyText("control.channel.cancelled");
  }
}

/** 一个动作怎么说「被拒」：拒绝码 ⇒ 一句 · 拒绝体读不出来 ⇒ 一句。 */
export interface Refusals {
  byCode(code: string, detail: string): string;
  noReason(): string;
}

/**
 * 等一发控制类帧命令的结局，拿成品（JSON 值）。
 * 失败 ⇒ 抛 [`ControlError`]（那一句已经按层 / 按码说好了；通道上的失败带着分好层的 `error`）。
 */
export async function settle(origin: Origin, op: string, sent: Promise<Uint8Array>, refusals: Refusals): Promise<unknown> {
  let body: Uint8Array;
  try {
    body = await sent;
  } catch (e) {
    if (!(e instanceof ChanError)) throw e;
    const err = e.error;
    if (err.layer === "peer" && err.why === "refused") {
      const r = refusalOf(err.body);
      throw new ControlError(r ? refusals.byCode(r.code, r.message) : refusals.noReason(), `${op} refused: ${r ? r.code : "unreadable refusal body"}`, err);
    }
    throw new ControlError(saidOfTransport(origin, err), `${op}：${e.message}`, err);
  }
  try {
    return readJson(body);
  } catch {
    throw unreadable(origin, op, "is not JSON");
  }
}

/**
 * `origin` 那台握手时说过做不到 `op` ⇒ 界面置灰时说的那一句；做得到 / 没把握 ⇒ `null`。
 * 事实只住 monitor 那份 `Offer`（本侧拿的是拷贝，`chan.cachedOffer`）；还没问过那台 ⇒ 去问（不等），这一回照常画。
 */
export function unavailableSaid(origin: Origin, op: string): string | null {
  const offer = chan.cachedOffer(origin);
  if (offer === undefined) void chan.offer(origin);
  const code = unavailableCode(offer, op);
  if (code === null) return null;
  return unavailableReason(code, machineName(origin));
}

/**
 * 那台说「做不到」的一个码说成人话（hello 的 `unavailable` · 真调用回的同一个码）。唯一住址：
 * 置灰那一句（{@link unavailableSaid}）与测试连接那一格的「做不到的那几类」（`settings/machine-card.ts`）共用。
 */
export function unavailableReason(code: string, machine: string): string {
  switch (code) {
    case "no_tmux":
      return copyText("control.unavailable.noTmux", { machine });
    case "no_unix_mode":
      return copyText("control.unavailable.noUnixMode", { machine });
    default:
      // 认不出的码：只说做不到（码不上屏）。
      return copyText("control.unavailable.other", { machine });
  }
}
