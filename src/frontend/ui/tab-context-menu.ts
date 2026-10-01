/**
 * 〔拆 `tabs.ts` ⑤〕**tab 右键菜单这个控件**（开 / 关 / 按 id 就地换项 / 追加 / 二级 flyout）。
 *
 * 只管「菜单长什么样、怎么开关」，**不管里面放哪几项** —— 那是 `tab-menu.ts` 的事。
 * 零 IPC。原住 `tabs.ts` 文件尾，逐字搬出；唯一的形状改动是给代次加了个只读出口 `menuGeneration()`。
 */
/**
 * issue #10：极简一次性上下文菜单（Tab 右键用）。挂 document.body 作 fixed 浮层，
 * 点任意项 / 点外部 / Esc 即关。一次只允许一个（开新的前先关旧的）。
 *
 * B14-F51：升级为 action 注册表 lite——项带可选 `id`/`enabled`,并可对**已打开**菜单按 id
 * `update`/`remove`(承载异步就绪项,如 attach 的 tmux 反查回来才可点）。
 */
export interface TabMenuItem {
  id?: string;
  label: string;
  enabled?: boolean; // 缺省 true;false = 禁用占位
  danger?: boolean; // F79：破坏性项（杀会话）红色样式
  title?: string; // A5：hover tooltip（如 compact 顺序说明）
  onClick?: () => void; // 有 submenu 时不需要——点击/悬停展开子菜单而非执行动作
  /** F09：二级 flyout（"动作 × 修饰"，R4 悬停+点击都可触发展开）。
   *  有 submenu 时 `onClick` 被忽略——这一级只负责展开，不执行动作；真正的动作在叶子项上。 */
  submenu?: TabMenuItem[];
  /** F09 Phase D 审计（UX，建议）：纯展示性分隔线——不可点、不响应悬停，只用来在 flyout 里把
   *  "跟随默认账号"的顶层选项和"换账号"的具名列表视觉分组，降低扫描成本（不增加点击次数）。
   *  为真时其余字段（onClick/submenu/danger 等）都被忽略。 */
  divider?: boolean;
}
let activeTabMenu: HTMLElement | null = null;
const activeTabMenuItems = new Map<string, HTMLElement>();
/** F51：菜单代次令牌——每次开/关菜单自增。在飞的异步就绪(attach 反查)回来时比对代次,
 * 只作用于发起它的那一代菜单;换/关菜单后旧查询整体 no-op(防 R-1 跨 tab 串味错配)。 */
let tabMenuGeneration = 0;
/**
 * 这一代菜单的代次（`tabMenuGeneration` 的只读出口）。菜单控件搬出 `tabs.ts` 之后，
 * 在飞的异步就绪（attach 反查 / 账号列表）要在**另一个模块**里比对代次 ⇒ 给一个函数，
 * 不让别处直接碰这个 `let`（写者只有本文件的开 / 关菜单两处）。
 */
export function menuGeneration(): number {
  return tabMenuGeneration;
}
/** F09 Phase D 审计（后端架构，重要）：本代菜单存活期间所有 submenu 的展开/收起定时器——
 *  `closeTabContextMenu` 统一清空，防止用户点外部/Esc 关掉整个菜单后，某个 pending 定时器
 *  仍在 150-250ms 后对已从文档树摘除的 wrap 执行 `classList` 操作（功能上是良性 no-op，
 *  但属未清理的悬空定时器）。 */
let pendingMenuTimers: number[] = [];

