// 〔RM1c · 第四波〕全景的通道封装（`src/frontend/ui/panorama/api.ts`）。〔MIG-3b 续 · 主会话 09-28 裁〕界面直问那台后端。
//
// 🔴 **被判的那块没被 mock**：只 mock 了 IPC 出口（`invoke`），`api.ts` 与 `ipc/chan.ts` 本体都是真的。
// 判法：
//  ① 每个**读**入口经通道恰发一次 `panorama`（发给那台机器），用到的 op 集合 ＋ 写那一侧的「算」op ＋ 刷文档关联 ==
//     小程序自报的 op 表（〔PANO〕后端不再存；读生成物 `engine-contract.json`，它 == 小程序 `OPS` 由小程序判据钉。
//     两向相等，**异源**：一侧真调一遍录下来，一侧读生成物）；每问都带上生成物里的 `shape`；
//  ② 〔RM1d · V110〕六个**写**入口本机远端同一条：经通道恰发一次 `panorama-edit`，origin / 仓原样，
//     op 集合 == 后端 `control/panorama_edit.rs::EDITS` 第一列（两向，异源）；
//  ③ 本机仓的**读**与远端同一条（origin 是 `<local>`，op 逐个相同）；
//  ④ 〔RM1e → MIG-3b 续〕没装 / 太旧 ⇒ **恰放一次字节、恰再问一次**；其余失败零放；放了仍说 ⇒ 如实报、不循环（原 monitor 那一跳的「问 · 放 · 再问」）；
//  ⑤ 期限归发起方：每个 op 按生成物里的档，长于后端给那一档的期限（加那一次探测）；
//     「缺 / 旧 ⇒ 放」的码 == 后端适配层映射出来的那两个（读 Rust 源码两向）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import * as api from "../../../../src/frontend/ui/panorama/api";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import { ChanError } from "../../../../src/comms/inward/chan";
import { chanArgsJson, chanReply, isChanCall, refusedReply, type ChanCallArgs } from "../../../test-support/chan-fake";

const REMOTE: api.RepoAt = { origin: "box1", path: "/srv/proj" };
const LOCAL: api.RepoAt = { origin: LOCAL_ORIGIN, path: "/home/me/proj" };

