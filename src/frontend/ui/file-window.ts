/**
 * 原生文件窗口的前端开口 —— **全仓唯一一处**调 `open_file_window` 的地方。
 *
 * 老 SFTP 面板退役之后，
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
 * ⇒ 这里 `await` 到的错是真错，那一句与复制详情放进 toast。成功不另外出声：窗口自己出现就是回应。
 */
import { commands } from "./ipc/commands";
import { failToast } from "./kit/toast";
import { copyText } from "./copy-table";
import { isLocalOrigin, type Machine } from "./ipc/origin";
import { revealInFolder } from "./reveal-in-folder";
import { THEME_TOKENS } from "./generated/filewin-theme-tokens";

/**
 * 文件窗口要的那几格设计令牌此刻在 `:root` 上的计算值（含用户在设置里改过的），原样交给 Rust 侧，
 * 由它解成数放进开窗种子 —— 窗口照着画，自己不另有一份色值。名单读契约那一份的生成物（`filewin_contract::THEME_TOKENS`）。
 */
export function fileWindowTheme(): Record<string, string> {
  const css = getComputedStyle(document.documentElement);
  return Object.fromEntries(THEME_TOKENS.map((t) => [t, css.getPropertyValue(t).trim()]));
}

/** 窗口开在哪（不给 ＝ 那台机器的 home）。 */
export type FileWindowTarget = { readonly dir: string } | { readonly revealFile: string };

/**
 * 在原生文件窗口里打开那台远端：只交它的名字（机器表里那一格；认那台 · 认不出那一句都在壳那一头）。
 *
 * 回值 ＝ 窗口起来了（`false` ＝ 失败，且已经 toast 过）。
 */
export async function openFileWindow(machine: string, at?: FileWindowTarget): Promise<boolean> {
  const path = at && "dir" in at ? at.dir : "";
  const revealFile = at && "revealFile" in at ? at.revealFile : null;
  try {
    await commands.open_file_window({ origin: machine, path, revealFile, theme: fileWindowTheme() });
    return true;
  } catch (e) {
    failToast(copyText("fileWindow.openFileWindow.failed"), e, { fact: machine });
    return false;
  }
}

/**
 * 「在文件窗口打开」那一路（会话来源 ⇒ 落点）：本机 ⇒ 系统文件管理器里显示它；远端 ⇒ 文件窗口开到它。
 * 远端那台认不出（被删 / 改名）⇒ 壳那一句进失败 toast。
 */
export async function openOnMachine(origin: Machine, at: FileWindowTarget): Promise<void> {
  if (isLocalOrigin(origin)) {
    await revealInFolder("dir" in at ? at.dir : at.revealFile);
    return;
  }
  await openFileWindow(origin, at);
}
