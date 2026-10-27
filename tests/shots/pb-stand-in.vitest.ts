/**
 * 截图台架边界上的 pb 替身（`disk/plan.ts`）：真后端照常找 pb 插件、起 `python3 <入口> <动词>`，碰到的是它。
 * 它只把工作区里那一份 dump（pb 0.5 · 形状 6）原样吐出来、照几条命令改那一份，演 pb 的几种退出码；成品（数 · 排 · 短词）全由真后端算。
 *
 * 买不到：真 pb 读 `图.md` 那一段不在这里；dump 的形状对不对由真后端读它（截图台架）判。
 */
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { blankDisk } from "./disk";
import { dumpNodes, PB_ENTRY, putPb, putWorkspace, type PbStandIn } from "./disk/plan";

const dirs: string[] = [];
afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

const DOC = {
  pb: "0.5.2",
  shape: 6,
  workspace: "/w",
  current_repo: "alpha",
  auto_continue: false,
  domains: [],
  repos: [
    { name: "alpha", domain: "软件", plan_only: true, path: "repos/alpha", nodes: [] },
    { name: "beta", domain: "研究", plan_only: false, path: "beta", nodes: [] },
  ],
};

/** 照 `putPb` · `putWorkspace` 落一个家目录，回它与工作区根。 */
function homeWith(doc: unknown, cfg: PbStandIn = {}, ws = "work/alpha"): { root: string; ws: string } {
  const root = mkdtempSync(path.join(os.tmpdir(), "shots-pb-"));
  dirs.push(root);
  const d = blankDisk();
  putPb(d, cfg);
  putWorkspace(d, ws, doc);
  for (const [rel, text] of Object.entries(d.files)) {
    mkdirSync(path.dirname(path.join(root, rel)), { recursive: true });
    writeFileSync(path.join(root, rel), text);
  }
  mkdirSync(path.join(root, ws, "sub"), { recursive: true });
  return { root, ws: path.join(root, ws) };
}
const home = (cfg: PbStandIn = {}): { root: string; ws: string } => homeWith(DOC, cfg);

function pb(root: string, cwd: string, ...args: string[]): { rc: number | null; out: string; err: string } {
  const r = spawnSync("python3", [path.join(root, PB_ENTRY), ...args], { cwd, encoding: "utf8", env: { PATH: "/usr/bin:/bin" } });
  return { rc: r.status, out: r.stdout, err: r.stderr };
}

const dumped = (root: string, cwd: string): Record<string, unknown> => JSON.parse(pb(root, cwd, "dump").out) as Record<string, unknown>;

