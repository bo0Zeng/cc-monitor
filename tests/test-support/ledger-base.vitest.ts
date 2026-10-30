/** 基线读法：在临时 git 仓里造一份 main 与一条分支，读回分叉点那一版。 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { baselineText } from "./ledger-base.ts";

const made: string[] = [];
afterEach(() => {
  for (const d of made.splice(0)) rmSync(d, { recursive: true, force: true });
});

function git(dir: string, ...args: string[]): void {
  execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", ...args], {
    cwd: dir,
    stdio: "ignore",
  });
}

/** main 上那份名单 3 行，分支上改成 5 行并提交；回仓的目录。 */
function repo(): string {
  const d = mkdtempSync(join(tmpdir(), "ledger-base-"));
  made.push(d);
  git(d, "init", "-q", "-b", "main");
  writeFileSync(join(d, "list.tsv"), "a\nb\nc\n");
  git(d, "add", "list.tsv");
  git(d, "commit", "-q", "-m", "base");
  git(d, "checkout", "-q", "-b", "work");
  writeFileSync(join(d, "list.tsv"), "a\nb\nc\nd\ne\n");
  git(d, "commit", "-q", "-am", "grow");
  return d;
}

describe("baselineText", () => {
  it("回分叉点那一版，不是此刻这一版", () => {
    expect(baselineText("list.tsv", repo())).toBe("a\nb\nc\n");
  });

  it("基线上没有这份 ⇒ null", () => {
    expect(baselineText("new.tsv", repo())).toBeNull();
  });

  it("仓里没有 main ⇒ 抛（读不到基线不许当成过）", () => {
    const d = repo();
    git(d, "branch", "-q", "-m", "main", "old");
    expect(() => baselineText("list.tsv", d)).toThrow("读不到基线");
  });
});
