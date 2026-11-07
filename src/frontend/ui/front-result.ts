/**
 * 「终端」拉窗那一下的结局 ⇒ 底部一条（一次性结局全产品只这一种样子：锚在按钮上的浮层 · 打勾都不另起）。那一张一律由 `face_core::front` 写好交来
 * （壳拉前之后 · 本机后端 `session-window` 说不出链时 · 出口这一跳没成时壳照 `front_missed` 写），这里照摆、不按结局挑字：
 * 只排语气 ⇒ 底部条的级别、按钮上的字。切过去了 ⇒ 什么都不出（窗口到了前面就是回执）。
 * 连那一张都没问到（壳那一问失败）⇒ 照壳交来的那一句进失败提示（详情可复制）。
 */
import type { FrontAct as WireAct } from "./generated/FrontAct";
import type { FrontTold } from "./generated/FrontTold";
import type { Tone } from "./generated/Tone";
import { copyText } from "./copy-table";
import { failToast, toast, type ToastAction, type ToastLevel } from "./kit/toast";

/** 底部条上的一颗按钮（做什么由调用方接；名字在文案表）：那一张带来的几种，连就地更新失败后的［重试］。 */
export type FrontAct = WireAct | { kind: "retry-update" };

export interface FrontView {
  title: string;
  body: string;
  /** 灰字办法（没有 ⇒ 不出那一行）。 */
  hint: string | null;
  tone: Tone;
  acts: FrontAct[];
}

/** 语气 ⇒ 底部条的级别。 */
function levelOf(t: Tone): ToastLevel {
  if (t === "fail") return "error";
  if (t === "warn" || t === "need") return "warn";
  return "info";
}

/** 结局 ⇒ 那一张。切过去了 ⇒ `null`（什么都不出，终端到前面就是回执）。 */
export function frontView(r: FrontTold): FrontView | null {
  if (r.kind === "switched") return null;
  return { title: r.title, body: r.body, hint: r.hint ?? null, tone: r.tone, acts: r.acts };
}

/** 一颗按钮上的字。 */
export function frontActLabel(a: Exclude<FrontAct, { kind: "copy" }>): string {
  switch (a.kind) {
    case "connect":
      return copyText("front.act.connect");
    case "open-in-terminal":
      return copyText("mobDrawer.item.terminal");
    case "open-here":
      return copyText("front.act.openHere");
    case "update":
      return copyText("front.act.update");
    case "retry":
    case "retry-update":
      return copyText("front.act.retry");
    case "reconnect":
      return copyText("history.list.reconnect");
  }
}

/** 那一张 ⇒ 底部一条：正文与灰字办法进灰字；［复制详情］照 `copy` 那一颗出；其余按钮点了交 `act`。 */
export function showFrontResult(view: FrontView, act: (a: FrontAct) => void | Promise<void>): void {
  const detail = [view.body, view.hint ?? ""].filter((x) => x !== "").join("\n");
  const copy = view.acts.find((a): a is Extract<FrontAct, { kind: "copy" }> => a.kind === "copy");
  const action: ToastAction[] = view.acts
    .filter((a): a is Exclude<FrontAct, { kind: "copy" }> => a.kind !== "copy")
    .map((a) => ({ label: frontActLabel(a), run: () => void act(a) }));
  toast(view.title, detail, { level: levelOf(view.tone), ...(action.length > 0 ? { action } : {}), ...(copy ? { detail: copy.detail } : {}) });
}

/** 连那一张都没问到（壳那一问失败，`bringTerminalToFront` 拒了）：照壳交来的那一句进全产品那条失败提示（标题是那一颗的名字「终端」，详情可复制）。 */
export function frontAskFailed(e: unknown): void {
  failToast(copyText("mobDrawer.item.terminal"), e);
}
