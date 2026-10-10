/**
 * 会话状态的那几个词：状态点的读屏名 / 悬停名（`dotLabel`）· 在等你时等的是什么（`needsWord`）。
 * 主窗口（标签页 · 会话头 · 悬停卡）与设置窗（轮换规则的在用名单）都从这里取，同一个状态一个说法。
 * 只依赖文案表与类型：设置窗的模块图里不许带 tab 管理那一套（`entry-graphs.vitest.ts`）。
 */
import type { DotState } from "./kit/status-dot";
import type { NeedsKind } from "./session-reads";
import { copyText } from "./copy-table";

/** 状态点的读屏名 / 悬停名。 */
export function dotLabel(d: DotState): string {
  switch (d) {
    case "running":
      return copyText("sessionFace.dot.running");
    case "needs-you":
      return copyText("sessionFace.dot.needs");
    case "idle":
      return copyText("sessionFace.dot.idle");
    case "exited":
      return copyText("sessionState.reconnectable.name");
    case "ended":
      return copyText("sessionState.ended.name");
    case "unknown":
      return copyText("sessionState.unseen.name");
    case "gone":
      return copyText("sessionState.gone.name");
    case "failed":
      return copyText("sessionFace.dot.failed");
  }
}

/** 「等批准 / 等回答 / 需手动」（标签页行尾 · 会话头 · 悬停卡）。计划也写「等批准」。 */
export function needsWord(kind: NeedsKind): string {
  switch (kind) {
    case "approve":
      return copyText("tabBar.needsKind.approve");
    case "answer":
      return copyText("tabBar.needsKind.answer");
    case "plan":
      return copyText("tabBar.needsKind.plan");
    case "network":
      return copyText("tabBar.needsKind.network");
    case "worker":
      return copyText("tabBar.needsKind.worker");
    case "goal":
      return copyText("tabBar.needsKind.goal");
    case "choose":
      return copyText("tabBar.needsKind.choose");
    case "unknown":
      return copyText("tabBar.needsKind.unknown");
  }
}
