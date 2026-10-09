/**
 * 固定高度的加载态：数据回来前若只渲染一行字、回来变成一整张表，块长高、下面的东西全往下掉，那几秒点什么都点错地方
 * ⇒ 加载态与渲染态占同样高。骨架高度与数据回来后钉在容器上的高度下限必须是同一个值 ⇒ 都从 `SKELETON_PX` 取，
 * 判据对着这张表与屏幕上真出现的那几块做两向相等（`settings-skeleton.vitest.ts`）。
 * `min-height` 是下限：内容比它高时仍会长高；「加载期零位移」jsdom 里量不了，判据不声称它。
 */

/** 每一块骨架占多高（像素），键就是那一块的身份（`data-skeleton`）。数是按典型行数 × 行高估的，不是量的。 */
export const SKELETON_PX = {
  /** 数据位置：4~5 张卡片。 */
  "data-places": 280,
  /** 后端开关：每台机一行，按「本机 ＋ 2 台」估。 */
  backend: 168,
} as const;

export type SkeletonKey = keyof typeof SKELETON_PX;

/**
 * 造一块骨架：不是空 div（空的和「加载失败」在屏幕上分不开）；`aria-busy` 给读屏器一句「正在载入」。
 * `className` 必须传盘上已有的类（默认 `settings-hint`）：没有规则的新类名会撞 `css-ledger`，新钩子走 `data-*`。
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

/** 把容器钉在与骨架同一个高度下限上。要在第一次渲染骨架时就钉，等数据回来再钉那一跳已经发生过了。 */
export function holdSkeletonHeight(container: HTMLElement, key: SkeletonKey): void {
  container.dataset.skeletonHold = key;
  container.style.minHeight = skeletonHeight(key);
}

/** 唯一一处把数字变成 CSS 长度的地方。 */
export function skeletonHeight(key: SkeletonKey): string {
  return `${SKELETON_PX[key]}px`;
}
