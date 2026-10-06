/**
 * ↗ 切到终端的结局：后端 / 壳交来的结局族 ⇒ 浮层上的标题 · 正文 · 灰字办法 · 按钮 · 色（规则：认得准才切，认不准照实说、给办法，永远不挑一个窗口切）。
 *
 * 分不清是哪个窗口：照实说拉不了、带候选个数（不切、不闪任何一个）。
 *
 * 结局族的来处（这里不判，只排版）：
 * - 壳 `bring_terminal_to_front` / `bring_remote_terminal_to_front`（`FrontOutcome`）：切过去了 · 分不清（带候选窗口）· 没登记 · 窗口已关 ·
 *   认不准 · 系统不许抢前台 · 窗口归 Windows Terminal 托管 · 真在后台；
 * - 那台后端 `session-terminals` 的 `why`：tmux 里没终端连着 · 没有终端 · 读不了；
 * - 本机后端 `terminal-processes` 的 `why`：那台本地屏幕 · 在另一台电脑 · 经跳板机对不上 · 这一次没查成；
 * - 通道那一跳的归因（`ipc/chan-caller.ts`）：那台旧 · 期限到 · 连不上。
 */
import type { FrontOutcome } from "./generated/FrontOutcome";
import { copyText } from "./copy-table";

export type FrontResult =
  | FrontOutcome
  | { kind: "detached" }
  | { kind: "no-terminal" }
  | { kind: "unreadable" }
  | { kind: "not-ssh" }
  | { kind: "elsewhere"; addr: string }
  | { kind: "mismatch" }
  | { kind: "query-failed"; detail: string }
  | { kind: "bad-shape"; detail: string }
  | { kind: "too-old" }
  | { kind: "timeout" }
  | { kind: "offline" }
  | { kind: "unknown"; detail: string };

/** 浮层上的一颗按钮（做什么由调用方接；名字在文案表）。 */
export type FrontAct =
  | { kind: "connect" }
  | { kind: "open-in-terminal" }
  | { kind: "copy"; detail: string }
  | { kind: "update" }
  | { kind: "retry" }
  | { kind: "reconnect" };

export interface FrontView {
  title: string;
  body: string;
  /** 灰字办法（没有 ⇒ 不出那一行）。 */
  hint: string | null;
  tone: "amber" | "grey" | "red";
  acts: FrontAct[];
}

/**
 * 结局 ⇒ 浮层。切过去了 ⇒ `null`（什么都不出，终端到前面就是回执）。
 * `machine` 是这个会话那台的名字（本机 ⇒ 「本机」）；`inTmux` = 会话在 tmux 里（窗口关了时才给［在终端里打开］）。
 */
export function frontView(r: FrontResult, machine: string, inTmux: boolean): FrontView | null {
  const v = (title: string, body: string, tone: FrontView["tone"], acts: FrontAct[] = [], hint: string | null = null): FrontView => ({ title, body, hint, tone, acts });
  const unsure = copyText("front.title.unsure");
  switch (r.kind) {
    case "switched":
      return null;
    case "several":
      return v(unsure, copyText("front.body.several", { program: r.program, n: r.count }), "amber", [], copyText("front.hint.several"));
    case "unbound":
      return v(copyText("front.title.unbound"), copyText("front.body.unbound"), "amber", [{ kind: "connect" }], copyText("front.hint.unbound"));
    case "detached":
      return v(copyText("front.title.noTerminal"), copyText("front.body.detached"), "grey", [{ kind: "open-in-terminal" }]);
    case "no-terminal":
    case "no-window":
      return v(copyText("front.title.noTerminal"), copyText("front.body.background"), "grey");
    case "elsewhere":
      return v(copyText("front.title.elsewhere"), copyText("front.body.elsewhere", { addr: r.addr }), "grey");
    case "not-ssh":
      return v(copyText("front.title.elsewhere"), copyText("front.body.notSsh", { machine }), "grey");
    case "mismatch":
      return v(unsure, copyText("front.body.mismatch"), "grey");
    case "unreadable":
      return v(unsure, copyText("front.body.unreadable", { machine }), "grey", [{ kind: "copy", detail: copyText("front.body.unreadable", { machine }) }]);
    case "unclear":
      return v(unsure, "", "grey");
    case "too-old":
      return v(copyText("front.title.update", { machine }), copyText("front.body.update"), "amber", [{ kind: "update" }]);
    case "timeout":
      return v(copyText("front.title.failed"), copyText("front.body.timeout", { machine }), "red", [{ kind: "retry" }]);
    case "offline":
      return v(copyText("front.title.failed"), copyText("front.body.offline", { machine }), "red", [{ kind: "reconnect" }]);
    case "refused":
      return v(copyText("front.title.flashing"), copyText("front.body.refused"), "grey");
    case "window-gone":
      return v(copyText("front.title.gone"), "", "grey", inTmux ? [{ kind: "open-in-terminal" }] : []);
    case "hosted-by-wt":
      return v(copyText("front.title.hosted"), copyText("front.body.hosted", { program: r.program }), "amber", [], copyText("front.hint.hosted"));
    case "query-failed":
      return v(copyText("front.title.unknown"), "", "red", [{ kind: "retry" }, { kind: "copy", detail: r.detail }]);
    case "bad-shape":
      return v(copyText("front.title.badShape"), "", "red", [{ kind: "copy", detail: r.detail }]);
    case "unsupported":
      return v(copyText("front.title.failed"), copyText("terminalFront.unavailable.short"), "grey");
    case "unknown":
      return v(copyText("front.title.failed"), copyText("front.body.unknown"), "red", [{ kind: "copy", detail: r.detail }]);
  }
}

/** 一颗按钮上的字。 */
export function frontActLabel(a: FrontAct): string {
  switch (a.kind) {
    case "connect":
      return copyText("front.act.connect");
    case "open-in-terminal":
      return copyText("front.act.openInTerminal");
    case "copy":
      return copyText("front.act.copy");
    case "update":
      return copyText("front.act.update");
    case "retry":
      return copyText("front.act.retry");
    case "reconnect":
      return copyText("front.act.reconnect");
  }
}
