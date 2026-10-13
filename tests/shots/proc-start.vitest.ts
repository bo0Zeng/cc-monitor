/**
 * 截图台架给替身写 pidfile 时取的进程起始时刻（`real/pool.mjs::procStartSh`）不受进程名里的空格影响。
 *
 * 起因：替身是 tmux 窗格里的进程，`new-session -P` 一答出窗格 pid 台架就去读 `/proc/<pid>/stat`；那一刻它可能还没 exec，
 * 进程名还是 `tmux: server`（带空格）⇒ 按空格数第 22 格数错一格，写进 pidfile 的 `procStart` 不是它 ⇒ 后端照真的那一套判「这个 pid 不是写这份
 * pidfile 的那个 claude」、不宣告它 ⇒ 截图里少一个 tab（偶发，约千分之一到千分之三张）。
 *
 * 人群：`procStartSh` 那一句（沙箱开场脚本里唯一取起始时刻的地方）。
 * 买不到：真 tmux 窗格那一刻进程名是什么不在这里造（造一份进程名带空格、带右括号的 stat 行）。
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";

// 池子是不带类型的 .mjs（vite 插件那一侧直接跑它）：按路径取那一个函数。
const POOL = "./real/pool.mjs";
const { procStartSh } = (await import(POOL)) as { procStartSh: (stat: string) => string };

/** 一行 stat：进程名之后的第 20 格（全行第 22 格）是起始时刻。 */
function statLine(comm: string, start: number): string {
  const after = ["S", "1", "7", "7", "0", "-1", "4194560", "0", "0", "0", "0", "0", "0", "0", "0", "0", "20", "0", "1", String(start), "8192"];
  return `7 (${comm}) ${after.join(" ")}\n`;
}

describe("替身 pidfile 的起始时刻", () => {
  for (const comm of ["sleep", "tmux: server", "a) b (c"]) {
    it(`进程名「${comm}」也取对第 22 格`, () => {
      const dir = mkdtempSync(path.join(os.tmpdir(), "shots-stat-"));
      try {
        const stat = path.join(dir, "stat");
        writeFileSync(stat, statLine(comm, 146973570));
        const got = execFileSync("sh", ["-c", procStartSh(`'${stat}'`)], { encoding: "utf8" }).trim();
        expect(got).toBe("146973570");
      } finally {
        rmSync(dir, { recursive: true, force: true });
      }
    });
  }
});
