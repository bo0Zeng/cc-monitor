// A3 accounts store 纯函数 + 缓存 + config 读写测试（vitest + jsdom）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
// config 写只交补丁；替身把补丁应用到 `loadConfig` 摆的那份上，写完的整份交 `fakeCfg.saved`。
vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));
// 记账失败要出声：只换 toast 这一个出口，判据读它收到了什么。
// 账号选不了的那句提示由 `withAccount` 自己出（先前是调用方各带一个「账号不可用」回调）。
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
// 上次值交本机后端记那一跳另有判据（`last-seen.vitest.ts`）；这里数的是账号那一问。
vi.mock("../../../src/frontend/ui/last-seen", () => ({ rememberSeen: vi.fn(async () => {}), recallSeen: vi.fn(async () => null) }));

import { invoke } from "@tauri-apps/api/core";
import { loadConfig } from "../../../src/frontend/ui/config";
import { deriveUi, defaultAccount, currentAccountForBadge, accountColorsActive, selectableAccounts, detectAccountMismatch, sessionBadge, shouldShowAccountBadge, type AccountsState, type Account } from "../../../src/frontend/ui/accounts";
import { accountsAgentProfile, fetchAccounts } from "../../../src/frontend/ui/account-reads";
import { AGENT_PROFILE_TABLE } from "../../../src/frontend/ui/generated/agent-profile-table";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { chosenAccount } from "../../../src/frontend/ui/launch-account";
import {
  accountReadCalls,
  chanArgsJson,
  isChanCall,
  withAccountReads,
  withHistoryReads,
  type ChanCallArgs,
} from "../../test-support/chan-fake";
import { copyPattern } from "../../test-support/copy-pattern";
import { nonEmpty } from "../../test-support/non-empty";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const loadCfg = loadConfig as unknown as ReturnType<typeof vi.fn>;

