/**
 * 〔C4c · 第四波 4B〕账号那两问（清单 `accounts-list` · 信任预检 `accounts-trust`）改走通道之后的判据。
 *
 * 要求住址：`设计/05 §8` 步 5「一次性请求那半收口成 `call` —— 按能力分批」· `设计/01 §5` D1「一个判定只有一个家」·
 * 用户裁决 V107「中转 ＋ 上游选择住本机常驻后端进程」（主会话据此裁：账号域读自己那台的 apikey 表、后端出成品）。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂**后端真出的**成品 —— 同一份跨语言金样，后端 `accounts_query_tests::the_account_products_match_the_cross_language_golden` 写它（异源：Rust 造、TS 解） | 「金样」那一条 |
 * | 形状不对 ⇒ 抛（多一格 / 缺一格 / 类型不对 / 老后端的 `{lines}` 形状），不替后端补值 | 「严格收」那一条 |
 * | 两问各自经通道说**对的帧命令、对的请求体**（`agent` 随请求带；账号 0 = `configDir: null`），本机也走同一条路 | 「请求」那两条 |
 * | 失败一律折成 `available:false` ＋ 一句人话，**不抛**；「够不着」≠「答了空表」（没有控制通道时不许渲染成「没有账号」） | 「失败」那两条 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧的判据在 Rust 里；monitor 那一跳由 `webview_tests` 量）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/config", () => ({ loadConfig: vi.fn().mockResolvedValue({}), patchConfig: vi.fn() })); // 〔CFG1〕写口换成按键补丁

import { invoke } from "@tauri-apps/api/core";
import { decodeAccountsList, decodeTrust } from "../../../src/frontend/ui/accounts-decode";
import { deriveUi } from "../../../src/frontend/ui/accounts";
import { __resetAccountsCacheForTest, checkTrust, fetchAccounts } from "../../../src/frontend/ui/account-reads";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { REPO_ROOT } from "../../test-support/repo-root";
import {
  chanArgsJson,
  chanReply,
  linesReply,
  NO_CHANNEL,
  UNSUPPORTED,
  type ChanCallArgs,
} from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/accounts.golden.json"), "utf8"),
) as Record<string, unknown>;

beforeEach(() => {
  invokeMock.mockReset();
  __resetAccountsCacheForTest();
});

/** 这一趟唯一那一发 `chan_call` 的 `(origin, op, 请求体)`。 */
function onlyChanCall(): { origin: string; op: string; body: unknown } {
  const calls = invokeMock.mock.calls;
  expect(calls.map((c) => c[0] as string)).toEqual(["chan_call"]);
  const a = calls[0][1] as ChanCallArgs;
  return { origin: a.origin, op: a.op, body: chanArgsJson(a) };
}

/** 「账号清单格式不对」那一句（按文案键取；`{what}` 那一截随用例变，只夹住前后两截）。 */
const esc = (t: string): string => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const LIST_BAD = new RegExp(copyText("accountsDecode.bad.list", { what: "\u0000" }).split("\u0000").map(esc).join("[^]*"));

describe("金样：后端出的成品，TS 这一侧读得懂", () => {
  it("accounts-list：三格照收，账号逐格照收（并过表的那一个是 api-key 且可选）", () => {
    const got = decodeAccountsList(golden["accounts-list"]);
    expect(got.meta.enabled).toBe(true);
    expect(got.meta.count).toBe(3);
    expect(got.meta.manifestPath).toBe("<root>/accts/accounts.json");
    expect(got.notice).toBeNull();
    expect(got.accounts.map((a) => [a.name, a.configDir, a.authKind, a.authReady])).toEqual([
      ["zero", null, "subscription", false],
      ["a", "<root>/accts/acct-a", "subscription", true],
      ["b", "<root>/accts/acct-b", "api-key", true],
    ]);
  });
  it("accounts-trust：两格照收", () => {
    expect(decodeTrust(golden["accounts-trust"])).toEqual({ trusted: true, known: true });
  });
});

