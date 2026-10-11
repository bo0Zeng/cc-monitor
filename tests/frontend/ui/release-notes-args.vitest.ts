// @vitest-environment node
/**
 * 发版正文生成器（`tests/scripts/release-notes.mjs`）的位置参数是**输出文件**：传一个像版本号的词（`4.1.8` · `v4.1.8`）
 * 就当场拒、说清要的是什么，一个字节不写。起因：当成版本号传进去，它就在当前目录写出一份名叫 `4.1.8` 的正文，
 * 被 `commit -a` 带进了仓根。正常那两种用法（`<输出文件>` · `--check`）照旧。
 */
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const SCRIPT = resolve(REPO_ROOT, "tests/scripts/release-notes.mjs");
const boxes: string[] = [];
function box(): string {
  const d = mkdtempSync(join(tmpdir(), "release-notes-"));
  boxes.push(d);
  return d;
}
afterEach(() => {
  for (const d of boxes.splice(0)) rmSync(d, { recursive: true, force: true });
});
function run(cwd: string, ...args: string[]) {
  return spawnSync(process.execPath, [SCRIPT, ...args], { cwd, encoding: "utf8" });
}

describe("release-notes.mjs 的位置参数只认输出文件", () => {
  for (const word of ["4.1.8", "v4.1.8", "10.0.12"]) {
    it(`★ 像版本号的「${word}」⇒ 退出码 1、说清要的是输出文件、当前目录里什么都没写`, () => {
      const cwd = box();
      const r = run(cwd, word);
      expect(r.status).toBe(1);
      expect(r.stderr).toContain("输出文件");
      expect(r.stderr).toContain("--check");
      expect(existsSync(join(cwd, word)), `写出了一份 ${word}`).toBe(false);
    });
  }

  it("正常用法照旧：给输出文件 ⇒ 写出正文；--check ⇒ 只验不写", () => {
    const cwd = box();
    const out = join(cwd, "RELEASE_BODY.md");
    const w = run(cwd, out);
    expect(w.status, w.stderr).toBe(0);
    expect(readFileSync(out, "utf8")).toMatch(/^# cc-monitor v\d+\.\d+\.\d+\n/);
    const c = run(cwd, "--check");
    expect(c.status, c.stderr).toBe(0);
  });
});