function acct(p: Partial<Account>): Account {
  return {
    name: "z",
    email: "z@example.test",
    configDir: "/h/.claude-alt/z",
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    authKind: "subscription",
    authReady: true,
    selectable: true,
    badge: { text: copyText("accounts.badge.signedIn"), warn: false, title: "" },
    ...p,
  };
}
/** 夹具照成品写：`selectable` · `meta.effectiveDefault` 是那台后端写好的格（规则住 `acct-core`，金样见下）。 */
function state(p: Partial<AccountsState>, effectiveDefault: string | null = null): AccountsState {
  return {
    origin: "devbox",
    available: true,
    oldBackend: false,
    error: null,
    notice: null,
    meta: {
      enabled: true,
      acctsDir: "/h/.claude-alt",
      manifestPath: "/h/.claude-alt/accounts.json",
      updatedAt: null,
      sharedStore: null,
      count: 0,
      error: null,
      effectiveDefault,
    },
    accounts: [],
    ...p,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("deriveUi 降级矩阵（DESIGN §7）", () => {
  // 🔴 `K-R59`：这里此前有一条「daemonless → hidden」。定框 `K35` 之后
  //    `accounts.rs::cfg_for` 不再产出那条错误串 ⇒ `AccountsUi` 的 `hidden` 那一档
  //    **再也到不了**，连档带测一起下岗。
  // 要求：「后端需更新在任何查询失败时都显示」⇒「只在真的版本不够时显示；查询失败按码说查询失败」。
  it("🔴 K-R59 · WF2：`available:false` 按失败种类分 —— 对端不认 ⇒ needs-update；其余 ⇒ query-failed（原因原样）；不再有「安静隐藏」那一档", () => {
    const failed = deriveUi(state({ available: false, error: "壳·离线" }));
    expect(failed).toEqual({ kind: "query-failed", reason: "壳·离线" });
    // 原因串里碰巧有「过旧」也不算（从前按串猜）：种类只看 `oldBackend`。
    expect(deriveUi(state({ available: false, error: "版本过旧？" })).kind).toBe("query-failed");
  });
  it("旧 backend（对端说不认这条命令）→ needs-update", () => {
    const ui = deriveUi(state({ available: false, oldBackend: true, error: "远端后端不支持账号查询（版本过旧）——请更新 backend" }));
    expect(ui.kind).toBe("needs-update");
  });
  it("未启用（enabled:false）→ not-enabled，带 manifest 路径", () => {
    const ui = deriveUi(
      state({ meta: { enabled: false, acctsDir: "/h/.claude-alt", manifestPath: "/h/.claude-alt/accounts.json", updatedAt: null, sharedStore: null, count: 0, error: "manifest 不可读", effectiveDefault: null } }),
    );
    expect(ui.kind).toBe("not-enabled");
    if (ui.kind === "not-enabled") {
      expect(ui.manifestPath).toBe("/h/.claude-alt/accounts.json");
      expect(ui.reason).toContain("manifest");
    }
  });
  it("enabled 但零账号 → not-enabled", () => {
    const ui = deriveUi(state({ accounts: [] }));
    expect(ui.kind).toBe("not-enabled");
  });
  it("有账号 → ready", () => {
    const ui = deriveUi(state({ accounts: [acct({ name: "z", isDefault: true })] }));
    expect(ui.kind).toBe("ready");
    if (ui.kind === "ready") expect(ui.accounts).toHaveLength(1);
  });
});

/**
 * 「这个号能不能选 · 默认号是谁」跨三处的金样（`tests/__fixtures__/account-default.golden.json`）：
 * 后端清单成品逐号写 `selectable`、`meta` 写 `effectiveDefault`（`accounts_query_tests` 钉）；命令行不带号落的是同一个（`plan_tests` 钉）；
 * 界面只照抄那两格 —— 这里按金样的期望造成品，界面读出的能选的号与默认号逐格相等（不另判：`isDefault` 与 `selectable` 故意反着也照成品）。
 */
describe("默认号 · 能选的号照那台写好的格（跨三处金样）", () => {
  const golden = JSON.parse(readFileSync(resolve(__dirname, "../../__fixtures__/account-default.golden.json"), "utf8")) as {
    cases: { what: string; accounts: { name: string; hasDir: boolean; isDefault: boolean; mode: string; authReady: boolean; exists: boolean }[]; effectiveDefault: string | null; selectable: string[] }[];
  };
  it("金样没被删薄", () => {
    nonEmpty(golden.cases);
  });
  for (const c of golden.cases) {
    it(c.what, () => {
      const accounts = c.accounts.map((a) =>
        acct({ name: a.name, configDir: a.hasDir ? `/h/${a.name}` : null, isDefault: a.isDefault, mode: a.mode, authReady: a.authReady, exists: a.exists, selectable: c.selectable.includes(a.name) }),
      );
      const s = state({ accounts }, c.effectiveDefault);
      expect(defaultAccount(s)?.name ?? null).toBe(c.effectiveDefault);
      expect(selectableAccounts(s).map((a) => a.name)).toEqual(c.selectable);
    });
  }
  it("界面不另判：成品说谁就是谁（与 isDefault · 鉴权那几格反着给也照成品）", () => {
    const s = state({ accounts: [acct({ name: "z", isDefault: true, selectable: false }), acct({ name: "b", authReady: false, selectable: true })] }, "b");
    expect(defaultAccount(s)?.name).toBe("b");
    expect(selectableAccounts(s).map((a) => a.name)).toEqual(["b"]);
  });
});

describe("currentAccountForBadge（account-ux U6：current 不可选就不能拿去判「不一致」）", () => {
  it("当前账号可选 → 返回它", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b", isDefault: true })] }, "b");
    expect(currentAccountForBadge(s)?.name).toBe("b");
  });
  // 下面三条 = DoD 明列的「current 不可选不对齐」。不过滤的话：账号徽章
  // （F09 后唯一消费者）会指着一个系统自己永远不会 follow 过去的账号说"你不一致"。
  it("当前账号未登录 → null（对齐必失败，不能拿它判「不一致」）", () => {
    const s = state({ accounts: [acct({ name: "b", isDefault: true, loggedIn: false, authReady: false, selectable: false })] }, "b");
    expect(defaultAccount(s)?.name).toBe("b"); // 默认号照样给
    expect(currentAccountForBadge(s)).toBeNull(); // 但对齐面必须拒
  });
  it("当前账号是 in-place（逃生口，不支持按会话切号）→ null", () => {
    const s = state({ accounts: [acct({ name: "b", isDefault: true, mode: "in-place", selectable: false })] }, "b");
    expect(currentAccountForBadge(s)).toBeNull();
  });
  it("当前账号目录缺失 → null", () => {
    const s = state({ accounts: [acct({ name: "b", isDefault: true, exists: false, selectable: false })] }, "b");
    expect(currentAccountForBadge(s)).toBeNull();
  });
  it("零账号 → null", () => {
    expect(currentAccountForBadge(state({ accounts: [] }))).toBeNull();
  });
});

describe("accountColorsActive（account-ux U8：单账号/降级时账号色系统休眠）", () => {
  const sel = (name: string): Account => acct({ name });
  it("≥2 个可选账号 → 激活（颜色此时才能区分东西）", () => {
    expect(accountColorsActive(state({ accounts: [sel("wei"), sel("amy")] }))).toBe(true);
  });
  it("只有 1 个可选账号 → 休眠（颜色区分不了任何东西，纯噪音）", () => {
    expect(accountColorsActive(state({ accounts: [sel("wei")] }))).toBe(false);
  });
  it("零账号 → 休眠", () => {
    expect(accountColorsActive(state({ accounts: [] }))).toBe(false);
  });
  it("有 2 个账号但只有 1 个**可选** → 休眠（数的是可选数，不是总数）", () => {
    const s = state({ accounts: [sel("wei"), acct({ name: "amy", loggedIn: false, authReady: false, selectable: false })] });
    expect(s.accounts.length).toBe(2); // 总数够
    expect(accountColorsActive(s)).toBe(false); // 但可选数不够
  });
  it("available=false（老 backend / 查询失败）→ 休眠，哪怕账号数够", () => {
    expect(
      accountColorsActive(state({ available: false, accounts: [sel("wei"), sel("amy")] })),
    ).toBe(false);
  });
  it("selectableAccounts 只留成品说选得了的", () => {
    const s = state({
      accounts: [sel("wei"), acct({ name: "amy", mode: "in-place", selectable: false }), acct({ name: "p", exists: false, selectable: false })],
    });
    expect(selectableAccounts(s).map((a) => a.name)).toEqual(["wei"]);
  });
});


describe("detectAccountMismatch（account-ux U1）", () => {
  it("两者都确知且不同 → true", () => {
    expect(detectAccountMismatch("b", "z")).toBe(true);
  });
  it("相同 → false", () => {
    expect(detectAccountMismatch("z", "z")).toBe(false);
  });
  it("live 未知 → false（不误报）", () => {
    expect(detectAccountMismatch(null, "z")).toBe(false);
  });
  it("无当前账号 → false", () => {
    expect(detectAccountMismatch("b", null)).toBe(false);
  });
  it("都 null → false", () => {
    expect(detectAccountMismatch(null, null)).toBe(false);
  });
});

describe("sessionBadge source 字段（account-ux U1/U5）", () => {
  const emailBy = new Map([["z", "z@example.test"]]);
  it("宣告说了号 ＋ 活着 → source:'live' + account 全名", () => {
    const b = sessionBadge("s1", "devbox", "z", true, emailBy);
    expect(b?.source).toBe("live");
    expect(b?.account).toBe("z");
  });
  it("lastAccount 兜底 → source:'last'", () => {
    const b = sessionBadge("s1", "devbox", null, true, emailBy, new Map([["s1", "b"]]));
    expect(b?.source).toBe("last");
    expect(b?.account).toBe("b");
  });
  it("未知 → source:'unknown' + account:null", () => {
    const b = sessionBadge("s1", "devbox", null, true, emailBy);
    expect(b?.source).toBe("unknown");
    expect(b?.account).toBeNull();
  });
});

describe("sessionBadge（§3 优先级）", () => {
  const emailBy = new Map([["z", "z@example.test"]]);
  it("本地会话（origin null）→ 无徽章", () => {
    expect(sessionBadge("s1", LOCAL_ORIGIN, "z", true, emailBy)).toBeNull();
  });
  it("宣告说了号 → 已知徽章，悬停带邮箱", () => {
    const b = sessionBadge("s1", "devbox", "z", true, emailBy);
    expect(b?.known).toBe(true);
    expect(b?.account).toBe("z");
    expect(b?.tooltip).toContain("z@example.test");
    expect(b?.tooltip).toMatch(copyPattern("accounts.sessionBadge.live"));
  });
  it("宣告没说（判不了）→ — 不猜", () => {
    const b = sessionBadge("s1", "devbox", null, true, emailBy);
    expect(b?.known).toBe(false);
    expect(b?.account).toBeNull();
  });
});

describe("fetchAccounts：要用时问后端，界面不存一份", () => {
  // backend 的本机 origin（`<local>`）⇒ 问本机后端，不拿它去问远端。
  // 本机与远端同一条路：经通道问 `<local>` 那条长连接的 `accounts-list`（后端出成品）。
  it("A3 / C4c：`<local>` 经通道问本机后端（`accounts-list` 发给 `<local>`），不是拿 `<local>` 去问远端配置", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta: null, accounts: [acct({})] }))));
    const st = await fetchAccounts("<local>");
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(accountReadCalls(invokeMock.mock.calls, "list_local_accounts")).toHaveLength(1);
    expect(st.available).toBe(true);
  });
  it("★ 每一次要用都问一次后端（没有缓存、没有 force 这一说）：两次就是两问，第二次拿到的是后端此刻的答", async () => {
    loadCfg.mockResolvedValue({});
    let n = 0;
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta: null, accounts: [acct({ name: `z${++n}` })] }))));
    await fetchAccounts("devbox");
    const again = await fetchAccounts("devbox");
    expect(invokeMock, "第二次拿了界面存的那一份").toHaveBeenCalledTimes(2);
    expect(again.accounts.map((a) => a.name)).toEqual(["z2"]);
  });
  it("没问到 ⇒ 此刻的事实照旧是不可用（上次那份只问本机后端，见 `last-seen.vitest.ts`）", async () => {
    loadCfg.mockResolvedValue({});
    const meta = { enabled: true, acctsDir: "/a", manifestPath: "/a/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null };
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta, accounts: [acct({})] }))));
    expect((await fetchAccounts("devbox")).available).toBe(true);
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => Promise.reject(copyText("beServer.words.cantConnect")))));
    const down = await fetchAccounts("devbox");
    expect(down.available).toBe(false);
    expect(down.accounts, "上次的混进了此刻的事实").toEqual([]);
    expect(down.last ?? null, "本机后端没记着那台，界面也不另记一份").toBeNull();
  });
  it("账号页那一家的画像与 `accounts-list` 线上交的 `agent` 是同一家，画像取自后端生成的画像表（不写死）", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta: null, accounts: [acct({})] }))));
    await fetchAccounts("<local>");
    const sent = invokeMock.mock.calls.filter((c) => isChanCall(c[0] as string, c[1], "accounts-list"));
    expect(sent).toHaveLength(1);
    const agent = (chanArgsJson(sent[0][1] as ChanCallArgs) as { agent: string }).agent;
    const p = accountsAgentProfile();
    expect(p, "查不到账号页那一家的画像").not.toBeNull();
    expect(AGENT_PROFILE_TABLE).toContain(p);
    expect(p!.agent).toBe(agent);
  });
  it("invoke throw（远端没配）→ available:false 不崩", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => (Promise.reject("远端 'x' 未配置")))));
    const s = await fetchAccounts("x");
    expect(s.available).toBe(false);
    expect(s.error).toContain("未配置");
  });
});

