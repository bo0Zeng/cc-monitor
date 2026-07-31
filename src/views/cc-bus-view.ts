/**
 * S6（settings-ia）：cc-bus 驾驶舱**从设置里搬出来**，成为顶层运营视图。
 *
 * # 为什么它不该待在设置里
 *
 * 主计划 §1-1：**它是运营视图，不是设置**。设置回答「这东西怎么配」，驾驶舱回答
 * 「现在谁在跑、给谁发消息」—— 后者跟「字体多大」「daemon 装哪」不是一类东西。
 * S2 那轮把它临时单列成一个设置页，正是为了这一刻删起来只是删一行注册。
 *
 * # 顶栏怎么收：**不加第 7 个图标**
 *
 * 顶栏已有 6 个 28px 图标（设置 / 历史 / 全景 / 用量 / 多 agent 监控 / SFTP），
 * `main.ts` 自陈「顶栏已拥挤」。
 *
 * **仓里已经为这件事立过先例**：F84 加命令面板（Ctrl+K）时，逐字写着
 * 「键位唯一入口（palette 惯例，顶栏已拥挤，按钮延后）」—— 那一次的决定就是
 * **新入口不再占顶栏**。这次再加一个图标等于推翻那个决定，而我没有新理由：
 * 驾驶舱是**低频**运营视图（不是每次开 app 都要看），恰恰是命令面板服务的对象。
 *
 * ⇒ 入口 = 命令面板里的一条（「打开 cc-bus 驾驶舱」）。这也符合面板自陈的范围
 * ——它是「只读命令面板：组装既有 view/dispatcher 目标」，开一个视图属于导航、
 * 不是被首刀排除的写/驱动动作。
 *
 * # 这层壳刻意很薄
 *
 * 只做「overlay 外框 + Esc + 返回」。驾驶舱本体仍是既有的 `CcBusSection`
 * ——它已经过 B03 两轮审计（零定时器、登记≠在线、脏数据如实计数），
 * 那些不变量连同守卫一起原样保留，**不因为换了个容器就重写一遍**。
 */

import { dispatcher } from "../keybindings/registry";
import { CcBusSection } from "../settings/cc-bus-section";

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
    back.textContent = "← 返回";
    back.addEventListener("click", () => this.close());
    const title = document.createElement("span");
    title.className = "cc-bus-view-title";
    title.textContent = "cc-bus 驾驶舱";
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
