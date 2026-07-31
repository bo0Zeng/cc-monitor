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
}

export interface SettingsRouterOptions {
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
  private readonly routes = new Map<
    string,
    { route: SettingsRoute; navButton: HTMLButtonElement }
  >();
  /** id → 该页的外壳（页头 + 内容）。显隐设在外壳上。 */
  private readonly pages = new Map<string, HTMLElement>();
  private active: string | null = null;

  constructor(opts: SettingsRouterOptions) {
    this.landingId = opts.landingId;

    this.root = document.createElement("div");
    this.root.className = "settings-shell";

    this.nav = document.createElement("nav");
    this.nav.className = "settings-nav";
    // 导航是一组互斥的页面选择器 —— 用 tablist 语义，读屏器才会念「第 2 项，共 4 项」。
    this.nav.setAttribute("role", "tablist");
    this.nav.setAttribute("aria-orientation", "vertical");
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

  /** 已注册的页 id（按注册顺序）——给测试和调用方对账用。 */
  get routeIds(): string[] {
    return [...this.routes.keys()];
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
    navButton.className = "settings-nav-item";
    navButton.setAttribute("aria-controls", panelId);
    navButton.textContent = route.title;
    navButton.setAttribute("role", "tab");
    navButton.addEventListener("click", () => this.navigate(route.id));
    this.nav.appendChild(navButton);

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
    page.appendChild(head);
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

  /** 切到某页。id 未注册 = no-op（不抛：切页是 UI 动作，不该因为拼错就炸掉面板）。 */
  navigate(id: string): void {
    if (!this.routes.has(id)) return;
    this.active = id;
    this.applyVisibility();
  }

  /** 方向键在导航组内移动（tablist 惯例：组内用方向键，Tab 跳出整组）。 */
  private onNavKeydown(ev: KeyboardEvent): void {
    const ids = this.routeIds;
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
