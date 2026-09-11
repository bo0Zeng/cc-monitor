/**
 * K-R45：**在一条消息流里按 uuid 找到那张卡、展开挡着它的折叠、滚过去并闪一下。**
 *
 * # 为什么这一段单独住在这里
 *
 * 它今天有**两个**调用方，而且是本轮才有第二个的：
 * - `views/session-viewer.ts::scrollToMessage`（历史查看器；搜索命中与「我说过的 N 句」都走它）
 * - `tabs.ts` 的实时窗口（`KR45D2` 本轮接上的那条路）
 *
 * 件 `§5.2` 的 B 段现打核过：这一段**两条路真能共用**，底下已经是同一份了 ——
 * 两条路的卡由**同一个** `renderStreamRecord` 建，`data-uuid` 由**唯一一份**
 * `render-stream-record.ts::markCardUuid` 写。所以「照抄一份到 tabs.ts」买到的
 * 只会是「两处要一起改」，而那正是这个仓一整天在治的病。
 *
 * ⚠ **不能共用的那两段刻意留在各自宿主里**（读数与分母在件的 `§5.2`）：
 * - 「目标还没渲染出来 ⇒ 先把它渲出来」—— 查看器有 `uuidToIdx` + `UnrenderedRanges`，
 *   实时窗口只有一个**只能从尾部取**的 `TailWindow`，两者结构不同。
 * - 「找不到卡时怎么办」—— 查看器退到底部（既有行为），实时窗口**什么都不做**
 *   （它本来就贴在底部，再滚一次是无意义的动作）。
 *   ⇒ 所以本函数**找不到就返回 `null`，一个动作都不做**，由宿主决定兜底。
 */

/**
 * 在 `container` 里找 `uuid` 那张卡：找到就展开祖先折叠、滚到视口中间、闪一下高亮，
 * 并把那张卡返回；**找不到返回 `null` 且什么都不做**。
 *
 * 返回值就是调用方的落点读数 —— 「跳过去了没有」不许再自己查一遍 DOM 猜。
 */
export function revealCard(container: HTMLElement, uuid: string): HTMLElement | null {
  // CSS.escape 防 uuid 里有特殊字符破坏选择器
  const sel = `[data-uuid="${CSS.escape(uuid)}"]`;
  const el = container.querySelector<HTMLElement>(sel);
  if (!el) return null;
  // 展开所有折叠祖先，确保目标可见。注:ESC 回退段是 div.branch-fold-wrap
  // + .expanded 类(非 <details>)——此前只开 details,命中折叠段内的卡会被
  // 0fr 裁剪、flash 不可见(Batch13 D 审计发现的既有 bug)
  let p: HTMLElement | null = el.parentElement;
  while (p && p !== container) {
    if (p instanceof HTMLDetailsElement) p.open = true;
    if (p.classList.contains("branch-fold-wrap") && !p.classList.contains("expanded")) {
      p.classList.add("expanded");
      p.querySelector(".branch-fold-header")?.setAttribute("aria-expanded", "true");
    }
    p = p.parentElement;
  }
  el.scrollIntoView({ block: "center" });
  // Batch13-F38:首次落点基于 content-visibility 估值几何;双 rAF 后周边已
  // 材料化(真实尺寸),幂等重发一次让 block:center 落点精确
  requestAnimationFrame(() => requestAnimationFrame(() => el.scrollIntoView({ block: "center" })));
  el.classList.add("search-hit-flash");
  // 动画结束后移除 class（再次跳同一条还能重放）
  window.setTimeout(() => el.classList.remove("search-hit-flash"), 2200);
  return el;
}
