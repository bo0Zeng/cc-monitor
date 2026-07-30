/**
 * 仓库根的单一来源——**守卫用**。
 *
 * ## 为什么需要它
 *
 * C04a Phase D 审计（J7）指出：两个守卫各自算仓库根，深度约定已经不一样了
 * ——`generated-boundary-guard.vitest.ts` 在 `src/` 下用 `resolve(dirname, "..")`，
 * `ipc/commands.vitest.ts` 在 `src/ipc/` 下用 `resolve(dirname, "..", "..")`。
 * 主计划说这个守卫形状要复制 127 次，抄错深度是必然会发生的事。
 *
 * 抄错的后果是 `readFileSync` 的 ENOENT **硬失败**（不是静默假绿），所以这不是洞、是效率问题
 * ——但既然本文件所在目录是固定的，让每个守卫自己数 `..` 就没有意义。
 *
 * **本函数的 `import.meta.dirname` 恒等于 `<repo>/src/test-support`，与调用方在哪无关**
 * ——这正是它能当单一来源的原因。实现上仍然向上找 `package.json` 而不是硬编码两个 `..`，
 * 这样万一 `test-support/` 被搬走也不会静默指错地方。
 */
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";

function findRoot(): string {
  let dir = import.meta.dirname;
  for (let up = 0; up < 8; up++) {
    if (existsSync(resolve(dir, "package.json"))) return dir;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error(`找不到仓库根（从 ${import.meta.dirname} 向上找 package.json 失败）`);
}

/** 仓库根的绝对路径（含 `package.json` 的那一层）。 */
export const REPO_ROOT: string = findRoot();
