/**
 * 设置窗左栏导航 ＋ 页路由（也用于页内分栏，`orientation: "horizontal"`）。
 *
 * - 页内容是已经建好的元素，路由器只管显隐；同一时刻恰好一页可见。
 * - 导航项：图标或状态点 · 名字 · 角标（0 不画，99+）；子项缩进（只一层）。
 * - 窄于 640 宽导航收成内容区顶上的一个下拉（样式表的容器查询切换）。
 * - 键盘：导航里 ↑↓ / Home / End 按看见的顺序走（非当前项退出 Tab 序）。
 */

import { icon, type IconName } from "../kit/icon";
import { countBadge } from "../kit/badge";
import { statusDot, setDot, type MachineDotState } from "../kit/status-dot";
import { copyText } from "../copy-table";

export interface SettingsRoute {
  id: string;
  title: string;
  element: HTMLElement;
  /** 父页 id：有父的项缩进挂在父项之下（只一层）。 */
  parentId?: string;
  /** 顶项的图标。 */
  icon?: IconName;
  /** 与上一项之间留一段空（分组）。 */
  gapBefore?: boolean;
  /** 页头右侧的动作（主按钮 · ⋯）。 */
  headActions?: HTMLElement[];
  /** 页头下一行灰字。 */
  sub?: string;
  /** 不画页头（页自己有卡头，如机器页）。 */
  hideHead?: boolean;
}

export interface SettingsRouterOptions {
  orientation?: "vertical" | "horizontal";
  /** 页内分栏不画页头（名字已在栏上）。 */
  hidePageHeader?: boolean;
  /** 第一次打开落在哪一页。 */
  landingId: string;
}

interface Entry {
  route: SettingsRoute;
  navButton: HTMLButtonElement;
  label: HTMLSpanElement;
  badge: HTMLSpanElement | null;
  dot: HTMLSpanElement | null;
  option: HTMLOptionElement | null;
}

export class SettingsRouter {
  private readonly root: HTMLElement;
  private readonly nav: HTMLElement;
  private readonly content: HTMLElement;
  private readonly narrowSelect: HTMLSelectElement | null;
  private readonly landingId: string;
  private readonly hidePageHeader: boolean;
  private readonly routes = new Map<string, Entry>();
  private readonly pages = new Map<string, HTMLElement>();
  private active: string | null = null;
  private readonly navListeners = new Set<(id: string) => void>();