describe("sessionBadge 源②（lastAccount 兜底，A4）", () => {
  const emailBy = new Map([
    ["z", "z@example.test"],
    ["b", "b@example.org"],
  ]);
  it("活着、宣告说了号 → 用源①，忽略 lastAccount", () => {
    const b = sessionBadge("s1", "devbox", "z", true, emailBy, new Map([["s1", "b"]]));
    expect(b?.account).toBe("z");
    expect(b?.tooltip).toMatch(copyPattern("accounts.sessionBadge.live"));
  });
  it("宣告没说但有 lastAccount → 用源②，标注上次 + 带邮箱", () => {
    const b = sessionBadge("s1", "devbox", null, true, emailBy, new Map([["s1", "b"]]));
    expect(b?.known).toBe(true);
    expect(b?.account).toBe("b");
    expect(b?.tooltip).toMatch(copyPattern("accounts.sessionBadge.last"));
    expect(b?.tooltip).toContain("b@example.org");
  });
  it("结束了 → 宣告说过的号标「上次」（它就是这条会话最后跑的号），不当此刻", () => {
    const b = sessionBadge("s1", "devbox", "z", false, emailBy, new Map([["s1", "b"]]));
    expect(b?.source).toBe("last");
    expect(b?.account).toBe("z");
    expect(b?.tooltip).toContain(copyText("resumeMenu.account.last"));
  });
  it("都无 → —（含不传 lastAccountByS 也安全）", () => {
    expect(sessionBadge("s1", "devbox", null, false, emailBy, new Map())?.account).toBeNull();
    expect(sessionBadge("s1", "devbox", null, false, emailBy)?.account).toBeNull();
  });
  it("本地会话（origin null）源②也不显", () => {
    expect(sessionBadge("s1", LOCAL_ORIGIN, null, false, emailBy, new Map([["s1", "b"]]))).toBeNull();
  });
});

