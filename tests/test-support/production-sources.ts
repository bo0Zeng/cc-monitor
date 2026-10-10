/**
 * TS 侧的**生产源码遍历**：一个家，别处不再各写一份。
 *
 * # 为什么有这个文件
 *
 * `scanning-guard-registry.vitest.ts` 那条递减棘轮盯着「测试里做目录遍历的文件数」，
 * 理由与 Rust 侧的 `scan_tree!` 一样：**判据在自己的登记表/注释里找到自己 ⇒ 恒绿，
 * 而恒绿看起来和真绿一模一样**。08-08 我给 `events-batch-schedule.vitest.ts` 写第
 * 十份遍历时被它当场拦下（9 → 10）。
 *
 * 棘轮要的答案是「你靠什么读不到自己」。这里的答案是**按构造**：
 * 遍历只收**生产**文件（排掉 `.vitest.` / `.test.`），而判据都住在测试文件里。
 * ⇒ 与 Rust 侧 `guard_core::scan_tree!`「摘除调用者自己」同一个用意，
 * 只是 TS 这边的分界是文件名而不是 `#[cfg(test)]`。
 *
 * ⚠ 它**不剥注释**：要不要剥、按哪种语法剥，是调用方的事
 * （`test-support/strip-comments.ts` 是那件事的家）。两件事合成一个函数，
 * 会让「我到底看的是代码还是注释」这个问题在调用处看不出来 —— 本会话为这件事
 * 红过好几次（判据比对到了注释）。
 */
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { toPosix } from "./posix-path.ts";
import { REPO_ROOT } from "./repo-root.ts";

/** 不进遍历的目录：构建产物、依赖、生成物。 */
const SKIP_DIRS = new Set(["node_modules", "dist", "coverage", "generated", ".vite"]);

/**
 * 「把全部生产源码逐份过一遍 TS AST」那一族判据的单条期限。它们读 `src` 下每一份 `.ts` 并逐份建语法树，
 * 在满核带插桩的覆盖率跑里（每核一个插桩 worker）会超过 vitest 默认的 5 秒。只给这一族，全局期限不动。
 */
export const SCAN_TIMEOUT_MS = 30_000;

export interface ProductionSource {
  /** 相对仓根的路径（正斜杠）。 */
  file: string;
  /** 原样文本（**没剥注释**）。 */
  text: string;
}

/**
 * 递归收集某个子树下的**生产** `.ts` 文件。
 *
 * @param subdir 相对仓根，默认 `src`。
 */
export function productionTsFiles(subdir = "src"): ProductionSource[] {
  return collect(subdir, ".ts");
}

/**
 * 同一套遍历，收 `.rs`（文案表的 Rust 读口落地之后，`copy-table.vitest.ts` 的「引用」一侧要读它们）。
 * Rust 这一侧的判据住在仓外的 `tests/frontend/shell/` 等目录（经 `#[path]` 挂进来），`src/` 下的 `.rs` 就是生产段 ——
 * 与 `.ts` 同一个用意：判据读不到自己。⚠ 不剥注释（同上，那是调用方的事）。
 */
export function productionRsFiles(subdir: string): ProductionSource[] {
  return collect(subdir, ".rs");
}

/**
 * 同一套遍历，收手机端（`src/mobile`）的生产 Kotlin：只要各模块 `src/main/` 下的（Gradle 的测试源在 `src/test/` · `src/androidTest/`，
 * 文件名上分不出来，按目录分）。
 */
export function productionKtFiles(subdir = "src/mobile"): ProductionSource[] {
  return collect(subdir, ".kt").filter((f) => f.file.includes("/src/main/"));
}

/**
 * 同一套遍历，收 `.css`（`bg-flat.vitest.ts` 要看「`.tab-bg` 在全部样式里零处」）。
 * CSS 里没有判据住着 ⇒ 读不到自己按构造成立；借这一个家是为了不在测试里另写一份目录遍历（`scanning-guard-registry` 那条棘轮）。
 */
export function productionCssFiles(subdir = "src"): ProductionSource[] {
  return collect(subdir, ".css");
}

function collect(subdir: string, ext: string): ProductionSource[] {
  const root = resolve(REPO_ROOT, subdir);
  const out: ProductionSource[] = [];
  const walk = (dir: string): void => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const full = `${dir}/${e.name}`;
      if (e.isDirectory()) {
        if (!SKIP_DIRS.has(e.name)) walk(full);
        continue;
      }
      if (!e.name.endsWith(ext)) continue;
      // ★ 就是这一行让判据读不到自己：测试文件不进人群。
      if (e.name.includes(".vitest.") || e.name.includes(".test.")) continue;
      out.push({
        // `resolve` 在 Windows 上把 `subdir` 那一截拼成 `\` ⇒ 不规整就是 `src\common/copy-core/…`，与字面量住址对不上。
        file: toPosix(full.slice(REPO_ROOT.length + 1)),
        text: readFileSync(full, "utf8"),
      });
    }
  };
  walk(root);
  out.sort((a, b) => a.file.localeCompare(b.file));
  return out;
}
