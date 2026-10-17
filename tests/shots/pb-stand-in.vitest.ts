/**
 * 截图台架边界上的 pb 替身（`disk/plan.ts`）：真后端照常找 pb 插件、起 `python3 <入口> <动词>`，碰到的是它。
 * 它只把工作区里那一份 dump 原样吐出来、照几条命令改那一份，演 pb 的几种退出码；成品（数 · 排 · 短词 · 时刻字）全由真后端算。
 *
 * 买不到：真 pb 读 `图.md` 那一段不在这里；dump 的形状对不对由真后端读它（截图台架）判。
 */
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { blankDisk } from "./disk";
import { PB_ENTRY, putPb, putWorkspace, type PbStandIn } from "./disk/plan";

const dirs: string[] = [];
afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

const DOC = { pb: "0.2.0", shape: 1, workspace: "/w", repo: "alpha", auto: false, slices: [{ name: "alpha", domain: "软件", docs_only: true, path: "repos/alpha", cells: [] }, { name: "beta", domain: "研究", cells: [] }] };

/** 照 `putPb` · `putWorkspace` 落一个家目录，回它与工作区根。 */
function home(cfg: PbStandIn = {}, ws = "work/alpha"): { root: string; ws: string } {
  const root = mkdtempSync(path.join(os.tmpdir(), "shots-pb-"));
  dirs.push(root);
  const d = blankDisk();
  putPb(d, cfg);
  putWorkspace(d, ws, DOC);
  for (const [rel, text] of Object.entries(d.files)) {
    mkdirSync(path.dirname(path.join(root, rel)), { recursive: true });
    writeFileSync(path.join(root, rel), text);
  }
  mkdirSync(path.join(root, ws, "sub"), { recursive: true });
  return { root, ws: path.join(root, ws) };
}

function pb(root: string, cwd: string, ...args: string[]): { rc: number | null; out: string; err: string } {
  const r = spawnSync("python3", [path.join(root, PB_ENTRY), ...args], { cwd, encoding: "utf8", env: { PATH: "/usr/bin:/bin" } });
  return { rc: r.status, out: r.stdout, err: r.stderr };
}

const dumped = (root: string, cwd: string): Record<string, unknown> => JSON.parse(pb(root, cwd, "dump").out) as Record<string, unknown>;

describe("pb 替身", () => {
  it("插件清单的名字是 planned-build，入口在 pb 的位置", () => {
    const d = blankDisk();
    putPb(d);
    const manifest = Object.entries(d.files).find(([k]) => k.endsWith(".claude-plugin/plugin.json"));
    expect(JSON.parse(manifest![1]).name).toBe("planned-build");
    expect(path.relative(path.dirname(path.dirname(manifest![0])), PB_ENTRY)).toBe("skills/planned-build/tools/entry.py");
  });

  it("dump：从当前目录往上找工作区，原样吐那一份；不在工作区里退 3", () => {
    const { root, ws } = home();
    expect(dumped(root, path.join(ws, "sub"))).toEqual(DOC);
    const off = pb(root, root, "dump");
    expect([off.rc, off.out]).toEqual([3, ""]);
    expect(off.err).not.toBe("");
  });

  it("help 退 0（设路径时那一问）", () => {
    const { root } = home();
    expect(pb(root, root, "help").rc).toBe(0);
  });

  it("没有 dump 的旧 pb：dump 退 4", () => {
    const { root, ws } = home({ dumpRc: 4 });
    expect(pb(root, ws, "dump").rc).toBe(4);
  });

  it("头一次之后那一片读不成：第二次起那一片只剩名字 · 领域 · 原因", () => {
    const { root, ws } = home({ brokenAfterFirst: { slice: "alpha", said: "图.md 第 3 行：元行缺 id" } });
    expect(dumped(root, ws)).toEqual(DOC);
    const again = dumped(root, ws) as { slices: unknown[] };
    expect(again.slices).toEqual([{ name: "alpha", domain: "软件", error: "图.md 第 3 行：元行缺 id" }, DOC.slices[1]]);
  });

  it("continue / pause 改 auto；repo 换此刻那一片；take 接过此刻那一片、说一句", () => {
    const { root, ws } = home();
    expect(pb(root, ws, "continue").rc).toBe(0);
    expect(dumped(root, ws).auto).toBe(true);
    pb(root, ws, "pause");
    expect(dumped(root, ws).auto).toBe(false);
    const t = pb(root, ws, "take");
    expect(t.rc).toBe(0);
    expect(t.out.split("\n")[0]).toContain("repos/alpha");
    expect((dumped(root, ws).slices as { docs_only?: boolean }[])[0].docs_only).toBe(false);
    pb(root, ws, "repo", "beta");
    expect(dumped(root, ws).repo).toBe("beta");
  });

  it("view 说写了哪一页", () => {
    const { root, ws } = home();
    const v = pb(root, ws, "view");
    expect(v.rc).toBe(0);
    expect(v.out).toMatch(/^写了 \/tmp\/\S+\.html\n/);
  });

  it("给了拒的那几条：退 2、stderr 是 pb 那一句；别的照常", () => {
    const { root, ws } = home({ refuse: { continue: "这个工作区还没有顶块" } });
    const r = pb(root, ws, "continue");
    expect([r.rc, r.err]).toEqual([2, "这个工作区还没有顶块\n"]);
    expect(dumped(root, ws).auto).toBe(false);
    expect(pb(root, ws, "pause").rc).toBe(0);
  });

  it("认不得的动词退 4", () => {
    const { root, ws } = home();
    expect(pb(root, ws, "nope").rc).toBe(4);
  });

  it("工作区里 dump 那一份就是 putWorkspace 写的（相对家目录）", () => {
    const { ws } = home();
    expect(JSON.parse(readFileSync(path.join(ws, ".planned-build", "dump.json"), "utf8"))).toEqual(DOC);
  });
});