// F05：resolveAccount 纯函数——withAccount 内部决策逻辑的可独立测试版本。

describe("shouldShowAccountBadge（A4/§7 徽章门控）", () => {
  it("本地会话（origin null）→ 不显", () => {
    expect(shouldShowAccountBadge(LOCAL_ORIGIN, new Set(["devbox"]))).toBe(false);
  });
  it("ready 远端 → 显", () => {
    expect(shouldShowAccountBadge("devbox", new Set(["devbox"]))).toBe(true);
  });
  it("非 ready 远端（未迁移/旧）→ 不显（避免满屏 —）", () => {
    expect(shouldShowAccountBadge("devbox", new Set())).toBe(false);
    expect(shouldShowAccountBadge("box2", new Set(["devbox"]))).toBe(false);
  });
});



describe("Z01 账号 0（configDir 缺席）", () => {
  const zero = () =>
    acct({ name: "0", configDir: null, mode: "bare", loggedIn: true, exists: true, selectable: false });

  it("暂不可选：从 UI 起它需要 unset 注入，launch-plan 今天只会 export", () => {
    expect(selectableAccounts(state({ accounts: [zero()] }))).toEqual([]);
  });

  it("deriveUi 把降级说明透传出去（绝不静默）", () => {
    const st = state({ accounts: [acct({})], notice: "远端后端版本较旧：…" });
    const ui = deriveUi(st);
    expect(ui.kind).toBe("ready");
    if (ui.kind === "ready") expect(ui.notice).toContain("后端");
  });

  it("无缺时 notice 为 null", () => {
    const ui = deriveUi(state({ accounts: [acct({})] }));
    if (ui.kind === "ready") expect(ui.notice).toBeNull();
  });

  it("账号 0 在列不影响既有账号的解析", () => {
    const st = state({ accounts: [acct({ name: "z" }), zero()] });
    expect(selectableAccounts(st).map((a) => a.name)).toEqual(["z"]);
  });
});


