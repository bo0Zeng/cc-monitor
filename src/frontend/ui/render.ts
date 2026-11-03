import { fillImage } from "./cards/attach-strip";
import { mountDoc, type HlPull } from "./md-view";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Machine } from "./ipc/origin";
import type { Doc } from "./generated/Doc";
import type { MdAs } from "./generated/MdAs";
import type { PlanRef } from "./generated/PlanRef";

/**
 * 正文排好之后补的那一样占位：图（`data-image-pending`，进视口才去取）。
 * Markdown 一律是后端解析好的树（`md-view.ts` 照树排），公式也是后端排好的画单（`md-view.ts` 当场画）；界面这一侧没有解析器、没有排版器。
 */

/**
 * 一整棵树排出来（短正文：来话 · 计划卡 · 思考 · 计划格）：各段一次排完。`lazy` ⇒ 图留着占位，卡进视口时 `enhanceCard` 补；否则当场补。
 * `pull` 给了 ⇒ 代码块照正文卡那样问后端要高亮（按内容，不认是哪一格）。
 */
export function mountWhole(doc: Doc, lazy = false, pull?: HlPull): HTMLElement {
  const view = mountDoc(doc, pull);
  while (view.buildNext());
  if (!lazy) enhancePending(view.root);
  return view.root;
}

/** 解析一段原文的期限：解析是纯计算（长正文几毫秒），过了就照原文的字。 */
const DOC_BUDGET_MS = 10_000;

/**
 * 界面手上有原文的那几处（思考展开 · 工具结果切到 Markdown · 计划格正文）问后端要树（`md-doc`）：`as` 选读法，
 * `refs` 是原文里提到的格（pb 给的 UTF-16 起止，只计划格正文带），解成引用节点。
 */
export function fetchDoc(origin: Machine, text: string, as: MdAs, refs: readonly PlanRef[] = []): Promise<Doc> {
  return callTyped(origin, "md-doc", { text, as, refs: [...refs] }, budgetWithin(DOC_BUDGET_MS));
}

/** 拉一段高亮的期限：算的是一段（≤ 两万字）的代码，过了就停在纯字。 */
const HL_BUDGET_MS = 30_000;

/** 问那台后端拉代码块的高亮（`prose-hl`，按内容算、按内容缓存：哪一格的代码都是这一条）。 */
export function hlPullOf(origin: Machine): HlPull {
  return (codes) => callTyped(origin, "prose-hl", { codes: [...codes] }, budgetWithin(HL_BUDGET_MS));
}

/** 纯文本（用户消息保守模式）：转义 + 保留换行 */
export function renderPlainText(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML.replace(/\n/g, "<br>");
}

/** 排好之后还要补的占位：等着取的图。 */
const PENDING = "[data-image-pending]";

/**
 * 补卡片里惰路留下的占位（[`enhancePending`]）。幂等：标 `data-enhanced`；没有占位时一次 querySelector 就返回。
 */
export function enhanceCard(el: HTMLElement): void {
  if (el.dataset.enhanced === "1") return;
  el.dataset.enhanced = "1";
  enhancePending(el);
}

/**
 * 补 `el` 里此刻还在的占位：图（`[data-image-pending]`）这时才去取（`cards/attach-strip.ts::fillImage`）。
 * 不标记：同一块后来又长出占位（长正文点「显示全部」补的那几段）时再调一次，只补新的。
 */
export function enhancePending(el: HTMLElement): void {
  for (const block of el.querySelectorAll<HTMLElement>(PENDING)) fillImage(block);
}

/**
 * 每个滚动容器一个 IntersectionObserver（root = 那个容器）：卡片进可视区（± 300px，快滚到时就取、不让图「弹」出来）调 enhanceCard，然后 unobserve。
 * root 必填：主窗里各 tab 的 `.stream` 叠在同一块区域（后台 tab 是 `visibility:hidden`，仍有几何），拿视口当 root 分不清卡属于哪一条流。
 *
 * - 同一个 root 复用一个 IO（`WeakMap`）；容器销毁时调 {@link releaseEnhanceRoot}（`TabStreamView.disposeTab` · `SessionViewer`）。
 * - 没有 IO 的环境（jsdom）⇒ 立即 enhance。
 */
const enhanceObservers = new WeakMap<HTMLElement, IntersectionObserver>();

/**
 * 让一个卡片接受 lazy enhance 调度（`root` = 它所在的滚动容器）。实时 tab 与查看器在 lazy 渲染期间挂卡片时调。
 * 里面没有等着补的占位（惰路只在建卡那一刻留占位）⇒ 不交给 IO：大多数卡没有图，个个挂着 ＝ 滚动时每一帧
 * 都要替它们算一遍交叉（300 轮的子运行 2495 个目标 → 299 个）。
 */
export function observeForEnhance(el: HTMLElement, root: HTMLElement): void {
  if (el.dataset.enhanced === "1" || el.querySelector(PENDING) === null) return;
  if (typeof IntersectionObserver === "undefined") {
    enhanceCard(el);
    return;
  }
  let io = enhanceObservers.get(root);
  if (!io) {
    io = new IntersectionObserver(
      (entries, obs) => {
        for (const e of entries) {
          if (e.isIntersecting && e.target instanceof HTMLElement) {
            enhanceCard(e.target);
            obs.unobserve(e.target);
          }
        }
      },
      { root, rootMargin: "300px" },
    );
    enhanceObservers.set(root, io);
  }
  io.observe(el);
}

/** 卡摘出了流（骨架交回占位）：不再等它进视口。 */
export function forgetEnhance(el: HTMLElement, root: HTMLElement): void {
  enhanceObservers.get(root)?.unobserve(el);
}

/** 滚动容器销毁：断开它那一个 IO（还没进过视口的卡不再补，容器都没了）。 */
export function releaseEnhanceRoot(root: HTMLElement): void {
  const io = enhanceObservers.get(root);
  if (!io) return;
  io.disconnect();
  enhanceObservers.delete(root);
}
