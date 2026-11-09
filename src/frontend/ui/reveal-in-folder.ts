/**
 * 「在文件夹中显示」—— 本机的一条路径：用系统文件管理器打开它所在的文件夹并选中它（壳命令 `os_open` 的 `reveal` 那一形）。
 * 界面里唯一一处交 `reveal` 的地方（远端的路径走我们的文件窗口，`file-window.ts`）。
 *
 * 只是打开文件管理器，不改盘；失败出声（原文进 toast）。
 */
import { commands } from "./ipc/commands";
import { failToast } from "./kit/toast";
import { copyText } from "./copy-table";

export async function revealInFolder(path: string): Promise<void> {
  try {
    await commands.os_open({ kind: "reveal", path });
  } catch (e) {
    failToast(copyText("revealInFolder.reveal.failed"), e);
  }
}
