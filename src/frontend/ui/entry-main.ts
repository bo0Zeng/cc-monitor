/**
 * **主窗口**的入口（`index.html` 加载它）。三入口拆分的第一格。
 *
 * 主窗口的 bootstrap 本体仍住 `main.ts`（一大批判据按住址读它）；本文件只负责
 * 「主窗口的模块图从这里起」这一件事，与 `entry-settings.ts` / `entry-viewer.ts` 对称。
 */
import "./entry-common";
import "./main";
