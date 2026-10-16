/**
 * 主窗口一个会话都没有时正文区中央那一块：「无会话」＋ 怎么让它有；本机终端还没接上时那一行多说一句、给［接上终端］（直达设置那一节）。
 * 接上没有照本机后端 `first-run` 里 `terminal` 那一步（问不到 ⇒ 不催，只有前两句）。
 */
import { copyText } from "./copy-table";
import { button } from "./kit/button";
import { emptyState } from "./kit/empty";
import type { Readiness } from "./settings/readiness-reads";

export class MainEmpty {
  readonly el: HTMLElement;

  constructor(private readonly connect: () => void) {
    this.el = document.createElement("div");
    this.el.className = "empty-state";
    this.paint(false);
  }

  /** 本机后端那一份「开始用」（问不到 ⇒ `null`）。 */
  readiness(r: Readiness | null): void {
    this.paint(r?.steps.some((s) => s.id === "terminal" && !s.done) ?? false);
  }

  private paint(unhooked: boolean): void {
    this.el.replaceChildren(
      emptyState({
        icon: "terminal",
        text: copyText("main.empty.title"),
        hint: unhooked ? copyText("main.empty.hintUnhooked") : copyText("main.empty.hint"),
        ...(unhooked ? { action: button({ label: copyText("main.empty.connect"), icon: "settings", size: "compact", onClick: () => this.connect() }) } : {}),
      }),
    );
  }
}
