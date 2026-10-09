/**
 * 写剪贴板：全产品的复制（复制详情 · 复制命令 · 代码块复制 · 诊断信息 …）只这一个口。
 *
 * 写由壳经系统接口做（`clipboard_write` · `clipboard.rs`），回的是真成败 —— 网页自己的 `navigator.clipboard`
 * 在 WebView2 上剪贴板被别的程序占着时也回成功（WIN5 · 10-08），界面就会说「已复制」而什么都没写进去。
 * 写不进 ⇒ 抛壳回的那个错（`SaidError`：一句「写不进剪贴板」＋ 复制详情）；调用方据此不说「已复制」、改走就地展开或报错。
 */
import { commands } from "./ipc/commands";

/** 把 `text` 写进系统剪贴板；写不进就抛。 */
export async function writeClipboard(text: string): Promise<void> {
  await commands.clipboard_write({ text });
}