/** 每个入口怎么调（参数随便给，只看它发了什么）。键 = `api.ts` 导出的函数名。 */
const READS: Record<string, (at: api.RepoAt) => Promise<unknown>> = {
  index: (at) => api.index(at),
  reindex: (at) => api.reindex(at),
  status: (at) => api.status(at),
  overview: (at) => api.overview(at, 2000),
  node: (at) => api.node(at, "a#f"),
  neighborhood: (at) => api.neighborhood(at, "a#f", 1),
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

const here = dirname(fileURLToPath(import.meta.url));
const rust = (rel: string): string => readFileSync(resolve(here, "../../../..", rel), "utf8");

/** 后端 `panorama_edit.rs::EDITS` 三列（读 Rust 源码，异源）。 */
function backendEdits(): { ops: string[]; plans: string[] } {
  const src = rust("src/backend/control/panorama_edit.rs");
  const at = src.indexOf("const EDITS: &[(&str, &str, bool)] = &[");
  expect(at, "后端的写表改了写法 —— 本条跟着改").toBeGreaterThan(0);
  const body = src.slice(at, src.indexOf("];", at));
  const rows = [...body.matchAll(/^\s*\("([a-z_]+)", "([a-z_]+)", (true|false)\)/gm)];
  return {
    ops: rows.map((m) => m[1]).sort(),
    plans: rows.map((m) => m[2]).sort(),
  };
}

/** 小程序自报的 op 表（生成物 `engine-contract.json`：op → 档；读文件，与 `api.ts` 的 import 不同路）。 */
function programOps(): { shape: string; ops: Record<string, string> } {
  const c = JSON.parse(rust("src/frontend/ui/panorama/engine-contract.json")) as { shape: string; ops: Record<string, string> };
  expect(Object.keys(c.ops).length, "生成物没读到 op").toBeGreaterThan(10);
  return c;
}

/** 后端适配层一个 `const NAME: u64 = <数>;` 的值。 */
function backendConst(name: string): number {
  const m = new RegExp(`const ${name}: u64 = (\\d+);`).exec(rust("src/backend/control/panorama.rs"));
  expect(m, `后端里找不到 ${name}`).not.toBeNull();
  return Number(m![1]);
}

type Sent = { origin: string; op: string; body: Record<string, unknown>; leftMs: number; callId: string | null };
const sent = (frame: string): Sent[] =>
  vi
    .mocked(invoke)
    .mock.calls.filter((c) => isChanCall(String(c[0]), c[1], frame))
    .map((c) => {
      const a = c[1] as unknown as ChanCallArgs;
      return { origin: a.origin, op: a.op, body: chanArgsJson(a) as Record<string, unknown>, leftMs: a.leftMs, callId: a.callId ?? null };
    });

describe("全景经通道直问那台后端（RM1c · MIG-3b 续）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation((async (cmd: string) => (cmd === "chan_call" ? chanReply({ result: null }) : null)) as never);
  });

  it("A0 自证：api.ts 是真的，被 mock 的只有 invoke", () => {
    expect(vi.isMockFunction(api.status)).toBe(false);
    expect(vi.isMockFunction(invoke)).toBe(true);
    const exported = Object.entries(api)
      .filter(([, v]) => typeof v === "function")
      .map(([k]) => k)
      .filter(
        (k) =>
          !["panoramaLoadDecision", "repoLabel", "sameRepo", "PanoramaCancelled", "askOrPlace", "wantsBytes", "budgetFor"].includes(k),
      )
      .sort();
    expect(exported).toEqual([...Object.keys(READS), ...Object.keys(WRITES)].sort());
  });

  it("A1 远端读：每个入口经通道恰发一次 panorama，op 集合 ＋ 写那一侧的算 op ＋ 刷文档关联 == 后端 OPS 表（两向）", async () => {
    const ops: string[] = [];
    for (const [name, call] of Object.entries(READS)) {
      vi.mocked(invoke).mockClear();
      await call(REMOTE);
      const s = sent("panorama");
      expect(s.length, name).toBe(1);
      expect(s[0].origin, name).toBe("box1");
      expect(s[0].body.repo, name).toBe(name === "diagramKinds" ? null : "/srv/proj");
      expect(s[0].body.shape, name).toBe(programOps().shape);
      ops.push(String(s[0].body.op));
    }
    const all = Object.keys(programOps().ops).sort();
    expect([...ops, ...backendEdits().plans, "refresh_doc_links"].sort()).toEqual(all);
  });

  it("A2 写：本机远端同一条 —— 经通道恰发一次 panorama-edit，op 集合 == 后端 EDITS 第一列；要刷的那几种 == 第三列（两向）", async () => {
    vi.mocked(invoke).mockImplementation((async (cmd: string) => (cmd === "chan_call" ? chanReply("id1") : null)) as never);
    for (const at of [REMOTE, LOCAL]) {
      const ops: string[] = [];
      for (const [name, call] of Object.entries(WRITES)) {
        vi.mocked(invoke).mockClear();
        await call(at);
        const s = sent("panorama-edit");
        expect(s.length, name).toBe(1);
        expect([s[0].origin, s[0].body.repo, s[0].body.shape], name).toEqual([at.origin, at.path, programOps().shape]);
        ops.push(String(s[0].body.op));
      }
      expect(ops.sort()).toEqual(backendEdits().ops);
    }
    vi.mocked(invoke).mockClear();
    await api.addAnnotation(REMOTE, "a.rs", null, "x", "me");
    expect(sent("panorama-edit")[0].body.args).toEqual({ file: "a.rs", symbol: null, body: "x", author: "me" });
  });

  it("A3 本机读：与远端同一条 —— origin = <local>，op 与远端逐个相同", async () => {
    const opsAt = async (at: api.RepoAt): Promise<string[]> => {
      const ops: string[] = [];
      for (const call of Object.values(READS)) {
        vi.mocked(invoke).mockClear();
        await call(at);
        const s = sent("panorama");
        expect(s[0].origin).toBe(at.origin);
        ops.push(String(s[0].body.op));
      }
      return ops;
    };
    const local = await opsAt(LOCAL);
    expect(local.length).toBe(Object.keys(READS).length);
    expect(local).toEqual(await opsAt(REMOTE));
    // 旧的 Tauri 命令一条都不再发。
    const cmds = vi.mocked(invoke).mock.calls.map((c) => String(c[0]));
    expect(cmds.filter((c) => c.startsWith("panorama_"))).toEqual([]);
  });

  it("A4 载荷原样带参数（op 自己的参数进 args，不摊开）；期限按档给", async () => {
    await api.overview(REMOTE, 1234);
    await api.index(REMOTE);
    const [ov, ix] = sent("panorama");
    expect(ov.body).toEqual({ op: "overview", repo: "/srv/proj", args: { budget: 1234 }, shape: programOps().shape });
    expect(ov.leftMs).toBeLessThanOrEqual(api.QUERY_BUDGET_MS);
    expect(ix.leftMs).toBeGreaterThan(api.QUERY_BUDGET_MS);
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

/** 对端「不行」、带码的那一个失败（与 monitor 交回的线上形状同一份：`refusedReply`）。 */
function coded(code: string): ChanError {
  const r = refusedReply(code, `对端说 ${code}`);
  return new ChanError({ layer: "peer", why: "refused", body: Uint8Array.from(r.body) });
}
const ok = (v: unknown): Uint8Array => new Uint8Array(chanReply(v));

describe("〔RM1e → MIG-3b 续〕没装 / 太旧 ⇒ 放字节再问一次（askOrPlace）", () => {
  it("★ 缺 / 旧 ⇒ 恰放一次、恰再问一次、交回第二问的结果；其余失败 ⇒ 零放、原话带回", async () => {
    // 期望取自题面（「远端 `panorama` 回 `not_installed`（或 `unsupported` = 旧版）时」），不取自 `PUSH_ON`（同源恒真）。
    for (const code of ["not_installed", "unsupported"]) {
      const asks = [Promise.reject(coded(code)), Promise.resolve(ok({ ok: 1 }))];
      let n = 0;
      let placed = 0;
      const got = await api.askOrPlace(
        () => asks[n++],
        async () => {
          placed += 1;
        },
      );
      expect(got, code).toEqual({ ok: 1 });
      expect([placed, n], code).toEqual([1, 2]);
    }
    for (const first of [
      coded("failed"),
      coded("bad_args"),
      coded("timed_out"),
      new ChanError({ layer: "peer", why: "unsupported" }),
      new ChanError({ layer: "hop", at: { idx: 1, tag: "open" }, reach: "NotSent", why: "Unreachable" }),
    ]) {
      let n = 0;
      let placed = 0;
      await expect(
        api.askOrPlace(
          () => {
            n += 1;
            return Promise.reject(first);
          },
          async () => {
            placed += 1;
          },
        ),
      ).rejects.toThrow();
      expect([placed, n], JSON.stringify(first.error)).toEqual([0, 1]);
    }
  });

  it("★ 放了仍说缺 / 旧 ⇒ 如实报、不循环（放 1 次、问 2 次）；放失败 ⇒ 原话 ＋ 放失败那句都在、不再问", async () => {
    let n = 0;
    let placed = 0;
    await expect(
      api.askOrPlace(
        () => {
          n += 1;
          return Promise.reject(coded("unsupported"));
        },
        async () => {
          placed += 1;
        },
      ),
    ).rejects.toThrow(/已经把这一版/);
    expect([placed, n]).toEqual([1, 2]);

    n = 0;
    const e = await api
      .askOrPlace(
        () => {
          n += 1;
          return Promise.reject(coded("not_installed"));
        },
        () => Promise.reject(new Error("围栏拒了")),
      )
      .catch((x: Error) => x.message);
    expect(n, "放没成就不再问").toBe(1);
    expect(e).toContain("not_installed");
    expect(e).toContain("围栏拒了");
  });

  it("撤了（cancel 拨下）⇒ PanoramaCancelled，不放", async () => {
    const ac = new AbortController();
    ac.abort();
    let placed = 0;
    await expect(
      api.askOrPlace(
        () => Promise.reject(new ChanError({ layer: "ours", why: "Cancelled" })),
        async () => {
          placed += 1;
        },
        ac.signal,
      ),
    ).rejects.toBeInstanceOf(api.PanoramaCancelled);
    expect(placed).toBe(0);
  });

  it("⑤ 期限归发起方：每个 op 按生成物里的档，长于后端给那一档的期限（加探测）；写一律按最坏一形；「缺 / 旧 ⇒ 放」的码 == 后端映射的那两个", () => {
    const probe = backendConst("PROBE_DEADLINE_SECS");
    const secs: Record<string, number> = { long: backendConst("BUILD_DEADLINE_SECS"), quick: backendConst("QUERY_DEADLINE_SECS") };
    const ops = Object.entries(programOps().ops);
    expect(new Set(ops.map(([, t]) => t))).toEqual(new Set(["long", "quick"])); // 两档都在（否则下面恒真）
    const short = ops.filter(([op, t]) => !(api.budgetFor(op) / 1000 > probe + secs[t])).map(([op]) => op);
    expect(short).toEqual([]);
    expect(api.EDIT_BUDGET_MS / 1000).toBeGreaterThan(3 * (probe + secs.quick) + probe + secs.long);
    // 后端适配层 `discover::find(…)` 与 `probe::negotiate(…)` 那两条语句映射出来的码（读 Rust 源码）。
    const src = rust("src/backend/control/panorama.rs");
    const stmt = (start: string, end: string): string => {
      const at = src.indexOf(start);
      expect(at, start).toBeGreaterThan(0);
      return src.slice(at, src.indexOf(end, at));
    };
    const codes = new Set(
      [stmt("plugin::discover::find(", ";"), stmt("plugin::probe::negotiate(", ")?;")].flatMap((s) =>
        [...s.matchAll(/\("([a-z_]+)"/g)].map((m) => m[1]),
      ),
    );
    expect([...api.PUSH_ON].sort()).toEqual([...codes].sort());
  });
});
