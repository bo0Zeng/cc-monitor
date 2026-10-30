/**
 * 测试里读目录的**唯一一处**：别的测试文件不直接 `readdirSync`（`scanning-guard-registry.vitest.ts` 判，`tests/test-support/` 以外零处）。
 *
 * 判据读不到自己靠的是**构造**：默认只收**生产**文件（文件名带 `.vitest.` / `.test.` 的不进人群），而判据都住在测试文件里。
 * 要读测试目录本身（截图场景 · 起浏览器的脚本）的，显式给 `tests: true`。
 *
 * ⚠ 它**不剥注释**：要不要剥、按哪种语法剥，是调用方的事（`test-support/strip-comments.ts` 是那件事的家）。
 * 两件事合成一个函数，会让「我到底看的是代码还是注释」在调用处看不出来。
 */
import { readdirSync, readFileSync } from "node:fs";
import { isAbsolute, resolve } from "node:path";

import { toPosix } from "./posix-path.ts";
import { REPO_ROOT } from "./repo-root.ts";

/** 不进遍历的目录：构建产物、依赖。`generated` 另由 `generated` 那一格管。 */
const SKIP_DIRS = new Set(["node_modules", "dist", "coverage", ".vite", "target", ".git"]);

export interface ProductionSource {
  /** 仓里的 ⇒ 相对仓根（正斜杠）；仓外的（`subdir` 给的是仓外的绝对路径）⇒ 绝对路径（正斜杠）。 */
  file: string;
  /** 原样文本（**没剥注释**）。 */
  text: string;
}

export interface WalkOptions {
  /** 收哪几种后缀（任一个结尾就收）：`".ts"` · `[".ts", ".kt"]`。`.d.css.ts` 也是 `.ts` 结尾，要排掉的调用方自己滤。 */
  ext: string | readonly string[];
  /** 收不收测试文件（名字带 `.vitest.` / `.test.`）。默认不收：判据读不到自己。 */
  tests?: boolean;
  /** 进不进 `generated/` 目录。默认不进（生成物另有判据）；`subdir` 本身就是生成目录的照读。 */
  generated?: boolean;
  /** 只读这一层，不往子目录走。 */
  shallow?: boolean;
  /** 另外不进的目录名。 */
  skipDirs?: readonly string[];
  /** 一份都没有也行（默认空人群当场抛：扫描判据在空人群上的「零命中」恒真）。 */
  allowEmpty?: boolean;
}

/**
 * 某个子树下照 `opts` 挑中的文件路径（不读内容），按路径排好。
 *
 * @param subdir 相对仓根的目录，或一个绝对路径（仓外的样本目录）。
 */
export function filePaths(subdir: string, opts: WalkOptions): string[] {
  const exts = typeof opts.ext === "string" ? [opts.ext] : opts.ext;
  const skip = new Set([...SKIP_DIRS, ...(opts.skipDirs ?? []), ...(opts.generated ? [] : ["generated"])]);
  const root = isAbsolute(subdir) ? subdir : resolve(REPO_ROOT, subdir);
  const out: string[] = [];
  const walk = (dir: string): void => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const full = `${dir}/${e.name}`;
      if (e.isDirectory()) {
        if (!opts.shallow && !skip.has(e.name)) walk(full);
        continue;
      }
      if (!exts.some((x) => e.name.endsWith(x))) continue;
      // ★ 就是这一行让判据读不到自己：测试文件默认不进人群。
      if (!opts.tests && (e.name.includes(".vitest.") || e.name.includes(".test."))) continue;
      out.push(relToRepo(full));
    }
  };
  walk(root);
  if (out.length === 0 && !opts.allowEmpty) throw new Error(`${subdir} 下一份 ${exts.join(" / ")} ${opts.tests ? "" : "生产"}文件都没扫到 —— 遍历坏了`);
  return out.sort((a, b) => a.localeCompare(b));
}

/** 同一套挑法，连内容一起读。 */
export function sourceFiles(subdir: string, opts: WalkOptions): ProductionSource[] {
  return filePaths(subdir, opts).map((file) => ({ file, text: readFileSync(isAbsolute(file) ? file : resolve(REPO_ROOT, file), "utf8") }));
}

/** 生产 `.ts`（默认 `src`）。 */
export function productionTsFiles(subdir = "src"): ProductionSource[] {
  return sourceFiles(subdir, { ext: ".ts" });
}

/** 生产 `.rs`：Rust 这一侧的判据住在 `tests/` 底下（经 `#[path]` 挂进来），`src/` 下的 `.rs` 就是生产段。 */
export function productionRsFiles(subdir: string): ProductionSource[] {
  return sourceFiles(subdir, { ext: ".rs" });
}

/**
 * 手机端（`src/mobile`）的生产 Kotlin：只要各模块 `src/main/` 下的（Gradle 的测试源在 `src/test/` · `src/androidTest/`，文件名上分不出来，按目录分）。
 */
export function productionKtFiles(subdir = "src/mobile"): ProductionSource[] {
  return sourceFiles(subdir, { ext: ".kt" }).filter((f) => f.file.includes("/src/main/"));
}

/** 生产 `.css`（默认 `src`）。 */
export function productionCssFiles(subdir = "src"): ProductionSource[] {
  return sourceFiles(subdir, { ext: ".css" });
}

/** 仓里的路径 ⇒ 相对仓根；仓外的 ⇒ 原样绝对路径。一律正斜杠（Windows 上 `resolve` 给的是 `\`）。 */
function relToRepo(full: string): string {
  const p = toPosix(full);
  const root = toPosix(REPO_ROOT);
  return p.startsWith(`${root}/`) ? p.slice(root.length + 1) : p;
}
