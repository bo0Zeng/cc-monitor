import "katex/dist/katex.min.css";
import { fillImage } from "./cards/attach-strip";
import { mountDoc, type HlPull } from "./md-view";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { Doc } from "./generated/Doc";
import type { MdAs } from "./generated/MdAs";
import type { PlanRef } from "./generated/PlanRef";

/**
 * 正文排好之后补的那两样占位：公式（`md-view` 出的 `data-math-pending`）· 图（`data-image-pending`）。
 * Markdown 一律是后端解析好的树（`md-view.ts` 照树排），界面这一侧没有解析器。
 *
 * KaTeX 有公式才载（`import("katex")`，只载一次）：大多数会话一个公式都没有，首屏的包里没有它的脚本；
 * 样式表照旧静态引（类名账 `tests/evidence/class-ledger.ts` 从这一行现读第三方类名）。
 */

/**
 * KaTeX 的选项：
 * - throwOnError: false → 错误 LaTeX 渲染成红色源码而不是抛异常
 */
const KATEX_OPTIONS = { throwOnError: false } as const;

type Katex = typeof import("katex").default;

/** 载 KaTeX 的那一次；载过之后同一个承诺。 */
const katexLoad: { once: Promise<Katex> | null } = { once: null };

function loadKatex(): Promise<Katex> {
  katexLoad.once ??= import("katex").then((m) => m.default);
  return katexLoad.once;
}

/** 在路上的几处公式（[`__mathSettledForTests`] 等它们）。 */
const mathInFlight = new Set<Promise<void>>();

/**
 * 一整棵树排出来（短正文：来话 · 计划卡 · 思考 · 计划格）：各段一次排完。`lazy` ⇒ 公式与图留着占位，卡进视口时 `enhanceCard` 补；否则当场补。
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
export function fetchDoc(origin: Origin, text: string, as: MdAs, refs: readonly PlanRef[] = []): Promise<Doc> {
  return callTyped(origin, "md-doc", { text, as, refs: [...refs] }, budgetWithin(DOC_BUDGET_MS));
}

/** 拉一段高亮的期限：算的是一段（≤ 两万字）的代码，过了就停在纯字。 */
const HL_BUDGET_MS = 30_000;

/** 问那台后端拉代码块的高亮（`prose-hl`，按内容算、按内容缓存：哪一格的代码都是这一条）。 */
export function hlPullOf(origin: Origin): HlPull {
  return (codes) => callTyped(origin, "prose-hl", { codes: [...codes] }, budgetWithin(HL_BUDGET_MS));
}

/** 纯文本（用户消息保守模式）：转义 + 保留换行 */
export function renderPlainText(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML.replace(/\n/g, "<br>");
}

/** 排好之后还要补的占位：等着排版的公式 · 等着取的图。 */
const PENDING = "[data-math-pending], [data-image-pending]";

/**
 * 补卡片里惰路留下的占位（[`enhancePending`]）。幂等：标 `data-enhanced`；没有占位时一次 querySelector 就返回。
 */
export function enhanceCard(el: HTMLElement): void {
  if (el.dataset.enhanced === "1") return;
  el.dataset.enhanced = "1";
  enhancePending(el);
}

/**
 * 补 `el` 里此刻还在的占位：数学（`[data-math-pending]`）载 KaTeX（头一回才载）、排好原位替换；
 * 图（`[data-image-pending]`）这时才去取（`cards/attach-strip.ts::fillImage`）。
 * 不标记：同一块后来又长出占位（长正文点「显示全部」补的那几段）时再调一次，只补新的。
 */
export function enhancePending(el: HTMLElement): void {
  const pendings = el.querySelectorAll<HTMLElement>(PENDING);
  if (pendings.length === 0) return; // fast path: 没公式、没图
  const maths: HTMLElement[] = [];
  for (const block of pendings) {
    if (block.hasAttribute("data-math-pending")) maths.push(block);
    else fillImage(block);
  }
  if (maths.length === 0) return;
  const p: Promise<void> = loadKatex()
    .then((katex) => {
      // KaTeX 在路上时又补了一趟的那几处早被换掉了（没有父节点）⇒ 跳过。
      for (const ph of maths) if (ph.parentNode) enhanceMath(katex, ph);
    })
    .catch((e: unknown) => {
      // 载不到（打包缺块 · 离线）：留着 TeX 原文，去掉标记不反复重试。
      console.warn("[enhance] katex 没载到：", e);
      for (const ph of maths) ph.removeAttribute("data-math-pending");
    })
    .finally(() => mathInFlight.delete(p));
  mathInFlight.add(p);
}

/** 一个数学占位 → KaTeX，原位替换。KaTeX 出的是它自己拼好的标记（不带用户的 HTML：TeX 原文里的尖括号它转义）。 */
function enhanceMath(katex: Katex, ph: HTMLElement): void {
  const displayMode = ph.getAttribute("data-math-pending") === "display";
  const tex = ph.textContent ?? "";
  let html: string;
  try {
    html = katex.renderToString(tex, { ...KATEX_OPTIONS, displayMode });
  } catch (e) {
    // throwOnError:false 下不该走到这里；真走到就留着原文、去掉标记（不反复重试）
    console.warn("[enhance] katex failed:", e);
    ph.removeAttribute("data-math-pending");
    return;
  }
  const tpl = document.createElement("template");
  tpl.innerHTML = html;
  ph.replaceWith(...Array.from(tpl.content.childNodes));
}

/** 判据用：等在路上的公式都排好（KaTeX 头一回要载）。 */
export async function __mathSettledForTests(): Promise<void> {
  while (mathInFlight.size > 0) await Promise.all([...mathInFlight]);
}

/**
 * 每个滚动容器一个 IntersectionObserver（root = 那个容器）：卡片进可视区（± 300px，快滚到时就补、不让公式「弹」出来）调 enhanceCard，然后 unobserve。
 * root 必填：主窗里各 tab 的 `.stream` 叠在同一块区域（后台 tab 是 `visibility:hidden`，仍有几何），拿视口当 root 分不清卡属于哪一条流。
 *
 * - 同一个 root 复用一个 IO（`WeakMap`）；容器销毁时调 {@link releaseEnhanceRoot}（`TabStreamView.disposeTab` · `SessionViewer`）。
 * - 没有 IO 的环境（jsdom）⇒ 立即 enhance。
 */
const enhanceObservers = new WeakMap<HTMLElement, IntersectionObserver>();

/**
 * 让一个卡片接受 lazy enhance 调度（`root` = 它所在的滚动容器）。实时 tab 与查看器在 lazy 渲染期间挂卡片时调。
 * 里面没有等着补的占位（惰路只在建卡那一刻留占位）⇒ 不交给 IO：大多数卡没有公式 / 图，个个挂着 ＝ 滚动时每一帧
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
