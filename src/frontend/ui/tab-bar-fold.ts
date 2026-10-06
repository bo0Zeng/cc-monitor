/**
 * 标签页栏收成 44px 的图标条：窗宽 < 980 自己收；也可以手动收起 / 展开（命令面板），手动的状态记住。
 * 收不收由 `body[data-tab-bar="folded"]` 一个属性说（`styles.css` 那一组规则读它）。
 */
import { LS_KEYS, safeGet, safeSet } from "./local-storage";

const NARROW = "(width < 980px)";

/** 手动收起了没有（记住的）。 */
export function tabBarManuallyFolded(): boolean {
  return safeGet(LS_KEYS.tabBarFolded) === "1";
}

function sync(): void {
  const narrow = typeof window.matchMedia === "function" && window.matchMedia(NARROW).matches;
  if (narrow || tabBarManuallyFolded()) document.body.dataset.tabBar = "folded";
  else delete document.body.dataset.tabBar;
}

/** 起来时挂一次：照窗宽与记住的那一格收，窗宽跨过 980 时跟着变。 */
export function mountTabBarFold(): void {
  sync();
  if (typeof window.matchMedia === "function") window.matchMedia(NARROW).addEventListener("change", sync);
}

/** 命令面板「收起 / 展开标签页栏」：翻手动那一格并记住。 */
export function toggleTabBarFold(): void {
  safeSet(LS_KEYS.tabBarFolded, tabBarManuallyFolded() ? "" : "1");
  sync();
}