// ============================================================================
// K-A1：鉴权方式那一维（`KAY2` / `KAY3`）
// ============================================================================
//
// ★ **为什么这一族必须落在 TS 这一侧**：那条级联整个住在这里 ——
// 成品 `selectable` 为假 ⇒ `selectableAccounts` 不收它 ⇒ 不能设为当前号 ·
// 进不了 resume/restart 菜单 ·
// `accountColorsActive` 因为「可选账号 ≥ 2」不成立而连账号色一起休眠。
// **Rust 全绿而这一条没改，用户看到的还是「这个号不能用」。**
describe("K-A1 鉴权方式：api-key 号不再因为缺凭据文件而不可用", () => {
  /** api-key 号，**目录里没有 `.credentials.json`**（⇒ `loggedIn:false`）。 */
  const apiKey = (name = "api") =>
    acct({
      name,
      configDir: `/h/.claude-alt/${name}`,
      loggedIn: false,
      authKind: "api-key",
      // 两个 Rust 生产者都会填它（值由 `acct_core::auth_ready` · `account_selectable` 算）。
      authReady: true,
      selectable: true,
    });
  /** 订阅号，**同样没有凭据文件** —— `KAY3` 的阴性对照。 */
  const subNoCred = (name = "sub") =>
    acct({
      name,
      configDir: `/h/.claude-alt/${name}`,
      loggedIn: false,
      authKind: "subscription",
      authReady: false,
      selectable: false,
    });

  it("★ KAY3：订阅号缺凭据 —— 进不了可选列表（规则在 Rust：`acct_core` 的金样）", () => {
    const st = state({ accounts: [subNoCred()] });
    expect(selectableAccounts(st)).toEqual([]);
  });

  it("★ KAY2 级联①：selectableAccounts 收它", () => {
    const st = state({ accounts: [apiKey(), subNoCred()] });
    expect(selectableAccounts(st).map((a) => a.name)).toEqual(["api"]);
  });

  it("★ KAY2 级联③：能当当前号 / 上徽章（跟随选中它由那台后端判，见后端判号那一族）", () => {
    const st = state({ accounts: [apiKey()] }, "api");
    expect(defaultAccount(st)?.name).toBe("api");
    expect(currentAccountForBadge(st)?.name).toBe("api");
    // 阴性对照同一格：订阅号缺凭据时落空。
    const bad = state({ accounts: [subNoCred()] }, "sub");
    expect(currentAccountForBadge(bad)).toBeNull();
  });

  it("★ KAY2 级联④：它算进「可选账号 ≥ 2」⇒ 账号色不再休眠", () => {
    // 一个订阅号（正常）+ 一个 api-key 号（缺凭据）= 2 个可选。
    expect(accountColorsActive(state({ accounts: [acct({ name: "z" }), apiKey()] }))).toBe(true);
    // 阴性对照：把 api-key 换成缺凭据的订阅号 ⇒ 只剩 1 个可选 ⇒ 仍休眠。
    expect(accountColorsActive(state({ accounts: [acct({ name: "z" }), subNoCred()] }))).toBe(
      false,
    );
  });

});



// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b` `D2 阻-3` / `D3 阻-2`：**取值口的行为判据**（先前它零行为判据）
// ═════════════════════════════════════════════════════════════════════════════
//
// ★★ `D2` 现打过两刀，**两刀都是 1495 全绿**：
// ① 把取值口掏空成「永远说不出账号」；② 把它头注**逐字禁止**的那条回落
//（有 pin 但不可选 ⇒ 下沉到当前号）真加进生产段。
// ⇒ 那时只有两条**源码形状**判据（数调用点），一条量行为的都没有。
// 本组就是那两刀的反面：**掏空必须红，加回落也必须红。**

// ═══════════════════════════════════════════════════════════════════════════
// `K-P5h` `KP5HD2`：**拿身份 token 回填新会话的 sid**
//
// ★★ 本组买的是「**拿 token 说出了一个它自己里面没有的 sid**」——
//    输入里 token 与 sid 是两个独立的格，输出必须是**那一条**的 sid，
//    而不是「第一条」「唯一一条」或任何与 token 无关的东西。
// ⚠ **人群只算「新开」那一支**：resume 那一支会退化成布尔谓词
//   （token 就是 sid ⇒ 答案要么是它自己要么 `null`），本组一格都不为它写。
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// `K-P5h` `KP5HD2` / `KP5HD3`：**待回填表** —— 起会话方终于把 pin 写得出来
// ═══════════════════════════════════════════════════════════════════════════

describe("launch-account：点名那一格", () => {
  it("账号 0 ⇒ base；名字 ⇒ 按名字（线上只有名字那一形）", () => {
    expect(chosenAccount(null)).toEqual({ kind: "base" });
    expect(chosenAccount("z")).toEqual({ kind: "named", name: "z" });
  });
});