/** F09：带 submenu 的项渲染成 `.tab-context-menu-item-wrap`（按钮 + 侧边 flyout 面板），
 *  悬停延迟 150ms 展开、点击也可切换展开（R4）；离开延迟 250ms 收起（不对称是有意的——
 *  Phase D 审计（UX，重要）指出零延迟收起会命中经典"safe triangle"问题：账号数≥2 时
 *  Resume flyout 是一列都带 submenu 的项，用户从某账号项斜向移动鼠标去够它自己 submenu
 *  里的选项，路径中途经过下一个账号项就会被判定"已离开"、submenu 瞬间关闭。给收起也一个
 *  可取消的宽限期，鼠标真落进目标区域后被新一轮 `mouseenter` 清掉，不会误关）。
 *  叶子项（无 submenu）行为不变——仍是裸 `<button class="tab-context-menu-item">`，
 *  点击执行 `onClick` 并关闭整个菜单。 */
function makeTabMenuButton(it: TabMenuItem): HTMLElement {
  if (it.divider) {
    const sep = document.createElement("div");
    sep.className = "tab-context-menu-divider";
    return sep;
  }
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "tab-context-menu-item";
  if (it.danger) btn.classList.add("is-danger");
  btn.textContent = it.label;
  if (it.title) btn.title = it.title;
  const enabled = it.enabled !== false;
  btn.disabled = !enabled;

  if (it.submenu && it.submenu.length > 0) {
    btn.classList.add("has-submenu");
    const wrap = document.createElement("div");
    wrap.className = "tab-context-menu-item-wrap";
    wrap.appendChild(btn);
    const flyout = document.createElement("div");
    flyout.className = "tab-context-menu tab-context-submenu";
    for (const sub of it.submenu) flyout.appendChild(makeTabMenuButton(sub));
    wrap.appendChild(flyout);
    if (enabled) {
      let openTimer: number | null = null;
      let closeTimer: number | null = null;
      const clearPending = (t: number | null): void => {
        if (t == null) return;
        window.clearTimeout(t);
        pendingMenuTimers = pendingMenuTimers.filter((id) => id !== t);
      };
      const open = (): void => {
        flipSubmenuIfOverflowing(wrap, flyout);
        wrap.classList.add("is-open");
      };
      wrap.addEventListener("mouseenter", () => {
        clearPending(openTimer);
        clearPending(closeTimer);
        closeTimer = null;
        openTimer = window.setTimeout(open, 150);
        pendingMenuTimers.push(openTimer);
      });
      wrap.addEventListener("mouseleave", () => {
        clearPending(openTimer);
        openTimer = null;
        closeTimer = window.setTimeout(() => wrap.classList.remove("is-open"), 250);
        pendingMenuTimers.push(closeTimer);
      });
      btn.addEventListener("click", (e) => {
        e.stopPropagation(); // 别冒泡到 onDocPointerForMenu 把整个菜单关掉
        if (wrap.classList.contains("is-open")) {
          wrap.classList.remove("is-open");
        } else {
          open();
        }
      });
    }
    return wrap;
  }

  if (enabled) {
    btn.addEventListener("click", () => {
      closeTabContextMenu();
      it.onClick?.();
    });
  }
  return btn;
}

/** F09 Phase D 审计（UX，重要）：级联 flyout 没有视口边缘碰撞检测——tab-bar 可拖到 340px 宽
 *  （`main.ts::clampW` 的硬上限），三级级联（一级菜单+Resume flyout+账号自身 submenu）从
 *  x≈340px 起算需要窗口宽度 ≳790px 才保证不溢出右边界，窄窗口/宽 tab-bar 这两个正常操作
 *  组合起来就会让最深一级 flyout 部分或整体跑出屏幕、变成死菜单。每次展开前（不是持续轮询）
 *  实测一次 `getBoundingClientRect()`，右侧放不下就加 `.flip-left`（CSS 改成向左展开）。 */
function flipSubmenuIfOverflowing(wrap: HTMLElement, flyout: HTMLElement): void {
  flyout.classList.remove("flip-left"); // 先复位，按当前真实位置重新判断（tab-bar 宽度可变）
  const wrapRect = wrap.getBoundingClientRect();
  const flyoutWidth = flyout.getBoundingClientRect().width || 150; // 未展开时宽度可能是 0，给合理估计
  if (wrapRect.right + flyoutWidth > window.innerWidth) {
    flyout.classList.add("flip-left");
  }
}