// 替身是起 `python3` 的一段脚本、PATH 照 Unix 给：截图台架只在 Linux 上跑，Windows 那一格让开。
describe.skipIf(process.platform === "win32")("pb 替身", () => {
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

  it("头一次之后那个仓库读不成：第二次起它只剩名字 · 领域 · 位置 · 原因", () => {
    const { root, ws } = home({ brokenAfterFirst: { repo: "alpha", said: "图.md 第 3 行：元数据缺 id" } });
    expect(dumped(root, ws)).toEqual(DOC);
    const again = dumped(root, ws) as { repos: unknown[] };
    expect(again.repos).toEqual([{ name: "alpha", domain: "软件", path: "repos/alpha", plan_only: true, error: "图.md 第 3 行：元数据缺 id" }, DOC.repos[1]]);
  });

  it("continue / pause 改 auto_continue；repo 换当前仓库；adopt 接过当前仓库、说一句", () => {
    const { root, ws } = home();
    expect(pb(root, ws, "continue").rc).toBe(0);
    expect(dumped(root, ws).auto_continue).toBe(true);
    pb(root, ws, "pause");
    expect(dumped(root, ws).auto_continue).toBe(false);
    const t = pb(root, ws, "adopt");
    expect(t.rc).toBe(0);
    expect(t.out.split("\n")[0]).toContain("repos/alpha");
    expect((dumped(root, ws).repos as { plan_only?: boolean }[])[0].plan_only).toBe(false);
    pb(root, ws, "repo", "beta");
    expect(dumped(root, ws).current_repo).toBe("beta");
  });

  it("dump --domains <目录>：往上找到工作区 ⇒ 它那几档；找不到（目录还不在也行）⇒ 插件那几档、工作区 null，都退 0", () => {
    const plugin = [{ name: "软件", source: "plugin" }];
    const { root, ws } = home({ domains: plugin });
    const inWs = JSON.parse(pb(root, root, "dump", "--domains", path.join(ws, "sub", "deeper")).out) as Record<string, unknown>;
    expect([inWs.shape, inWs.workspace, inWs.domains]).toEqual([6, DOC.workspace, DOC.domains]);
    const off = pb(root, root, "dump", "--domains", path.join(root, "nowhere", "new"));
    expect(off.rc).toBe(0);
    const offDoc = JSON.parse(off.out) as Record<string, unknown>;
    expect([offDoc.shape, offDoc.workspace, offDoc.domains]).toEqual([6, null, plugin]);
  });

  it("render 说写了哪一页", () => {
    const { root, ws } = home();
    const v = pb(root, ws, "render");
    expect(v.rc).toBe(0);
    expect(v.out).toMatch(/^写了 \/tmp\/\S+\.html\n/);
  });

  it("0.5 没有的动词（take · view · record）退 4", () => {
    const { root, ws } = home();
    for (const v of ["take", "view", "record", "nope"]) expect(pb(root, ws, v).rc, v).toBe(4);
  });

  it("给了拒的那几条：退 2、stderr 是 pb 那一句；别的照常", () => {
    const { root, ws } = home({ refuse: { continue: "这个工作区还没有人认领 project" } });
    const r = pb(root, ws, "continue");
    expect([r.rc, r.err]).toEqual([2, "这个工作区还没有人认领 project\n"]);
    expect(dumped(root, ws).auto_continue).toBe(false);
    expect(pb(root, ws, "pause").rc).toBe(0);
  });

  it("工作区里 dump 那一份就是 putWorkspace 写的（相对家目录）", () => {
    const { ws } = home();
    expect(JSON.parse(readFileSync(path.join(ws, ".planned-build", "dump.json"), "utf8"))).toEqual(DOC);
  });
});

// pb 0.5：整份 dump 不带 agent 视角，点到一个节点再 `dump --node <仓库> <编号>` 取。
describe.skipIf(process.platform === "win32")("pb 替身 · dump --node", () => {
  const nodes = dumpNodes([{ id: "A1", title: "甲", kind: "能力", view: "── A1 甲" }], "架构", "lead");
  const VIEWED = { ...DOC, repos: [{ ...DOC.repos[0], nodes }, DOC.repos[1]] };

  it("dump 摘掉每个节点的 agent_view；dump --node 回那一个的那一段，图上没有 ⇒ 带 error", () => {
    const { root, ws } = homeWith(VIEWED);
    const all = dumped(root, ws) as { repos: { nodes?: Record<string, unknown>[] }[] };
    expect(all.repos[0].nodes?.[0].agent_view).toBeUndefined();
    expect(all.repos[0].nodes?.[0].id).toBe("A1");
    const one = JSON.parse(pb(root, ws, "dump", "--node", "alpha", "A1", ws).out) as Record<string, unknown>;
    expect([one.shape, one.repo, one.id, one.agent_view]).toEqual([6, "alpha", "A1", "── A1 甲"]);
    const gone = JSON.parse(pb(root, ws, "dump", "--node", "alpha", "Z9", ws).out) as Record<string, unknown>;
    expect(gone.agent_view).toBeUndefined();
    expect(gone.error).toBeTruthy();
  });

  it("dumpNodes 照 6 的键写一个节点：状态一值一项带笔数 · 文件带 file_check · 归谁三样", () => {
    const [n] = dumpNodes(
      [{ id: "A1", title: "甲", kind: "模块", state: [["已签收", 2]], files: [{ path: "a.txt", check: "empty", note: "0 字" }], sets: [{ target: "A2", value: "已签收" }] }],
      "架构",
      "lead",
    );
    expect(n.state).toEqual([{ value: "已签收", count: 2 }]);
    expect(n.files).toEqual([{ path: "a.txt", file_check: "empty", note: "0 字", chars: null, bytes: null }]);
    expect([n.children_in, n.assignee, n.signer, n.graph]).toEqual(["project", "lead", "lead", "架构"]);
    expect((n.edges as { sets: unknown[] }).sets).toEqual([{ op: "add", target: "A2", value: "已签收" }]);
  });
});
