/**
 * 主窗口一个会话都没有时正文区中央那一块：大字与下面那一行照摆本机后端合各台写好的首屏那一句（`FleetLines` 的 `line` · `hint`：
 * 各台都报完了才说「无会话」，有台还在连 · 没连上就说是哪几台），界面不数台；那一句没来之前什么都不说（还没读到 ≠ 无会话）。
 * 底下是「开始用」卡（`start-card.ts`，本机后端 `first-run` 那一份）：问不到 · 跳过过 · 都做完 ⇒ 不出。「开始用」只在这一处。
 */
import { emptyState } from "./kit/empty";
import type { FleetLines } from "./generated/FleetLines";
import type { Readiness } from "./settings/readiness-reads";
import { startCard, wantsCard, type StartCardHost } from "./start-card";

export class MainEmpty {
  readonly el: HTMLElement;
  private said: FleetLines | null = null;
  private start: Readiness | null = null;

  constructor(private readonly host: StartCardHost) {
    this.el = document.createElement("div");
    this.el.className = "empty-state";
  }

  /** 本机后端合好的那一句（`machine-read` 回包 · `changed {empty}`）。 */
  face(f: FleetLines): void {
    this.said = f;
    this.paint();
  }

  /** 本机后端那一份「开始用」（问不到 ⇒ `null`）。 */
  readiness(r: Readiness | null): void {
    this.start = r;
    this.paint();
  }

  private paint(): void {
    const f = this.said;
    if (!f) {
      this.el.replaceChildren();
      return;
    }
    const face = emptyState({ icon: "terminal", text: f.line, hint: f.hint });
    const r = this.start;
    this.el.replaceChildren(face, ...(wantsCard(r) ? [startCard(r, this.host)] : []));
  }
}
