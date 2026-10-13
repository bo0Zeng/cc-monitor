/**
 * 轮换（第五版）几处共用的字：提示那一句（后端写好的）配上「还有多久」—— 停发那一句里 `{dur}` 在画的那一刻照回包的 `now` 与
 * `until` 填（会走的钟，`duration-format.ts`）。不带样式（标签页 · 会话里 · 面板都用，不把面板的 CSS 拽进来）。
 */
import { clockNow } from "./duration-format";
import type { RotNote } from "./generated/RotNote";
import type { RotationViewReply } from "./generated/RotationViewReply";

/** 提示的颜色：出错红 · 需手动与停发琥珀 · 只告知中性。 */
export type NoteTone = "error" | "warn" | "info";

const TONE: Record<RotNote["hint"]["k"], NoteTone> = {
  hold: "warn",
  strategyError: "error",
  tzUnknown: "warn",
  oldScripts: "warn",
  wordsBad: "info",
  bookUnreadable: "error",
  newer: "info",
};

/** 一条提示 ⇒ 颜色 ＋ 此刻的那一句。 */
export function hintText(n: RotNote, view: RotationViewReply): { tone: NoteTone; text: string } {
  const until = n.hint.k === "hold" ? n.hint.until : undefined;
  const text = until === undefined ? n.text : clockNow({ text: n.text, from: view.now * 1000, to: until * 1000 }, view.now * 1000);
  return { tone: TONE[n.hint.k], text };
}

/** 停发那一句（没停发 ⇒ `null`）。标签页的悬停 · 会话里的提示都照它。 */
export function holdOf(read: RotationViewReply | null | undefined): string | null {
  if (!read) return null;
  const n = read.hints.find((x) => x.hint.k === "hold");
  return n ? hintText(n, read).text : null;
}
