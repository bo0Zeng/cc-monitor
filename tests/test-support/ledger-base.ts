/**
 * 「只许变短」的名单对着基线判：基线 ＝ 此刻 HEAD 与 main 的分叉点上那一版（`git show <merge-base>:<路径>`）。
 *
 * 名单与上限住同一个改动面，同一次提交就能一起放大；对着分叉点判，放大的那一次在这条分支上一直红，提交了也不变绿。
 * 读不到基线（没有 git · 没有 main · 浅克隆）⇒ 抛，不当成过。路径在基线上还没有 ⇒ `null`（新名单没有基线，由调用方判）。
 *
 * Rust 侧同口径的一份是 `guard_core::baseline_text`；两边认的分支名都是 `main` · `origin/main`。
 */
import { execFileSync } from "node:child_process";

import { REPO_ROOT } from "./repo-root.ts";

/** 基线分支：本地的 `main`，没有就用远端跟踪的 `origin/main`（CI 的检出只有后者）。 */
const BASE_REFS = ["main", "origin/main"] as const;

function git(root: string, args: string[]): string | null {
  try {
    return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch {
    return null;
  }
}

/** `rel`（相对仓根）在基线上的正文；基线上没有这份 ⇒ `null`；基线本身读不到 ⇒ 抛。 */
export function baselineText(rel: string, root: string = REPO_ROOT): string | null {
  const ref = BASE_REFS.find((r) => git(root, ["rev-parse", "--verify", "-q", `${r}^{commit}`]) !== null);
  if (ref === undefined) throw new Error(`读不到基线：${root} 里 ${BASE_REFS.join(" · ")} 都不在（CI 的检出要 fetch-depth: 0）`);
  const base = git(root, ["merge-base", "HEAD", ref])?.trim();
  if (!base) throw new Error(`读不到基线：HEAD 与 ${ref} 找不到分叉点（浅克隆把历史截掉了？）`);
  if (git(root, ["cat-file", "-e", `${base}:${rel}`]) === null) return null;
  const text = git(root, ["show", `${base}:${rel}`]);
  if (text === null) throw new Error(`读不到基线：git show ${base}:${rel} 失败`);
  return text;
}
