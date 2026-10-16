/**
 * 轮换（第五版）的读口：一个会话此刻在它那台的 `rotation-view`（标签页上的账号格照它的 `start` · `current` 画）。
 * 会话所在那台；本机远端同一条 `chan.call(origin, …)`。typed 命令（入参 · 回包从 Rust 类型生成），形状由握手指纹管。
 * 判定全在后端：这里只问、交给画的那几处。轮换那几块界面（会话面板 · 设置页）此刻还没有。
 */
import { budgetWithin, callTyped, refusalIn } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { RotationViewReply } from "./generated/RotationViewReply";

/** 读盘 ＋ 推演 ＋ 回程。 */
const BUDGET_MS = 15_000;

/** 推演只要最短的一段（账号格只看此刻）。 */
const HOURS = 1;

/** 一个会话的 `rotation-view`；中转没见过这个会话（`no_session`）⇒ `null`。别的失败照抛。 */
export async function readSessionView(origin: Origin, sid: string): Promise<RotationViewReply | null> {
  try {
    return await callTyped(origin, "rotation-view", { target: { sid }, hours: HOURS }, budgetWithin(BUDGET_MS));
  } catch (e) {
    if (refusalIn(e)?.code === "no_session") return null;
    throw e;
  }
}
