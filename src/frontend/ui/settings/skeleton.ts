/**
 * （第一刀 · 步 1）：**固定高度的加载态**。
 *
 * # 它治的不是「慢」，是「界面自己在变」
 *
 * 那两张相隔 3 秒的真机截图逐格对比出来的结论是：用户说的「卡住」其实是
 * **整页重排** —— 每一块在数据回来前渲染的是一行字（「扫描中…」「加载中…」），
 * 数据回来变成一整张表 ⇒ 块长高 ⇒ 它下面的东西**全部往下掉**。
 * 那 3 秒里点任何东西都会点在错的地方。
 *
 * ⇒ 修法不是「加载快一点」，是**让加载态与渲染态占同样高**。
 *
 * # 为什么高度要登记在这里，而不是各块自己拍一个数
 *
 * 「一个事实一个住址」：骨架高度与**数据回来之后钉在容器上的那个高度下限**必须是
 * **同一个值**。分开写两处，下一次有人调其中一处，重排就悄悄回来了 —— 而它不报错。
 * ⇒ 两处都从 `SKELETON_PX` 取，判据对着这张表与**屏幕上真出现的那几块**做两向集合相等
 *   （`tests/frontend/ui/settings/settings-skeleton.vitest.ts`）。
 *
 * ⚠ **诚实边界**：`min-height` 是**下限**，不是「等高」。内容比它高的时候仍然会长高
 * —— 本模块买的是「**数据回来时不会从一行字跳成一整屏**」，不是「像素级零位移」。
 * 那条「加载期高度变化 = 0」要真机上挂 `ResizeObserver` 才量得到，
 * **jsdom 里量不了**（没有排版引擎），判据不声称它。
 */

/**
 * 每一块骨架占多高（像素）。**键就是那一块的身份**，`data-skeleton` 上原样写出去。
 *
 * 数怎么来的：按各块渲染后的典型行数 × 行高估的，**是拍的，不是量的**
 * —— 没有真机排版读数支持（同那个 `lingerMs` 的处境，如实标着）。
 */
export const SKELETON_PX = {
  /** 数据位置（原「数据存储」）：4~5 张卡片。 */
  "data-places": 280,
  /** backend 开关：每台机一行，按「本机 + 2 台」估。 */
  backend: 168,
} as const;

export type SkeletonKey = keyof typeof SKELETON_PX;

/**
 * 造一块骨架。**它不是空 div** —— 空的和「加载失败」在屏幕上分不开
 * （那一格的病根正是「没有第三态」：兜底态被当成了加载态）。
 *
 * `aria-busy` 是给读屏器的：屏幕上是几条灰杠，读屏器需要一句「正在载入」。
 *
 * ⚠ `className` **必须传盘上已有的类**（默认 `settings-hint`）。
 * 新造一个没有 CSS 规则的类名会当场撞 `css-ledger.vitest.ts` 的两条棘轮
 * （「悬空的类名引用只许变少」与「每个 CSS 类名都说得出谁在用它」）——
 * 而给的正解就是：要新钩子用 `data-*`，不要新类名。本模块照办
 * （`data-skeleton` / `data-skeleton-hold`）。
 */
export function makeSkeleton(
  key: SkeletonKey,
  label: string,
  className = "settings-hint",
): HTMLElement {
  const el = document.createElement("div");
  el.className = className;
  el.dataset.skeleton = key;
  el.setAttribute("aria-busy", "true");
  el.style.minHeight = skeletonHeight(key);
  el.textContent = label;
  return el;
}

/**
 * 把容器钉在与骨架**同一个**高度下限上。
 *
 * ⚠ 顺序很要紧：要在**第一次渲染骨架的时候**就钉，不是等数据回来才钉
 * —— 等回来再钉，那一跳已经发生过了。
 */
export function holdSkeletonHeight(container: HTMLElement, key: SkeletonKey): void {
  container.dataset.skeletonHold = key;
  container.style.minHeight = skeletonHeight(key);
}

/** 唯一一处把数字变成 CSS 长度的地方。 */
export function skeletonHeight(key: SkeletonKey): string {
  return `${SKELETON_PX[key]}px`;
}
