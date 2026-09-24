/**
 * 原生文件窗口的前端开口 —— **全仓唯一一处**调 `open_file_window` 的地方。
 *
 * 老 SFTP 面板退役（`设计/60 §14`〔F7b〕，用户裁「另一个面板直接删」）之后，
 * 界面上打开远端文件的入口全部经这里，落点只有一种：那个独立进程里的原生窗口。
 *
 * # 三种落点 —— 与 Rust 侧 `filewin::entry::plan_target` 的优先级一一对应
 *
 * | 调用方给的 | 窗口开在哪 | 谁这么用 |
 * |---|---|---|
 * | `{ dir }` | 直接进这个**目录** | 远端会话「打开工作目录」 |
 * | `{ revealFile }` | 进它的父目录，**高亮那一行并滚进视野** | 会话工具卡上那个文件链接 |
 * | 不给 | 那台机器的 home | 顶栏按钮 · 命令面板 · 机器页「文件」 |
 *
 * ⚠ 父目录与尾段**由 Rust 侧切**（`filewin::source::parent_dir` / `remote_basename`），
 * 这里一个字节都不切 —— 前端多一份切远端路径的逻辑，就多一种 `\` 被当成分隔符的机会。
 *
 * # 失败要出声
 *
 * Rust 侧**先真的列一趟目录再开窗**，列不出来、通道没起来、窗口进程当场就退，都会 reject
 * ⇒ 这里 `await` 到的错是真错，原文放进 toast。成功不另外出声：窗口自己出现就是回应。
 */
import { commands } from "./ipc/commands";
import { showActionFailureToast } from "./error-toast";
import type { RemoteHostConfig } from "./remote-config";

/** 窗口开在哪（不给 ＝ 那台机器的 home）。 */
export type FileWindowTarget = { readonly dir: string } | { readonly revealFile: string };

/**
 * 在原生文件窗口里打开 `cfg` 那台远端。
 *
 * 回值 ＝ 窗口起来了（`false` ＝ 失败，且已经 toast 过）。
 */
export async function openFileWindow(cfg: RemoteHostConfig, at?: FileWindowTarget): Promise<boolean> {
  const path = at && "dir" in at ? at.dir : "";
  const revealFile = at && "revealFile" in at ? at.revealFile : null;
  try {
    await commands.open_file_window({ cfg, path, revealFile });
    return true;
  } catch (e) {
    showActionFailureToast("文件窗口打开失败", String(e));
    return false;
  }
}