  constructor(opts: SettingsRouterOptions) {
    this.landingId = opts.landingId;
    this.hidePageHeader = opts.hidePageHeader ?? false;
    const horizontal = opts.orientation === "horizontal";

    this.root = document.createElement("div");
    this.root.className = horizontal ? "settings-shell settings-shell-h" : "settings-shell";

    this.nav = document.createElement("nav");
    this.nav.className = "settings-nav";
    this.nav.setAttribute("role", "tablist");
    this.nav.setAttribute("aria-orientation", horizontal ? "horizontal" : "vertical");
    this.nav.setAttribute("aria-label", copyText("router.ctor.ariaLabel"));
    this.nav.addEventListener("keydown", (ev) => this.onNavKeydown(ev));
    this.root.appendChild(this.nav);

    this.content = document.createElement("div");
    this.content.className = "settings-content";
    if (!horizontal) {
      const sel = document.createElement("select");
      sel.className = "settings-nav-select";
      sel.setAttribute("aria-label", copyText("router.ctor.ariaLabel"));
      sel.addEventListener("change", () => this.navigate(sel.value));
      this.content.appendChild(sel);
      this.narrowSelect = sel;
    } else {
      this.narrowSelect = null;
    }
    this.root.appendChild(this.content);
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 内容区（各页的滚动容器）。 */
  get contentElement(): HTMLElement {
    return this.content;
  }

  get activeId(): string | null {
    return this.active;
  }

  get routeIds(): string[] {
    return [...this.routes.keys()];
  }

  has(id: string): boolean {
    return this.routes.has(id);
  }

  /** 导航项在屏幕上的顺序（＝ DOM 顺序；子项是后注册、插在父项后面的）。 */
  private navOrderIds(): string[] {
    const out: string[] = [];
    for (const el of this.nav.querySelectorAll<HTMLElement>(".settings-nav-item")) {
      const id = el.dataset.routeId ?? "";
      if (this.routes.has(id)) out.push(id);
    }
    return out;
  }

  addRoute(route: SettingsRoute): void {
    if (this.routes.has(route.id)) {
      throw new Error(`bug: SettingsRouter registered route id "${route.id}" twice`);
    }
    const tabId = `settings-tab-${route.id}`;
    const panelId = `settings-panel-${route.id}`;

    const navButton = document.createElement("button");
    navButton.type = "button";
    navButton.id = tabId;
    navButton.dataset.routeId = route.id;
    navButton.className = route.parentId ? "settings-nav-item settings-nav-item-child" : "settings-nav-item";
    if (route.gapBefore) navButton.classList.add("settings-nav-item-gap");
    navButton.setAttribute("aria-controls", panelId);
    navButton.setAttribute("role", "tab");
    navButton.addEventListener("click", () => this.navigate(route.id));
    if (route.icon) navButton.appendChild(icon(route.icon));
    const label = document.createElement("span");
    label.className = "settings-nav-label";
    label.textContent = route.title;
    navButton.appendChild(label);

    const anchor = route.parentId ? this.lastNavNodeUnder(route.parentId) : null;
    if (anchor) anchor.after(navButton);
    else this.nav.appendChild(navButton);

    const page = document.createElement("section");
    page.id = panelId;
    page.className = "settings-page";
    page.setAttribute("role", "tabpanel");
    page.setAttribute("aria-labelledby", tabId);
    page.dataset.routeId = route.id;
    if (!this.hidePageHeader && !route.hideHead) {
      const head = document.createElement("div");
      head.className = "settings-page-head";
      const title = document.createElement("h2");
      title.className = "settings-page-title";
      title.textContent = route.title;
      head.appendChild(title);
      const sp = document.createElement("span");
      sp.className = "settings-page-head-sp";
      head.appendChild(sp);
      for (const a of route.headActions ?? []) head.appendChild(a);
      page.appendChild(head);
      if (route.sub) {
        const sub = document.createElement("div");
        sub.className = "settings-page-sub";
        sub.textContent = route.sub;
        page.appendChild(sub);
      }
    }
    page.appendChild(route.element);
    this.pages.set(route.id, page);
    this.content.appendChild(page);

    let option: HTMLOptionElement | null = null;
    if (this.narrowSelect) {
      option = document.createElement("option");
      option.value = route.id;
      option.textContent = route.parentId ? `· ${route.title}` : route.title;
      const at = route.parentId ? this.lastOptionUnder(route.parentId) : null;
      if (at) at.after(option);
      else this.narrowSelect.appendChild(option);
    }

    this.routes.set(route.id, { route, navButton, label, badge: null, dot: null, option });

    if (this.active === null || route.id === this.landingId) this.navigate(route.id);
    else this.applyVisibility();
  }

  /** 改一页的名字（机器改名）：导航项、页头、窄窗下拉同拍改。 */
  setTitle(id: string, title: string): void {
    const entry = this.routes.get(id);
    if (!entry || entry.route.title === title) return;
    entry.route = { ...entry.route, title };
    entry.label.textContent = title;
    if (entry.option) entry.option.textContent = entry.route.parentId ? `· ${title}` : title;
    const head = this.pages.get(id)?.querySelector<HTMLElement>(":scope > .settings-page-head .settings-page-title");
    if (head) head.textContent = title;
  }

  /** 导航项右侧的琥珀角标：0 不画，超过 99 写 99+。 */
  setBadge(id: string, n: number): void {
    const entry = this.routes.get(id);
    if (!entry) return;
    const next = countBadge(n, "warn");
    next?.classList.add("settings-nav-badge");
    if (entry.badge && next) entry.badge.replaceWith(next);
    else if (entry.badge) entry.badge.remove();
    else if (next) entry.navButton.appendChild(next);
    entry.badge = next;
  }

  /** 子项前的状态点；悬停与读屏名第一行是状态词。 */
  setNavDot(id: string, state: MachineDotState, label: string): void {
    const entry = this.routes.get(id);
    if (!entry) return;
    if (!entry.dot) {
      entry.dot = statusDot(state, label, "compact");
      entry.navButton.insertBefore(entry.dot, entry.label);
    } else {
      setDot(entry.dot, state, label);
    }
    entry.navButton.title = label;
  }

  pageContentOf(id: string): HTMLElement | null {
    return this.routes.get(id)?.route.element ?? null;
  }

  /** 整页（页头 ＋ 内容）。 */
  pageOf(id: string): HTMLElement | null {
    return this.pages.get(id) ?? null;
  }

  navButtonOf(id: string): HTMLButtonElement | null {
    return this.routes.get(id)?.navButton ?? null;
  }

  private lastNavNodeUnder(parentId: string): HTMLElement | null {
    const parent = this.routes.get(parentId);
    if (!parent) return null;
    let node: HTMLElement = parent.navButton;
    for (const [, e] of this.routes) if (e.route.parentId === parentId) node = e.navButton;
    return node;
  }

  private lastOptionUnder(parentId: string): HTMLOptionElement | null {
    const parent = this.routes.get(parentId);
    if (!parent) return null;
    let opt = parent.option;
    for (const [, e] of this.routes) if (e.route.parentId === parentId && e.option) opt = e.option;
    return opt;
  }

  /** 注销一页（机器删掉）；正看着它就切到父页（没有父就第一页）。 */
  removeRoute(id: string): void {
    const entry = this.routes.get(id);
    if (!entry) return;
    const wasActive = this.active === id;
    const fallback = entry.route.parentId ?? this.routeIds.find((x) => x !== id);
    entry.navButton.remove();
    entry.option?.remove();
    this.pages.get(id)?.remove();
    this.pages.delete(id);
    this.routes.delete(id);
    if (wasActive) {
      this.active = null;
      if (fallback) this.navigate(fallback);
    } else {
      this.applyVisibility();
    }
  }

  onNavigate(fn: (id: string) => void): () => void {
    this.navListeners.add(fn);
    return () => this.navListeners.delete(fn);
  }

  /** 切到某页。没注册 ⇒ 不动；同页不重复通知。 */
  navigate(id: string): void {
    if (!this.routes.has(id)) return;
    if (this.active === id) return;
    this.active = id;
    this.applyVisibility();
    for (const fn of [...this.navListeners]) {
      try {
        fn(id);
      } catch (e) {
        console.warn("[settings-router] onNavigate 订阅者抛异常：", e);
      }
    }
  }

  private onNavKeydown(ev: KeyboardEvent): void {
    const ids = this.navOrderIds();
    if (ids.length === 0) return;
    const cur = this.active === null ? 0 : ids.indexOf(this.active);
    let next: number;
    switch (ev.key) {
      case "ArrowDown":
      case "ArrowRight":
        next = (cur + 1) % ids.length;
        break;
      case "ArrowUp":
      case "ArrowLeft":
        next = (cur - 1 + ids.length) % ids.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = ids.length - 1;
        break;
      default:
        return;
    }
    ev.preventDefault();
    this.navigate(ids[next]!);
    this.routes.get(ids[next]!)!.navButton.focus();
  }

  private applyVisibility(): void {
    for (const [id, { navButton }] of this.routes) {
      const on = id === this.active;
      this.pages.get(id)!.hidden = !on;
      navButton.classList.toggle("settings-nav-item-active", on);
      navButton.setAttribute("aria-selected", on ? "true" : "false");
      navButton.tabIndex = on ? 0 : -1;
    }
    if (this.narrowSelect && this.active) this.narrowSelect.value = this.active;
  }
}
