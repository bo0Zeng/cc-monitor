/**
 * 「在文件夹中显示」—— 本机的一条路径：用系统文件管理器打开它所在的文件夹并选中它。
 * 全仓唯一一处调 `revealItemInDir` 的地方（远端的路径走我们的文件窗口，`file-window.ts`）。
 *
 * 只是打开文件管理器，不改盘；失败出声（原文进 toast）。
 */
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";

export async function revealInFolder(path: string): Promise<void> {
  try {
    await revealItemInDir(path);
  } catch (e) {
    showActionFailureToast(copyText("revealInFolder.reveal.failed"), String(e));
  }
}