describe("严格收：形状不对就抛，不补值", () => {
  const good = golden["accounts-list"] as Record<string, unknown>;
  const acct0 = (good.accounts as Record<string, unknown>[])[1];
  const cases: [string, unknown][] = [
    ["老后端那一形 `{lines}`", { lines: ["{}"] }],
    ["顶层多一格", { ...good, extra: 1 }],
    ["顶层缺 notice", { meta: good.meta, accounts: good.accounts }],
    ["meta 缺一格", { ...good, meta: { ...(good.meta as object), count: undefined } }],
    ["账号缺 authReady", { ...good, accounts: [{ ...acct0, authReady: undefined }] }],
    ["账号 authKind 认不出", { ...good, accounts: [{ ...acct0, authKind: "bedrock" }] }],
    ["账号多一格", { ...good, accounts: [{ ...acct0, token: "x" }] }],
    ["notice 类型不对", { ...good, notice: 3 }],
  ];
  for (const [why, v] of cases) {
    it(why, () => {
      // JSON 往返一次：`undefined` 那几格在线上就是「缺席」。
      expect(() => decodeAccountsList(JSON.parse(JSON.stringify(v)))).toThrow(LIST_BAD);
    });
  }
  it("信任预检：缺一格 / 多一格都抛", () => {
    expect(() => decodeTrust({ trusted: true })).toThrow(copyText("accountsDecode.bad.trust"));
    expect(() => decodeTrust({ trusted: true, known: false, error: null })).toThrow(copyText("accountsDecode.bad.trust"));
  });
});

describe("请求：两问经通道说对的帧命令、对的请求体", () => {
  it("fetchAccounts：`accounts-list` 带 `agent`（适配器 id，值从后端生成物来）；本机也走通道（`<local>`）", async () => {
    invokeMock.mockResolvedValue(chanReply(golden["accounts-list"]));
    const st = await fetchAccounts(LOCAL_ORIGIN);
    const { origin, op, body } = onlyChanCall();
    expect([origin, op]).toEqual(["<local>", "accounts-list"]);
    expect(body).toEqual({ agent: "claude-code" });
    expect(st.available).toBe(true);
    expect(st.accounts).toHaveLength(3);
    expect(deriveUi(st).kind).toBe("ready");
  });
  it("checkTrust：`accounts-trust`，账号 0 是 `configDir: null`（绝不传空串）", async () => {
    invokeMock.mockResolvedValue(chanReply({ trusted: false, known: true }));
    const r = await checkTrust("devbox", null, "/w/p");
    const { origin, op, body } = onlyChanCall();
    expect([origin, op]).toEqual(["devbox", "accounts-trust"]);
    expect(body).toEqual({ configDir: null, cwd: "/w/p" });
    expect(r).toEqual({ available: true, error: null, trusted: false, known: true });
  });
});

describe("失败：折成 available:false ＋ 一句人话，不抛", () => {
  it("没有控制通道 ⇒ 不可用（说「够不着」），**不是**「答了一张空表」", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    const st = await fetchAccounts("devbox");
    expect(st.available).toBe(false);
    expect(st.accounts).toEqual([]);
    expect(st.error).toMatch(/够不着/);
    // 〔WF2 · WIN3 读数 C〕够不着 ≠ 要更新。
    expect([st.oldBackend, deriveUi(st).kind]).toEqual([false, "query-failed"]);
  });
  it("后端不认（老后端）⇒ 说「版本过旧」（`deriveUi` 据此落「需更新」）", async () => {
    invokeMock.mockRejectedValue(UNSUPPORTED);
    const st = await fetchAccounts("devbox");
    expect(st.available).toBe(false);
    expect(st.error).toMatch(/过旧/);
    expect([st.oldBackend, deriveUi(st).kind]).toEqual([true, "needs-update"]);
    const t = await checkTrust("devbox", "/h/a", "/w");
    expect(t.available).toBe(false);
    expect(t.error).toMatch(/过旧/);
  });
  it("老后端回旧形状 `{lines}` ⇒ 不可用（两端契约对不上），不当成零个账号", async () => {
    invokeMock.mockResolvedValue(linesReply(['{"kind":"accounts-meta","enabled":true}']));
    const st = await fetchAccounts("devbox");
    expect(st.available).toBe(false);
    expect(st.error).toMatch(LIST_BAD);
  });
});
