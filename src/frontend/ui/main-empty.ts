/**
 * 主窗口一个会话都没有时正文区中央那一块（A2-17-6-2）：大字与下面那一行照摆本机后端合各台写好的首屏那一句（`FleetLines` 的 `line` · `hint`：
 * 各台都报完了才说「无会话」，有台还在连 · 没连上就说是哪几台），界面不数台；那一句没来之前什么都不说（还没读到 ≠ 无会话）。
 * 本机终端还没接上时给［接上终端］（直达设置那一节）：接上没有照本机后端 `first-run` 里 `terminal` 那一步（问不到 ⇒ 不催）。
 */
import { copyText } from "./copy-table";
import { button } from "./kit/button";
import { emptyState } from "./kit/empty";
import type { FleetLines } from "./generated/FleetLines";
import type { Readiness } from "./settings/readiness-reads";

export class MainEmpty {
  readonly el: HTMLElement;
  private said: FleetLines | null = null;
  private unhooked = false;

  constructor(private readonly connect: () => void) {
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
    this.unhooked = r?.steps.some((s) => s.id === "terminal" && !s.done) ?? false;
    this.paint();
  }

  private paint(): void {
    const f = this.said;
    if (!f) {
      this.el.replaceChildren();
      return;
    }
    this.el.replaceChildren(
      emptyState({
        icon: "terminal",
        text: f.line,
        hint: f.hint,
        ...(this.unhooked ? { action: button({ label: copyText("front.act.connect"), icon: "settings", size: "compact", onClick: () => this.connect() }) } : {}),
      }),
    );
  }
}
