/**
 * S2（settings-ia）：设置窗的**左侧导航 + 页面路由**。
 *
 * 为什么要新建而不是复用：全仓没有 tab/nav 原语 —— `CollapsibleGroup` 是「一列里收起一段」，
 * 语义是折叠不是切页（它同一时刻**允许多个展开**，而路由的定义就是同一时刻只有一个）。
 *
 * # 刻意很小
 *
 * 只有「注册一页 / 切一页 / 现在在哪一页」。没有 `removeRoute`、没有嵌套导航项、
 * 没有懒加载 —— S3 真需要时再加。L1 那次「为消灭一个 dead_code 告警而造投机 API」
 * 的教训还在（那轮的正确答案是接上已有入口，不是新造一个）。
 *
 * # 不碰构造时机
 *
 * 页面内容是**已经建好的 `HTMLElement`**，路由器只管显隐。设置面板今天 14 个块全部在
 * `buildBody()` 里即时构造，改成懒加载会引入「某页第一次打开才炸」这类新失效模式，
 * 也会让 `panel.ts` 里那几个在 build 时赋值、`open()` 时使用的字段
 * （`remoteSection` / `dataSection`）行为漂移。
 */

import { makeInfoIcon } from "./info-icon";

export interface SettingsRoute {
  /** 稳定 id（切页、测试、后续持久化都用它）。 */
  id: string;
  /** 导航上显示的名字。 */
  title: string;
  /** 该页的内容（**已构造好**，路由器只管显隐）。 */
  element: HTMLElement;
  /** 可选：页标题旁 ⓘ 的 hover 文案。 */
  infoTooltip?: string;
  /**
   * S4b：可选的**父页 id**。有父的项在导航里缩进显示在父项之下。
   *
   * 用途只有一个：机器详情页挂在「机器」下面（主计划 §2.3 那张图里，机器是可展开的父项，
   * 每台机器是它的子项）。**不做任意层级** —— 只支持一层子项，够用且不必去想
   * 「孙子项怎么缩进 / 折叠状态存哪」这些本轮没有的问题。
   */
  parentId?: string;
}

export interface SettingsRouterOptions {
  /**
   * S4b-3b-2：导航方向。默认 `vertical`（设置窗左侧那条）。
   * `horizontal` 用于**页内分栏**（机器详情页的 连接/组件/账号/工具）。
   *
   * **复用而不是另造一个 tab 原语**：分栏要的「同一时刻只有一页可见 + aria + 方向键 +
   * 不重复注册」与左侧导航**逐条相同**，差的只是排列方向与是否显示页头。
   * 另造等于把这套逻辑抄第二遍，而它们会各自漂。
   */
  orientation?: "vertical" | "horizontal";
  /** S4b-3b-2：页内分栏不需要页头（标题已经在 tab 上了）。默认 false。 */
  hidePageHeader?: boolean;
  /**
   * 落地页 id。主计划 §2.3 指定为「机器」。
   *
   * **刻意不记忆上次停在哪一页**：既然计划指定了落地页，记忆就会让这个决定失效
   * ——第二次打开起用户看到的就不是设计的落地页了。
   */
  landingId: string;
}

export class SettingsRouter {
  private readonly root: HTMLElement;
  private readonly nav: HTMLElement;
  private readonly content: HTMLElement;
  private readonly landingId: string;
  private readonly hidePageHeader: boolean;
  private readonly routes = new Map<
    string,
    { route: SettingsRoute; navButton: HTMLButtonElement }
  >();
  /** id → 该页的外壳（页头 + 内容）。显隐设在外壳上。 */
  private readonly pages = new Map<string, HTMLElement>();
  private active: string | null = null;
  private readonly navListeners = new Set<(id: string) => void>();

