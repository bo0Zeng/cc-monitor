/**
 * **设置窗**的入口（`settings.html` 加载它）。：只含设置面板 ＋ 主题 ＋ 键位。
 *
 * 🔴 本窗的模块图里**根本没有**语法高亮（highlight.js）/ 数学排版（katex）/ markdown（marked）/
 * tab 管理（`tabs.ts`）/ 渲染栈（`render.ts`）。这不是「看起来拆开了」—— 判据是
 * `tests/frontend/ui/entry-graphs.vitest.ts`：真跑一遍 `vite build`，对设置窗入口 chunk 的传递闭包做零命中断言。
 * 往这里（或 `settings/**` 的依赖链上）加一个会带进渲染栈的 import，那条当场红。
 *
 * 原先设置窗加载 `index.html?settings=1`，由 `main.ts` 在 DOMContentLoaded 里分叉 ——
 * 分叉之前整份主窗口包已经解析完了。
 */
import { installExternalLinkDelegation } from "./entry-common"; // 顺带装上全局错误捕获（模块副作用）
import { loadTheme } from "./theme";
import { SettingsPanel } from "./settings";
import { dispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";

// Vite HMR：任何热更新一律整页重载（理由见 `main.ts` 同名那段：部分热替换会让旧 listener 与新代码并存）。
if (import.meta.hot) {
  import.meta.hot.accept(() => {
    window.location.reload();
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  // 主题尽早应用，避免渲染抖动
  await loadTheme();
  await bootstrapSettings();
});

/**
 * F82a（#56+#47）：独立**设置窗口**的精简 bootstrap —— 只挂 SettingsPanel（windowMode），
 * 无 tab / 历史 chrome。照 viewer §22 范式。设置项经既有 config 命令读写（窗口无关）；
 * 保存 / 行为 toggle 后 panel 广播 `settings-applied`，主窗口 listen 并重读应用主题+行为（跨窗同步）。
 * 主题已在本文件的 DOMContentLoaded 里先 `loadTheme()` 应用（本 fn 在其后），此处不重复。
 * **窗体渲染/布局本环境无 GUI 不可自测 → 真机验证累积**（照 viewer 已验证脚手架把盲实现风险压最低）。
 */
async function bootstrapSettings(): Promise<void> {
  document.body.classList.add("settings-window-mode");
  // 同 viewer：外链走全局 click 代理（防未来设置里的外链在本 WebView 打开顶掉 UI）。
  installExternalLinkDelegation(); // 设置窗没有代码块，只要外链那一半（`entry-render-common.ts` 头注）
  const panel = new SettingsPanel({ windowMode: true });
  await panel.open(); // 面板压 overlay 栈底
  // ★ 设置窗有自己的 dispatcher 实例，必须 applyOverrides + start()（同 viewer bootstrap）——否则
  //   ① 快捷键编辑器录制收不到键（onKeyDown 未挂）；② Esc 无法经 overlay LIFO 逐层关（编辑器 / SFTP
  //   面板压栈其上时先关它们，栈底面板 handleEsc→cancel→关窗）。不 bind app 动作（本窗无 tab 等），
  //   overlay.close(Esc) 与录制都不依赖 bind。（原手搓的 window Esc 监听会双关窗，已删。）
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();
}
