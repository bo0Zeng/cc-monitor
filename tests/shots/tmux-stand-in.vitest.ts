/**
 * 截图台架那台私有 tmux 的两样输入（`real/pool.mjs`）：各活会话的 tmux 会话名 · 边界上的 tmux 替身。
 *
 * - 会话名照起会话那一处铸名的样子（`<目录名>-cc`，重名往后加 `-2`），先让开不住 claude 的那几个：重了的话名单里两行同名，
 *   终端页 · 未报到那一卡按名字认错终端。
 * - 替身只挡给定的那两问（版本 · 「没有 server」），其余原样交真 tmux：挡多了真后端读到的就不是那台 tmux。
 *
 * 买不到：替身之后那一跳（`exec` 真 tmux）不在这里起；真后端照它读出什么由截图看。
 */
import { execFileSync, spawnSync } from "node:child_process";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";

const POOL = "./real/pool.mjs";
type Live = { sid: string; cwd: string; tmux?: false };
const { tmuxNames, tmuxShim } = (await import(POOL)) as {
  tmuxNames: (m: { live: Live[]; bareTmux?: { name: string; screen: string }[] }) => Map<number, string>;
  tmuxShim: (shim: { version?: string; noServer?: string[] }, sock: string) => string;
};

describe("各活会话的 tmux 会话名", () => {
  it("目录名 ＋ -cc，重名往后编号，先让开不住 claude 的那几个；不住 tmux 的没有", () => {
    const live: Live[] = [
      { sid: "a", cwd: "/w/orders" },
      { sid: "b", cwd: "/x/orders" },
      { sid: "c", cwd: "/w/notes", tmux: false },
      { sid: "d", cwd: "/w/notes" },
    ];
    expect([...tmuxNames({ live, bareTmux: [{ name: "orders-cc-2", screen: "" }] })]).toEqual([
      [0, "orders-cc"],
      [1, "orders-cc-3"],
      [3, "notes-cc"],
    ]);
  });
});

describe("边界上的 tmux 替身", () => {
  const run = (shim: { version?: string; noServer?: string[] }, args: string[]) => {
    const dir = mkdtempSync(path.join(os.tmpdir(), "shots-tmux-shim-"));
    try {
      const bin = path.join(dir, "tmux");
      writeFileSync(bin, tmuxShim(shim, "/s/default"));
      chmodSync(bin, 0o755);
      return spawnSync(bin, args, { encoding: "utf8", env: { PATH: "/usr/bin:/bin" } });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };

  it("问版本答给定的那个；挡下的子命令照没有 server 那一句失败", () => {
    const v = run({ version: "3.1c", noServer: ["capture-pane"] }, ["-u", "display-message", "-p", "#{version}"]);
    expect([v.status, v.stdout]).toEqual([0, "3.1c\n"]);
    const c = run({ noServer: ["capture-pane"] }, ["-u", "capture-pane", "-p", "-t", "%1"]);
    expect([c.status, c.stderr]).toEqual([1, "no server running on /s/default\n"]);
  });

  // 只拿 `-V` 验转交（不连任何 server，碰不到用户的 tmux）。
  it("别的原样交真 tmux", () => {
    const real = execFileSync("env", ["-u", "TMUX", "-u", "TMUX_PANE", "tmux", "-L", "shots-stand-in", "-V"], { encoding: "utf8" });
    expect(run({ version: "3.1c", noServer: ["capture-pane"] }, ["-V"]).stdout).toBe(real);
  });
});