  constructor(opts: SettingsRouterOptions) {
    this.landingId = opts.landingId;
    this.hidePageHeader = opts.hidePageHeader ?? false;
    const horizontal = opts.orientation === "horizontal";

    this.root = document.createElement("div");
    this.root.className = horizontal
      ? "settings-shell settings-shell-h"
      : "settings-shell";

    this.nav = document.createElement("nav");
    this.nav.className = "settings-nav";
    // 导航是一组互斥的页面选择器 —— 用 tablist 语义，读屏器才会念「第 2 项，共 4 项」。
    this.nav.setAttribute("role", "tablist");
    this.nav.setAttribute(
      "aria-orientation",
      horizontal ? "horizontal" : "vertical",
    );
    this.nav.setAttribute("aria-label", "设置分类");
    // ★ 方向键必须实现，不是锦上添花：下面给非当前项设了 `tabIndex = -1`
    // （tablist 的 roving tabindex 惯例），**不配方向键的话那些项就键盘完全不可达了**。
    // 二选一必须成对：要么两个都做，要么两个都别做。
    this.nav.addEventListener("keydown", (ev) => this.onNavKeydown(ev));
    this.root.appendChild(this.nav);

    this.content = document.createElement("div");
    this.content.className = "settings-content";
    this.root.appendChild(this.content);
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 当前页 id；一页都没注册时为 `null`。 */
  get activeId(): string | null {
    return this.active;
  }

  /** 已注册的页 id（按**注册**顺序）——给测试和调用方对账用。 */
  get routeIds(): string[] {
    return [...this.routes.keys()];
  }

  /**
   * 导航项在屏幕上的**视觉**顺序（= DOM 顺序）。
   *
   * **E61：方向键必须走这个，不是 `routeIds`。** 两者只在「有子项」时不同，而生产里
   * 恰恰有：`addRoute` 把子项 `anchor.after(...)` 插到父项之后（视觉序
   * 应用/机器/aya/nano/改动足迹），而机器页是 `RemoteSection` **异步 `refresh()` 之后**
   * 才注册的 ⇒ 注册序永远是 应用/机器/改动足迹/aya/nano。
   * ⇒ 焦点在「机器」上按 ↓ 会跳到「改动足迹」，`End` 落到最后一台机器而不是视觉最后一项。
   *
   * 键盘导航的语义就是「按你看见的顺序走」，所以判据只能是 DOM。
   */
  private navOrderIds(): string[] {
    const out: string[] = [];
    for (const el of this.nav.querySelectorAll<HTMLElement>(".settings-nav-item")) {
      const id = el.id.replace(/^settings-tab-/, "");
      if (this.routes.has(id)) out.push(id);
    }
    return out;
  }

  addRoute(route: SettingsRoute): void {
    if (this.routes.has(route.id)) {
      throw new Error(`SettingsRouter: 重复注册路由 id "${route.id}"`);
    }

    const tabId = `settings-tab-${route.id}`;
    const panelId = `settings-panel-${route.id}`;

    const navButton = document.createElement("button");
    navButton.type = "button";
    navButton.id = tabId;
    navButton.className = route.parentId
      ? "settings-nav-item settings-nav-item-child"
      : "settings-nav-item";
    navButton.setAttribute("aria-controls", panelId);
    navButton.textContent = route.title;
    navButton.setAttribute("role", "tab");
    navButton.addEventListener("click", () => this.navigate(route.id));
    // 子项紧跟在父项（及父项已有的子项）之后 —— 否则新机器会跑到导航最末尾，
    // 和它的父项「机器」隔着「改动足迹」。
    const anchor = route.parentId ? this.lastNavNodeUnder(route.parentId) : null;
    if (anchor) anchor.after(navButton);
    else this.nav.appendChild(navButton);

    // 页头：标题 + 可选 ⓘ。既给页面一个锚（导航项在左边，视线回不来时容易失焦），
    // 也给那些原本挂在分组上的 hover 文案一个新宿主 —— 分组没了，文案不该跟着消失。
    const page = document.createElement("section");
    page.id = panelId;
    page.className = "settings-page";
    page.setAttribute("role", "tabpanel");
    page.setAttribute("aria-labelledby", tabId);
    // 给页面挂上 id：测试与调试靠它认页，不靠文本内容（页头一加标题，认文本就散了）。
    page.dataset.routeId = route.id;
    const head = document.createElement("div");
    head.className = "settings-page-head";
    const title = document.createElement("h3");
    title.className = "settings-page-title";
    title.textContent = route.title;
    head.appendChild(title);
    if (route.infoTooltip) head.appendChild(makeInfoIcon(route.infoTooltip));
    // 页内分栏时标题已经在 tab 上了，再来一份页头是重复
    //（但 ⓘ 若有仍要保住 —— 那是内容不是装饰）。
    if (!this.hidePageHeader) page.appendChild(head);
    else if (route.infoTooltip) page.appendChild(head);
    page.appendChild(route.element);
    this.pages.set(route.id, page);
    this.content.appendChild(page);

    this.routes.set(route.id, { route, navButton });

    // 第一页注册完就得有东西可看。落地页若还没注册（注册顺序与导航顺序未必一致），
    // 先顶上第一页；等落地页真注册上来再切过去。
    if (this.active === null) {
      this.navigate(route.id);
    } else if (route.id === this.landingId) {
      this.navigate(route.id);
    } else {
      this.applyVisibility();
    }
  }

  /** S4b-2：某一页的内容容器（页头之下那块）。给宿主往里搬 DOM 用。 */
  pageContentOf(id: string): HTMLElement | null {
    return this.routes.get(id)?.route.element ?? null;
  }

  /** 父项本身、或它最后一个子项的导航节点 —— 新子项插在它后面。 */
  private lastNavNodeUnder(parentId: string): HTMLElement | null {
    const parent = this.routes.get(parentId);
    if (!parent) return null;
    let node: HTMLElement = parent.navButton;
    for (const [, e] of this.routes) {
      if (e.route.parentId === parentId) node = e.navButton;
    }
    return node;
  }

  /**
   * S4b：注销一页（机器被删 / 改名时）。
   *
   * **当前页被注销时会切走**，切到它的父页（没有父就切第一页）—— 否则用户会停在
   * 一个已经从 DOM 里摘掉的页面上，看到一片空白且不知道发生了什么。
   */
  removeRoute(id: string): void {
    const entry = this.routes.get(id);
    if (!entry) return;
    const wasActive = this.active === id;
    const fallback = entry.route.parentId ?? this.routeIds.find((x) => x !== id);
    entry.navButton.remove();
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

  /**
   * S4b-2：订阅「切到哪一页了」。返回退订函数。
   *
   * 为什么需要：切页可以从**两个**入口发生 —— 点导航项，或点机器列表里那一行。
   * 「切到某台机器页时要做的事」（把 per-machine 的几块分节挪过去、更新
   * `machine-context`）必须两条入口都覆盖，所以只能挂在路由器这一层。
   */
  onNavigate(fn: (id: string) => void): () => void {
    this.navListeners.add(fn);
    return () => this.navListeners.delete(fn);
  }

  /** 切到某页。id 未注册 = no-op（不抛：切页是 UI 动作，不该因为拼错就炸掉面板）。 */
  navigate(id: string): void {
    if (!this.routes.has(id)) return;
    if (this.active === id) return; // 同页不重复通知（订阅者可能做搬 DOM 这类有代价的事）
    this.active = id;
    this.applyVisibility();
    for (const fn of [...this.navListeners]) {
      try {
        fn(id);
      } catch (e) {
        // 一个订阅者抛异常不能让切页本身失败 —— 页面已经切了，只是某件附带的事没做成。
        console.warn("[settings-router] onNavigate 订阅者抛异常：", e);
      }
    }
  }

  /** 方向键在导航组内移动（tablist 惯例：组内用方向键，Tab 跳出整组）。 */
  private onNavKeydown(ev: KeyboardEvent): void {
    // E61：**视觉序**，不是注册序（见 `navOrderIds` 的头注）。
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
    // 只在真的处理了按键时才 preventDefault —— 否则会把 Tab/Esc 之类一并吞掉。
    ev.preventDefault();
    this.navigate(ids[next]!);
    // 焦点跟着走，否则「按了下键但焦点还在原来那个按钮上」，再按一次就跳回去了。
    this.routes.get(ids[next]!)!.navButton.focus();
  }

  private applyVisibility(): void {
    for (const [id, { navButton }] of this.routes) {
      const on = id === this.active;
      // `hidden` 而不是 `display:none` 内联：既是语义（读屏器会跳过），
      // 也让 CSS 那边不必为「谁可见」负责。
      this.pages.get(id)!.hidden = !on;
      navButton.classList.toggle("settings-nav-item-active", on);
      navButton.setAttribute("aria-selected", on ? "true" : "false");
      // 非当前页的导航项退出 Tab 序（tablist 惯例：左右键在组内走，Tab 跳出整组）。
      navButton.tabIndex = on ? 0 : -1;
    }
  }
}
