/**
 * `src/frontend/ui/history-reads.ts` 的判据：历史清单与注解经通道问**本机常驻后端**，按形状收。
 *
 * 守的要求：「本机后端经 `remote_ask` 问远端那台的
 * 项目 / 会话清单、并上注解、出成品；前端经 `chan.call`」—— 以及用户那一条「用户的历史注解一条不许丢」。
 *
 * 判据：
 * 1. **跨语言金样**：后端对结构占位输入出的三份成品（`tests/__fixtures__/history-products.golden.json`，后端
 *    `history_join_tests.rs::the_products_match_the_cross_language_golden` 产出并对拍）—— 这里的解码器读同一份，逐行逐格收下（异源）。
 * 2. **严格收**：多一格 / 缺一格 / 类型不对 / fork 两格只给一格 / 老后端的 `{lines}` 形状 ⇒ 抛（不猜、不补默认值）。
 * 3. **问的是谁、带了什么**：一律问 `<local>`；远端那一批逐台带 `origin`；会话那一问带 `project_dir`（远端再带 `origin`）；
 *    改注解带 `{sid, patch}`；每一发都显式给期限。
 * 4. **fan-out 的失败语义**（原样搬自 monitor 那一份）：没配远端 ⇒ 空、零发；逐台失败 ⇒ 进 `failedHosts`、其余照收；全部失败 ⇒ 抛。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  annotate,
  decodeEntry,
  decodeLastAccounts,
  decodeProjects,
  decodeSessions,
  fetchLocalProjects,
  fetchRemoteProjects,
  fetchSessions,
  HistoryShapeError,
  lastAccounts,
} from "../../../src/frontend/ui/history-reads";
import {
  chanArgsJson,
  chanReply,
  refusedReply,
  type ChanCallArgs,
} from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(
    join(__dirname, "../../__fixtures__", "history-products.golden.json"),
    "utf8",
  ),
) as Record<string, { rows: Record<string, unknown>[]; notice: string | null }>;

const chanCalls = (): ChanCallArgs[] =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "chan_call")
    .map((c) => c[1] as ChanCallArgs);

beforeEach(() => {
  invokeMock.mockReset();
});

describe("跨语言金样（后端出、这里收）", () => {
  it("远端项目 · 远端会话 · 合成会话三份成品逐行逐格收下", () => {
    const p = decodeProjects(golden.remoteProjects);
    expect(p.projects).toEqual(golden.remoteProjects.rows);
    expect(p.notice).toBeNull();
    // 「不知道」过线是 null，不是 0（K-R92）：金样里没带 sid 清单的那一行三个数全 null。
    expect(
      p.projects.find((r) => r.projectDir === "-w-beta")?.starredCount,
    ).toBeNull();
    const s = decodeSessions(golden.remoteSessions);
    expect(s.sessions).toEqual(golden.remoteSessions.rows);
    expect(s.sessions[0].forkedFromSessionId).toBeTypeOf("string");
    expect(s.sessions[1].forkedFromSessionId).toBeUndefined();
    const c = decodeSessions(golden.synthSessions);
    expect(c.sessions).toEqual(golden.synthSessions.rows);
    expect(c.sessions[0].origin, "本机那一行不带 origin").toBeUndefined();
  });
});

describe("严格收：形状不对就抛，不猜", () => {
  const row = (): Record<string, unknown> => ({
    ...golden.remoteProjects.rows[0],
  });
  const sess = (): Record<string, unknown> => ({
    ...golden.remoteSessions.rows[0],
  });
  it.each([
    ["老后端的按行形状", { lines: [] }],
    ["外壳多一格", { rows: [], notice: null, extra: 1 }],
    ["外壳缺 notice", { rows: [] }],
    ["一行多一格", { rows: [{ ...row(), bogus: 1 }], notice: null }],
    [
      "一行缺一格",
      { rows: [(({ hasLive: _h, ...r }) => r)(row())], notice: null },
    ],
    ["星标数是串", { rows: [{ ...row(), starredCount: "1" }], notice: null }],
  ])("项目：%s", (_why, v) => {
    expect(() => decodeProjects(v)).toThrow(HistoryShapeError);
  });
  it.each([
    [
      "fork 两格只给一格",
      {
        rows: [(({ forkedFromMessageUuid: _m, ...r }) => r)(sess())],
        notice: null,
      },
    ],
    ["判活是串", { rows: [{ ...sess(), isLive: "no" }], notice: null }],
    ["多一格", { rows: [{ ...sess(), extra: true }], notice: null }],
  ])("会话：%s", (_why, v) => {
    expect(() => decodeSessions(v)).toThrow(HistoryShapeError);
  });
  it("注解那一条与上次账号表同样严格", () => {
    const e = {
      starred: true,
      customTitle: null,
      hidden: false,
      updatedAt: 1,
      lastAccount: "a",
    };
    expect(decodeEntry({ entry: e })).toEqual(e);
    expect(() => decodeEntry({ entry: { ...e, extra: 1 } })).toThrow(
      HistoryShapeError,
    );
    expect(() => decodeEntry(e)).toThrow(HistoryShapeError);
    expect(decodeLastAccounts({ accounts: { s: "a" } })).toEqual({ s: "a" });
    expect(() => decodeLastAccounts({ accounts: { s: 1 } })).toThrow(
      HistoryShapeError,
    );
  });
});

describe("问的是谁、带了什么", () => {
  it("本机项目 / 会话 / 改注解 / 上次账号：一律问 <local>，请求体逐键、显式给期限", async () => {
    invokeMock.mockImplementation((_cmd: string, a: ChanCallArgs) => {
      switch (a.op) {
        case "history-projects":
          return Promise.resolve(chanReply({ rows: [], notice: null }));
        case "history-sessions":
          return Promise.resolve(chanReply(golden.synthSessions));
        case "history-annotate":
          return Promise.resolve(
            chanReply({
              entry: {
                starred: true,
                customTitle: null,
                hidden: false,
                updatedAt: 9,
                lastAccount: null,
              },
            }),
          );
        case "history-last-accounts":
          return Promise.resolve(chanReply({ accounts: {} }));
      }
      return Promise.resolve(undefined);
    });
    await fetchLocalProjects();
    await fetchSessions({ projectDir: "-w-alpha" });
    await fetchSessions({ projectDir: "-w-alpha", origin: "dev" });
    await annotate("s1", { customTitle: "" });
    await lastAccounts();
    const got = chanCalls().map((a) => [a.origin, a.op, chanArgsJson(a)]);
    expect(got).toEqual([
      ["<local>", "history-projects", {}],
      ["<local>", "history-sessions", { project_dir: "-w-alpha" }],
      [
        "<local>",
        "history-sessions",
        { project_dir: "-w-alpha", origin: "dev" },
      ],
      [
        "<local>",
        "history-annotate",
        { sid: "s1", patch: { customTitle: "" } },
      ],
      ["<local>", "history-last-accounts", {}],
    ]);
    for (const a of chanCalls())
      expect(a.leftMs, `${a.op} 没给期限`).toBeGreaterThan(0);
  });
});

describe("远端那一批的 fan-out（失败语义原样搬自 monitor 那一份）", () => {
  const byOrigin = (fail: string[]) => (cmd: string, a: ChanCallArgs) => {
    if (cmd === "list_remote_mcp_origins")
      return Promise.resolve(["dev", "box"]);
    const o = (chanArgsJson(a) as { origin: string }).origin;
    if (fail.includes(o))
      return Promise.reject(refusedReply("unreachable", `[${o}] 问不到`));
    return Promise.resolve(
      chanReply({
        rows: golden.remoteProjects.rows.map((r) => ({ ...r, origin: o })),
        notice: null,
      }),
    );
  };
  it("没配远端 ⇒ 空、一发 chan_call 都没有", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_remote_mcp_origins"
        ? Promise.resolve([])
        : Promise.resolve(undefined),
    );
    expect(await fetchRemoteProjects()).toEqual({
      projects: [],
      failedHosts: [],
      emptyHosts: [],
    });
    expect(chanCalls()).toHaveLength(0);
  });
  it("逐台问、逐台带 origin；一台失败 ⇒ 进 failedHosts，其余照收", async () => {
    invokeMock.mockImplementation(byOrigin(["box"]));
    const r = await fetchRemoteProjects();
    expect(
      chanCalls()
        .map((a) => (chanArgsJson(a) as { origin: string }).origin)
        .sort(),
    ).toEqual(["box", "dev"]);
    expect(r.failedHosts).toEqual(["box"]);
    expect(new Set(r.projects.map((p) => p.origin))).toEqual(new Set(["dev"]));
    expect(r.projects).toHaveLength(golden.remoteProjects.rows.length);
  });
  it("全部失败 ⇒ 抛（与「没配远端」分开）", async () => {
    invokeMock.mockImplementation(byOrigin(["dev", "box"]));
    await expect(fetchRemoteProjects()).rejects.toThrow(
      "2 台远端的历史都没拿到",
    );
  });
});
