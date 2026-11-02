/**
 * 「这一趟问的是哪台机器」在 TS 侧的唯一判定处。本机 ＝ `LOCAL`（`"<local>"`），是一台具名的机器，
 * 不是「没说」（本地 ＝ 不走 ssh 的远端）。「是不是本机」只在这里判（[`isLocalOrigin`] / [`isRemoteOrigin`]），
 * 调用处不许自己比：`"<local>"` 是真值，一处 `origin ? … : …` 就会把本机当成远端，`tsc` 一个字都不会说。
 *
 * 类型与值都从生成物来：`Machine`（`generated/Machine.ts`）与 `LOCAL`（`generated/judgment-rules.ts`），
 * 家在 Rust `cmd_wire::machine`；这里只转出。线上的 origin 两个方向都必填（本机也写 `"<local>"`），没有「缺省 ＝ 本机」。
 */
import { LOCAL } from "../generated/judgment-rules";
import type { Machine } from "../generated/Machine";

export { LOCAL };
export type { Machine };

/** 过渡名：界面读者全部改读 `LOCAL` 之后删。 */
export const LOCAL_ORIGIN = LOCAL;
/** 过渡名：界面读者全部改读 `Machine` 之后删。 */
export type Origin = Machine;

/** 这是本机那台吗。 */
export function isLocalOrigin(origin: Machine): boolean {
  return origin === LOCAL;
}

/** 这是一台远端吗（= 不是本机）。 */
export function isRemoteOrigin(origin: Machine): boolean {
  return origin !== LOCAL;
}
