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
 *
 * # 〔FW34〕〔待退役〕老面板书签的搬家那一跳
 *
 * 老 SFTP 面板的书签留在本 webview 的 localStorage 里，而窗口是独立进程、读不到这里 ⇒
 * 开窗时顺手带过去（`carryBookmarks`，Rust 侧 `filewin::entry::carry_legacy` 并进书签文件），
 * **开窗成功才删旧键**（Rust 侧先搬、搬不成整趟报错）。逐条理由住 [`legacyBookmarks`]。
 */
import { commands } from "./ipc/commands";
import { showActionFailureToast } from "./error-toast";
import { enumeratePrefix, LS_KEYS, safeRemove } from "./local-storage";
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
  const legacy = legacyBookmarks();
  try {
    await commands.open_file_window(
      legacy ? { cfg, path, revealFile, carryBookmarks: legacy.carry } : { cfg, path, revealFile },
    );
    // 搬成了（Rust 侧先搬、搬不成整趟报错）⇒ 旧键这才删；失败一把不删，下一次再搬（合并是幂等的）。
    for (const k of legacy?.keys ?? []) safeRemove(k);
    return true;
  } catch (e) {
    showActionFailureToast("文件窗口打开失败", String(e));
    return false;
  }
}

/**
 * 〔FW34〕〔待退役〕老 SFTP 面板留在 webview localStorage 里的目录书签（面板 F7b 删了，数据没删）。
 *
 * 窗口是独立进程，读不到这里 ⇒ 开窗那一跳顺手带过去（`carryBookmarks`，Rust 侧并进书签文件）。
 * **一次搬全部机器**，不只这一台：键是按机器分的，而开窗只开一台 —— 只搬这一台的话，
 * 从此不再开的那几台的书签就永远留在这里。
 *
 * 回 `null` ＝ 没有要搬的（没有旧键 ⇒ 开窗实参里**不带**这一格，与搬家之前逐字相同）。
 * 解析不了的那一把跳过、**也不删**（不认得的东西不替用户扔）。
 * 退役条件见 `LS_KEYS.legacySftpBookmarksPrefix`。
 */
function legacyBookmarks(): { carry: Record<string, string[]>; keys: string[] } | null {
  const prefix = LS_KEYS.legacySftpBookmarksPrefix;
  const carry: Record<string, string[]> = {};
  const keys: string[] = [];
  for (const { key, value } of enumeratePrefix(prefix)) {
    const origin = key.slice(prefix.length);
    let list: unknown;
    try {
      list = JSON.parse(value);
    } catch {
      continue;
    }
    if (!origin || !Array.isArray(list) || !list.every((x) => typeof x === "string")) continue;
    carry[origin] = list as string[];
    keys.push(key);
  }
  return keys.length > 0 ? { carry, keys } : null;
}
