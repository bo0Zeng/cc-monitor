/**
 * ↗ 切到终端的结局 ⇒ 浮层。那一张由 `face_core::front` 写好交来（壳拉前之后 · 本机后端 `session-window` 说不出链时，用那台的名字写），这里照摆、不按结局挑字。
 *
 * 这里只写出口自己那一跳的失败（后端没答到这里，没人替它写）：本机后端不认这一问 · 期限到（壳那一跳 · 通道那一跳）·
 * 断开（连接层随失败交出的那一句照摆）· 认不出（细节可复制）。
 */
import type { FrontAct as WireAct } from "./generated/FrontAct";
import type { FrontTold } from "./generated/FrontTold";
import type { Tone } from "./generated/Tone";
import { copyText } from "./copy-table";

export type FrontResult =
  | FrontTold
  | { kind: "too-old" }
  | { kind: "timeout" }
  /** `said`：连接层随那一次失败交出的那一句（照摆）。 */
  | { kind: "offline"; said: string }
  | { kind: "unknown"; detail: string }
  | { kind: "bad-shape"; detail: string };

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

/**
 * 结局 ⇒ 浮层。切过去了 ⇒ `null`（什么都不出，终端到前面就是回执）。
 * `machine` 是这个会话那台的名字（只用在出口自己那一跳的失败上）。
 */
export function frontView(r: FrontResult, machine: string): FrontView | null {
  const v = (title: string, body: string, tone: FrontView["tone"], acts: FrontAct[]): FrontView => ({ title, body, hint: null, tone, acts });
  switch (r.kind) {
    case "switched":
      return null;
    case "said":
      return { title: r.title, body: r.body, hint: r.hint ?? null, tone: shade(r.tone), acts: r.acts };
    case "too-old":
      return v(copyText("front.title.update", { machine }), copyText("front.body.update"), "amber", [{ kind: "update" }]);
    case "timeout":
      return v(copyText("front.title.failed"), copyText("front.body.timeout", { machine }), "red", [{ kind: "retry" }]);
    case "offline":
      return v(copyText("front.title.failed"), r.said, "red", [{ kind: "reconnect" }]);
    case "bad-shape":
      return v(copyText("front.title.badShape"), "", "red", [{ kind: "copy", detail: r.detail }]);
    case "unknown":
      return v(copyText("front.title.failed"), copyText("reason.io.unknown"), "red", [{ kind: "copy", detail: r.detail }]);
  }
}

/** 一颗按钮上的字。 */
export function frontActLabel(a: FrontAct): string {
  switch (a.kind) {
    case "connect":
      return copyText("front.act.connect");
    case "open-in-terminal":
      return copyText("front.act.openInTerminal");
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
      return copyText("front.act.reconnect");
  }
}
