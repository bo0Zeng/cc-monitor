/**
 * 轮换（第五版）几处共用的字：提示那一句全是后端写好的（停发那一句连「还有多久」也写好了），这里只配颜色。
 * 不带样式（标签页 · 会话里 · 面板都用，不把面板的 CSS 拽进来）。
 */
import type { RotNote } from "./generated/RotNote";
import type { RotationViewReply } from "./generated/RotationViewReply";

/** 横幅的颜色照后端给的语气画：出错红 · 需手动与停发琥珀 · 只告知中性。 */
export type NoteTone = "error" | "warn" | "info";

/** 一条提示 ⇒ 颜色 ＋ 那一句。 */
export function hintText(n: RotNote): { tone: NoteTone; text: string } {
  return { tone: n.tone === "fail" ? "error" : n.tone === "warn" || n.tone === "need" ? "warn" : "info", text: n.text };
}

/** 停发那一句（没停发 ⇒ `null`）。标签页的悬停 · 会话里的提示都照它。 */
export function holdOf(read: RotationViewReply | null | undefined): string | null {
  return read?.hints.find((x) => x.hint.k === "hold")?.text ?? null;
}
