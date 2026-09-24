/**
 * 三个入口共用的那一小撮（`设计/01 §1.2` 三入口拆分）。
 *
 * | html | 入口模块 | 装什么 |
 * |---|---|---|
 * | `index.html` | `entry-main.ts` | 主窗口（`main.ts` 全套） |
 * | `settings.html` | `entry-settings.ts` | 只含设置面板 ＋ 主题 ＋ 键位 |
 * | `viewer.html` | `entry-viewer.ts` | 只含 tab 管理 ＋ 渲染栈 |
 *
 * 🔴 **本文件会进三个窗口的模块图** ⇒ 它只许 import 三个窗口都该有的东西。
 * 今天只有一个 `@tauri-apps/plugin-opener`（外链走系统浏览器）。代码块「复制」那半只有主窗与
 * viewer 要，住 `entry-render-common.ts`。往这里加一个 import，
 * 就是往设置窗里塞一个模块 —— `tests/entry-graphs.vitest.ts` 对构建产物的模块图做零命中断言，
 * 塞进高亮 / 数学排版 / tab 管理会当场红。
 *
 * 原先这几件住 `main.ts`，三个窗口都加载 `main.ts`，所以顺带都有；拆开之后要显式共用。
 */
import { openUrl } from "@tauri-apps/plugin-opener";

// 全局错误捕获 —— 渲染到 status-bar 便于无 devtools 也能诊断。
// ⚠ 设置窗没有 `#status-bar`（`settings.html` 只有一个空 body），那边只进 console。
window.addEventListener("error", (e) => {
  // Batch13-F38:c-v 材料化→RO→snap 链路使这条**良性**浏览器提示变频繁
  // (RO 回调引发布局变化,浏览器推迟到下帧,无功能影响)——不上状态栏不进 console
  if (e.message.includes("ResizeObserver loop")) return;
  const bar = document.getElementById("status-bar");
  if (bar) bar.textContent = `ERR: ${e.message} @ ${e.filename}:${e.lineno}`;
  console.error("global error:", e.error ?? e.message);
});
window.addEventListener("unhandledrejection", (e) => {
  const bar = document.getElementById("status-bar");
  if (bar) bar.textContent = `REJ: ${e.reason}`;
  console.error("unhandled rejection:", e.reason);
});

/**
 * 外链的全局 click 代理。三个窗口共用（设置窗里也有外链）。
 * - 外链（render.ts 标 data-external）：preventDefault + openUrl 走系统浏览器，
 *   避免 WebView2 把 UI 替换成外站。capture 阶段顶层接管防被卡片 handler 抢先。
 */
export function installExternalLinkDelegation(): void {
  document.addEventListener(
    "click",
    (e) => {
      const a = (e.target as HTMLElement | null)?.closest?.(
        "a[data-external]",
      ) as HTMLAnchorElement | null;
      if (!a) return;
      const href = a.getAttribute("href") ?? "";
      if (!href) return;
      e.preventDefault();
      void openUrl(href).catch((err) => {
        console.warn("[external link] openUrl failed:", href, err);
      });
    },
    true,
  );
}