export function showTabContextMenu(x: number, y: number, items: TabMenuItem[]): void {
  closeTabContextMenu();
  const menu = document.createElement("div");
  menu.className = "tab-context-menu";
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
  for (const it of items) {
    const btn = makeTabMenuButton(it);
    if (it.id) activeTabMenuItems.set(it.id, btn);
    menu.appendChild(btn);
  }
  document.body.appendChild(menu);
  activeTabMenu = menu;
  tabMenuGeneration++; // 新一代菜单 → 让上一代在飞的异步就绪回调失效
  // 下一拍再挂关闭监听，避免本次右键触发的事件立刻把菜单关掉
  window.setTimeout(() => {
    window.addEventListener("pointerdown", onDocPointerForMenu, true);
    window.addEventListener("keydown", onKeyForMenu, true);
  }, 0);
}

/** F51：把已打开菜单里某 id 项替换为新项(异步就绪→可点);菜单已关或无此 id 则 no-op。
 *  F09 Phase D 审计（UX，阻塞）：若旧项当前正展开着 flyout（用户已 hover/点开），替换后的新
 *  元素默认是关闭态——鼠标没动但 flyout 会无预警"啪"地收起（浏览器不会因 DOM 被替换重新
 *  派发 mouseenter）。直接命中 R4"悬停+点击都可触发"这条契约，且越熟练的用户越容易踩中
 *  （账号数据还没到就已经手快点开了）。替换前记下展开态，替换后原样带回去。 */
export function updateTabContextMenuItem(id: string, item: TabMenuItem): void {
  const old = activeTabMenuItems.get(id);
  if (!old || !activeTabMenu) return;
  const wasOpen = old.classList.contains("is-open");
  const btn = makeTabMenuButton(item);
  if (wasOpen) btn.classList.add("is-open");
  activeTabMenuItems.set(item.id ?? id, btn);
  old.replaceWith(btn);
}

/** F51：移除已打开菜单里某 id 项(异步查无匹配);无此 id 则 no-op。 */
export function removeTabContextMenuItem(id: string): void {
  const old = activeTabMenuItems.get(id);
  if (!old) return;
  old.remove();
  activeTabMenuItems.delete(id);
}

/** A4/A5：往已打开菜单**追加**一项(异步就绪,如账号列表 fetch 回来)。菜单已关则 no-op。 */
export function appendTabContextMenuItem(item: TabMenuItem): void {
  if (!activeTabMenu) return;
  const btn = makeTabMenuButton(item);
  if (item.id) activeTabMenuItems.set(item.id, btn);
  activeTabMenu.appendChild(btn);
}

function closeTabContextMenu(): void {
  if (!activeTabMenu) return;
  activeTabMenu.remove();
  activeTabMenu = null;
  activeTabMenuItems.clear();
  // F09 Phase D 审计（后端架构，重要）：清掉本代菜单存活期间所有 submenu 的展开/收起定时器，
  // 防止用户点外部/Esc 关掉整个菜单后，某个 pending 定时器仍在之后对已摘除的 wrap 操作。
  for (const t of pendingMenuTimers) window.clearTimeout(t);
  pendingMenuTimers = [];
  tabMenuGeneration++; // 关菜单也让在飞的异步就绪回调失效(不改别的菜单)
  window.removeEventListener("pointerdown", onDocPointerForMenu, true);
  window.removeEventListener("keydown", onKeyForMenu, true);
}
function onDocPointerForMenu(e: PointerEvent): void {
  if (activeTabMenu && !activeTabMenu.contains(e.target as Node)) {
    closeTabContextMenu();
  }
}
function onKeyForMenu(e: KeyboardEvent): void {
  if (e.key === "Escape") closeTabContextMenu();
}
