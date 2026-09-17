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

/**
 * 〔src/test 分离〕把「我所在的 `test/…` 目录」映射到**被守对象所在的 `src/…` 目录**。
 *
 * # 为什么需要它
 *
 * 源码级守卫过去用 `__dirname` 当「被守对象的目录」—— 那在测试与被守文件同住一个目录时
 * 成立。两棵树分开后不成立了，而 `test/` 与 `src/` 是**严格镜像**，所以映射就是换掉路径里
 * 那一段目录名。
 *
 * # 为什么是一个函数而不是各处一行正则
 *
 * 这条规则有 14 个消费者。写 14 遍 = 同一形状出现 14 次，改镜像约定时要改 14 处、
 * 漏一处就是**静默读错树**（读到不存在的路径会 ENOENT 当场红，但读到**同名的另一棵树**
 * 会安静地量错东西）。⇒ 一个东西一个住址。
 *
 * ⚠ 只换**最后一个** `test` 段，且要求它是完整一段（`/test/` 或结尾 `/test`）——
 * 避免把 `…/test/latest/…` 里的 `latest` 误伤。
 */
export function srcDirOf(testDir: string): string {
  const norm = testDir.split("\\").join("/");
  const m = norm.match(/^(.*?)\/test(\/.*)?$/);
  if (!m) {
    throw new Error(
      `srcDirOf 期望一个 test/ 下的目录，拿到的是 ${testDir}` +
        " —— 若目录布局变了，改这一处，别在调用方各自绕开。",
    );
  }
  return `${m[1]}/src${m[2] ?? ""}`;
}
