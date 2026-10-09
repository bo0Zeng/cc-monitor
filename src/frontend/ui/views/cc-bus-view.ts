/**
 * cc-bus 驾驶舱的顶层视图：它是运营视图（现在谁在跑、给谁发消息），不是设置。入口是命令面板里的一条（低频视图不再占顶栏）。
 * 这层壳只做浮层外框 ＋ Esc ＋ 返回；本体是 `CcBusSection`，它的不变量（零定时器 · 登记 ≠ 在线 · 脏数据如实计数）原样保留。
 */

import { dispatcher } from "../keybindings/registry";
import { CcBusSection } from "../settings/cc-bus-section";
import { copyText } from "../copy-table";

export class CcBusView {
  private root: HTMLElement;
  private isOpen = false;
  /** 懒建：不开就不构造（驾驶舱构造时会去读远端机器清单）。 */
  private section: CcBusSection | null = null;
  private bodyEl!: HTMLElement;

  constructor() {
    this.root = this.build();
  }

  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "cc-bus-view";

    const bar = document.createElement("div");
    bar.className = "cc-bus-view-bar";
    const back = document.createElement("button");
    back.type = "button";
    back.className = "cc-bus-view-back";
    back.textContent = copyText("ccBusView.build.back");
    back.addEventListener("click", () => this.close());
    const title = document.createElement("span");
    title.className = "cc-bus-view-title";
    title.textContent = copyText("ccBusView.build.title");
    bar.append(back, title);
    view.appendChild(bar);

    this.bodyEl = document.createElement("div");
    this.bodyEl.className = "cc-bus-view-body";
    view.appendChild(this.bodyEl);
    return view;
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  handleEsc(): void {
    this.close();
  }

  open(): void {
    if (this.isOpen) return;
    if (!this.section) {
      // 首次打开才构造。**不在 app 启动时就建** —— 驾驶舱构造会去拉远端机器清单，
      // 一个从不用 cc-bus 的用户不该为它付那次往返。
      this.section = new CcBusSection();
      this.bodyEl.appendChild(this.section.element);
    }
    document.body.appendChild(this.root);
    this.isOpen = true;
    dispatcher.pushOverlay(this);
  }

  close(): void {
    if (!this.isOpen) return;
    this.root.remove();
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }
}
