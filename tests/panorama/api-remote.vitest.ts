// 〔RM1c · 第四波〕全景的 IPC 封装按机器分两条路（`src/panorama/api.ts`）。
//
// 🔴 **被判的那块没被 mock**：只 mock 了 IPC 出口（`invoke`），`api.ts` 本体是真的。
// 判法：
//  ① 远端仓上，每个**读**入口发的恰是 `panorama_call`，而它们用到的 op 集合 == 后端适配层
//     `control/panorama.rs` 那张 `OPS` 表（两向相等，**异源**：一侧是真调一遍 `api.ts` 录下来的，
//     一侧是读 Rust 源码抽出来的）；
//  ② 〔RM1d · V110〕六个**写**入口本机远端同一条：恰发一次 `panorama_edit`，origin / 仓原样，
//     op 集合 == monitor `panorama_call.rs::EDITS` 第一列（两向，异源：读 Rust 源码）；
//  ③ 本机仓的**读**照旧走进程内那几条命令（`panorama_call` 一次都不发）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import * as api from "../../src/panorama/api";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";

const REMOTE: api.RepoAt = { origin: "box1", path: "/srv/proj" };
const LOCAL: api.RepoAt = { origin: LOCAL_ORIGIN, path: "/home/me/proj" };

/** 每个入口怎么调（参数随便给，只看它发了什么）。键 = `api.ts` 导出的函数名。 */
const READS: Record<string, (at: api.RepoAt) => Promise<unknown>> = {
  index: (at) => api.index(at),
  reindex: (at) => api.reindex(at),
  status: (at) => api.status(at),
  overview: (at) => api.overview(at, 2000),
  node: (at) => api.node(at, "a#f"),
  subgraph: (at) => api.subgraph(at, "a#f", 1),
  callers: (at) => api.callers(at, "a#f", 1),
  callees: (at) => api.callees(at, "a#f", 1),
  impact: (at) => api.impact(at, "a#f"),
  search: (at) => api.search(at, "f", 5),
  docsFor: (at) => api.docsFor(at, "a#f"),
  diagramKinds: (at) => api.diagramKinds(at.origin),
  diagram: (at) => api.diagram(at, "arch", {}),
  symbolsInFile: (at) => api.symbolsInFile(at, "a.rs"),
  drift: (at) => api.drift(at),
  listAnnotations: (at) => api.listAnnotations(at),
  touching: (at) => api.touching(at, ["/srv/proj/a.rs"], []),
};
const WRITES: Record<string, (at: api.RepoAt) => Promise<unknown>> = {
  addAnnotation: (at) => api.addAnnotation(at, "a.rs", "f", "x", "me"),
  proposeAnnotation: (at) => api.proposeAnnotation(at, "a.rs", "f", "x", "agent"),
  approveAnnotation: (at) => api.approveAnnotation(at, "id1"),
  removeAnnotation: (at) => api.removeAnnotation(at, "id1"),
  writeDocLink: (at) => api.writeDocLink(at, "d.md", "a#f"),
  removeDocLink: (at) => api.removeDocLink(at, "d.md", "a#f"),
};

