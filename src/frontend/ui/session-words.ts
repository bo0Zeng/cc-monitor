/**
 * 状态点那几档的读屏名 / 悬停名（`dotLabel`）：活着的会话照抄核心写的字，这里只剩不是活着的那几档与失败那一档。
 * 只依赖文案表与类型：设置窗的模块图里不许带 tab 管理那一套（`entry-graphs.vitest.ts`）。
 */
import type { DotState } from "./kit/status-dot";
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
