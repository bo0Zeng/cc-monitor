/**
 * 设置面板：通用可折叠分组（issue #7）。
 *
 * 用来把"低频但占位大"的 section 收起来：外观、诊断、快捷键。
 * 高频 section（数据、PowerShell 集成）保持平铺。
 *
 * 状态持久化到 localStorage：key = `cc-monitor.settings.collapsed.<id>`
 * 值 "1" = 折叠，"0" = 展开。读不到时用 `defaultCollapsed`。
 *
 * 展开动画用 grid-template-rows: 0fr ↔ 1fr 技巧 ——
 * 不用预测内容高度，纯 CSS 平滑过渡。
 */

import { makeInfoIcon } from "./info-icon";
import { LS_KEYS, safeGet, safeSet } from "../local-storage";

export interface CollapsibleGroupOptions {
  /** localStorage key 后缀，必须稳定（不要随翻译改） */
  id: string;
  /** 显示给用户的分组标题（"外观"/"诊断"…） */
  title: string;
  /** 默认是否折叠。无 localStorage 值时用这个。 */
  defaultCollapsed?: boolean;
  /** 可选：标题旁边的 i 图标 hover 文案 */
  infoTooltip?: string;
}

export class CollapsibleGroup {
  private root: HTMLElement;
  private header: HTMLElement;
  private arrow: HTMLElement;
  private bodyShell: HTMLElement;
  private body: HTMLElement;
  private collapsed: boolean;
  private storageKey: string;

  constructor(opts: CollapsibleGroupOptions) {
    this.storageKey = LS_KEYS.settingsCollapsed(opts.id);
    this.collapsed = this.loadCollapsedState(opts.defaultCollapsed ?? true);

    this.root = document.createElement("div");
    this.root.className = "settings-group settings-collapsible";

    // header：可点击；title + 箭头
    this.header = document.createElement("div");
    this.header.className = "settings-collapsible-header";
    this.header.setAttribute("role", "button");
    this.header.setAttribute("tabindex", "0");
    this.header.setAttribute(
      "aria-expanded",
      this.collapsed ? "false" : "true",
    );

    this.arrow = document.createElement("span");
    this.arrow.className = "settings-collapsible-arrow";
    this.arrow.textContent = "▶";
    this.header.appendChild(this.arrow);

    const titleEl = document.createElement("span");
    titleEl.className = "settings-collapsible-title";
    titleEl.textContent = opts.title;
    this.header.appendChild(titleEl);

    if (opts.infoTooltip) {
      this.header.appendChild(makeInfoIcon(opts.infoTooltip));
    }

    const toggle = () => this.setCollapsed(!this.collapsed);
    this.header.addEventListener("click", (e) => {
      // info icon 内部有自己的点击行为；点 icon 不要触发折叠
      const target = e.target as HTMLElement;
      if (target.closest(".settings-info-icon")) return;
      toggle();
    });
    this.header.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        toggle();
      }
    });

    this.root.appendChild(this.header);

    // body：双层 div，外层 grid 控制高度，内层 overflow:hidden
    this.bodyShell = document.createElement("div");
    this.bodyShell.className = "settings-collapsible-body";
    this.body = document.createElement("div");
    this.body.className = "settings-collapsible-body-inner";
    this.bodyShell.appendChild(this.body);
    this.root.appendChild(this.bodyShell);

    this.applyCollapsedClass();
  }

  /** 往 body 里塞内容（典型用法：append 子 section / field 列表） */
  appendChild(node: Node): void {
    this.body.appendChild(node);
  }

  /** panel.ts 把这个 element 塞到 .settings-body 里 */
  get element(): HTMLElement {
    return this.root;
  }

  private setCollapsed(next: boolean): void {
    if (next === this.collapsed) return;
    this.collapsed = next;
    this.applyCollapsedClass();
    this.header.setAttribute("aria-expanded", next ? "false" : "true");
    safeSet(this.storageKey, next ? "1" : "0");
  }

  private applyCollapsedClass(): void {
    this.root.classList.toggle("collapsed", this.collapsed);
    this.arrow.classList.toggle("expanded", !this.collapsed);
    this.bodyShell.classList.toggle("expanded", !this.collapsed);
  }

  private loadCollapsedState(fallback: boolean): boolean {
    const v = safeGet(this.storageKey);
    if (v === "1") return true;
    if (v === "0") return false;
    return fallback;
  }
}