/** monitor `panorama_call.rs::EDITS` 第一列与第二列（读 Rust 源码，异源）。 */
function monitorEdits(): { ops: string[]; plans: string[] } {
  const here = dirname(fileURLToPath(import.meta.url));
  const src = readFileSync(resolve(here, "../../src/bridge/src/panorama_call.rs"), "utf8");
  const at = src.indexOf("const EDITS: &[(&str, &str, bool)] = &[");
  expect(at, "monitor 的写表改了写法 —— 本条跟着改").toBeGreaterThan(0);
  const body = src.slice(at, src.indexOf("];", at));
  const rows = [...body.matchAll(/^\s*\("([a-z_]+)", "([a-z_]+)",/gm)];
  return { ops: rows.map((m) => m[1]).sort(), plans: rows.map((m) => m[2]).sort() };
}

/** 后端适配层 `OPS` 表里的 op 名（读 Rust 源码，异源）。 */
function backendOps(): string[] {
  const here = dirname(fileURLToPath(import.meta.url));
  const src = readFileSync(resolve(here, "../../src/backend/control/panorama.rs"), "utf8");
  const at = src.indexOf("const OPS: &[(&str, u64)] = &[");
  expect(at, "后端适配层的 op 表改了写法 —— 本条跟着改").toBeGreaterThan(0);
  const body = src.slice(at, src.indexOf("];", at));
  return [...body.matchAll(/^\s*\("([a-z_]+)",/gm)].map((m) => m[1]).sort();
}

type Sent = { cmd: string; args: Record<string, unknown> };
const sent = (): Sent[] =>
  vi.mocked(invoke).mock.calls.map((c) => ({ cmd: c[0] as string, args: (c[1] ?? {}) as Record<string, unknown> }));

describe("全景 IPC 按机器分路（RM1c）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockResolvedValue(null);
  });

  it("A0 自证：api.ts 是真的，被 mock 的只有 invoke", () => {
    expect(vi.isMockFunction(api.status)).toBe(false);
    expect(vi.isMockFunction(invoke)).toBe(true);
    // 本表覆盖 api.ts 的全部入口（漏一个就是漏判一条路）。
    const exported = Object.entries(api)
      .filter(([, v]) => typeof v === "function")
      .map(([k]) => k)
      // 〔RM1f〕`cancellable`（判定）与 `PanoramaCancelled`（错误类）不是发 IPC 的入口。
      .filter((k) => !["panoramaLoadDecision", "repoLabel", "sameRepo", "cancellable", "PanoramaCancelled"].includes(k))
      .sort();
    expect(exported).toEqual([...Object.keys(READS), ...Object.keys(WRITES)].sort());
  });

  it("A1 远端读：每个入口恰发一次 panorama_call，op 集合 == 后端 OPS 表（两向）", async () => {
    const ops: string[] = [];
    for (const [name, call] of Object.entries(READS)) {
      vi.mocked(invoke).mockClear();
      await call(REMOTE);
      const s = sent();
      expect(s.map((x) => x.cmd), name).toEqual(["panorama_call"]);
      expect(s[0].args.origin, name).toBe("box1");
      // 要仓的 op 带那台机器上的路径；图种注册表不要仓。
      expect(s[0].args.repo, name).toBe(name === "diagramKinds" ? null : "/srv/proj");
      ops.push(String(s[0].args.op));
    }
    // 〔RM1d〕后端 OPS 余下的那几个（「算」＋ 刷文档关联）不由读入口发，由 monitor 的写那条发：
    // 集合 == monitor `EDITS` 第二列 ＋ `refresh_doc_links`（两向，拼起来 == 后端全表）。
    expect([...ops, ...monitorEdits().plans, "refresh_doc_links"].sort()).toEqual(backendOps());
  });

  it("A2 写：本机远端同一条 —— 恰发一次 panorama_edit，op 集合 == monitor EDITS（两向）", async () => {
    for (const at of [REMOTE, LOCAL]) {
      const ops: string[] = [];
      for (const [name, call] of Object.entries(WRITES)) {
        vi.mocked(invoke).mockClear();
        await call(at);
        const s = sent();
        expect(s.map((x) => x.cmd), name).toEqual(["panorama_edit"]);
        expect([s[0].args.origin, s[0].args.repo], name).toEqual([at.origin, at.path]);
        ops.push(String(s[0].args.op));
      }
      expect(ops.sort()).toEqual(monitorEdits().ops);
    }
    vi.mocked(invoke).mockClear();
    await api.addAnnotation(REMOTE, "a.rs", null, "x", "me");
    expect(sent()[0].args.args).toEqual({ file: "a.rs", symbol: null, body: "x", author: "me" });
  });

  it("A3 本机读：照旧走进程内那几条命令，panorama_call 一次都不发", async () => {
    for (const call of Object.values(READS)) await call(LOCAL);
    const cmds = sent().map((s) => s.cmd);
    expect(cmds).not.toContain("panorama_call");
    expect(cmds.length).toBe(Object.keys(READS).length);
    expect(sent().find((s) => s.cmd === "panorama_overview")?.args).toEqual({ repo: "/home/me/proj", budget: 2000 });
  });

  it("A4 远端载荷原样带参数（op 自己的参数进 args，不摊开）", async () => {
    await api.overview(REMOTE, 1234);
    await api.search(REMOTE, "q");
    const s = sent();
    // 〔RM1f〕没给撤单手柄的一问不带票（`ticket: null`）。
    expect(s[0].args).toEqual({ origin: "box1", op: "overview", repo: "/srv/proj", args: { budget: 1234 }, ticket: null });
    expect(s[1].args).toEqual({ origin: "box1", op: "search", repo: "/srv/proj", args: { query: "q" }, ticket: null });
  });

  it("A5 住址带机器：本机就是路径，远端带上机器名；同一个路径换一台机器就是另一个仓", () => {
    expect(api.repoLabel(LOCAL)).toBe("/home/me/proj");
    expect(api.repoLabel(REMOTE)).toBe("/srv/proj（远端 box1）");
    expect(api.sameRepo({ origin: "box1", path: "/p" }, { origin: "box1", path: "/p" })).toBe(true);
    expect(api.sameRepo({ origin: "box1", path: "/p" }, { origin: LOCAL_ORIGIN, path: "/p" })).toBe(false);
    expect(api.sameRepo(null, null)).toBe(true);
    expect(api.sameRepo(null, LOCAL)).toBe(false);
  });
});
