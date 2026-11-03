/**
 * ↗ 切到终端的结局 ⇒ 浮层。那一张一律由 `face_core::front` 写好交来（壳拉前之后 · 本机后端 `session-window` 说不出链时 ·
 * 出口这一跳没成时壳照 `front_missed` 写），这里照摆、不按结局挑字：只排语气 ⇒ 色、按钮上的字。
 */
import type { FrontAct as WireAct } from "./generated/FrontAct";
import type { FrontTold } from "./generated/FrontTold";
import type { Tone } from "./generated/Tone";
import { copyText } from "./copy-table";

/** 浮层上的一颗按钮（做什么由调用方接；名字在文案表）：那一张带来的几种，连就地更新失败后的［重试］。 */
export type FrontAct = WireAct | { kind: "retry-update" };

export interface FrontView {
  title: string;
  body: string;
  /** 灰字办法（没有 ⇒ 不出那一行）。 */
  hint: string | null;
  tone: "amber" | "grey" | "red";
  acts: FrontAct[];
}

/** 语气 ⇒ 浮层的色。 */
function shade(t: Tone): FrontView["tone"] {
  if (t === "fail") return "red";
  if (t === "warn" || t === "need") return "amber";
  return "grey";
}

/** 结局 ⇒ 浮层。切过去了 ⇒ `null`（什么都不出，终端到前面就是回执）。 */
export function frontView(r: FrontTold): FrontView | null {
  if (r.kind === "switched") return null;
  return { title: r.title, body: r.body, hint: r.hint ?? null, tone: shade(r.tone), acts: r.acts };
}

/** 一颗按钮上的字。 */
export function frontActLabel(a: FrontAct): string {
  switch (a.kind) {
    case "connect":
      return copyText("front.act.connect");
    case "open-in-terminal":
      return copyText("sessionHead.act.openTerm");
    case "open-here":
      return copyText("front.act.openHere");
    case "copy":
      return copyText("detail.act.copy");
    case "update":
      return copyText("front.act.update");
    case "retry":
    case "retry-update":
      return copyText("front.act.retry");
    case "reconnect":
      return copyText("history.list.reconnect");
  }
}
