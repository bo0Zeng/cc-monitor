// A3 accounts store 纯函数 + 缓存 + config 读写测试（vitest + jsdom）。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./config", () => ({ loadConfig: vi.fn(), saveConfig: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { loadConfig, saveConfig } from "./config";
import {
  deriveUi,
  effectiveDefault,
  currentWorkingAccount,
  resolveFollowAccount,
  detectAccountMismatch,
  isSelectable,
  accountConfigDir,
  badgeText,
  sessionBadge,
  shouldShowAccountBadge,
  recordLastAccount,
  withAccount,
  getDefaultName,
  setDefaultName,
  fetchAccounts,
  fetchSessionAccounts,
  invalidateAccountsCache,
  __resetAccountsCacheForTest,
  type AccountsState,
  type Account,
  type SessionAccount,
} from "./accounts";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const loadCfg = loadConfig as unknown as ReturnType<typeof vi.fn>;
const saveCfg = saveConfig as unknown as ReturnType<typeof vi.fn>;

function acct(p: Partial<Account>): Account {
  return {
    name: "z",
    email: "z@x.edu",
    configDir: "/h/.claude-accts/z",
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    ...p,
  };
}
function state(p: Partial<AccountsState>): AccountsState {
  return {
    origin: "aya",
    available: true,
    error: null,
    meta: {
      enabled: true,
      acctsDir: "/h/.claude-accts",
      manifestPath: "/h/.claude-accts/accounts.json",
      updatedAt: null,
      sharedStore: null,
      count: 0,
      error: null,
    },
    accounts: [],
    defaultName: null,
    ...p,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  __resetAccountsCacheForTest();
});

describe("deriveUi 降级矩阵（DESIGN §7）", () => {
  it("daemonless → hidden", () => {
    const ui = deriveUi(state({ available: false, error: "该主机配置为 daemonless（无 daemon），账号功能不可用" }));
    expect(ui.kind).toBe("hidden");
  });
  it("旧 daemon → needs-update", () => {
    const ui = deriveUi(state({ available: false, error: "远端 daemon 不支持账号查询（版本过旧）——请更新 daemon" }));
    expect(ui.kind).toBe("needs-update");
  });
  it("未启用（enabled:false）→ not-enabled，带 manifest 路径", () => {
    const ui = deriveUi(
      state({ meta: { enabled: false, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 0, error: "manifest 不可读" } }),
    );
    expect(ui.kind).toBe("not-enabled");
    if (ui.kind === "not-enabled") {
      expect(ui.manifestPath).toBe("/h/.claude-accts/accounts.json");
      expect(ui.reason).toContain("manifest");
    }
  });
  it("enabled 但零账号 → not-enabled", () => {
    const ui = deriveUi(state({ accounts: [] }));
    expect(ui.kind).toBe("not-enabled");
  });
  it("有账号 → ready", () => {
    const ui = deriveUi(state({ accounts: [acct({ name: "z", isDefault: true })], defaultName: null }));
    expect(ui.kind).toBe("ready");
    if (ui.kind === "ready") expect(ui.accounts).toHaveLength(1);
  });
});

describe("effectiveDefault", () => {
  it("本机 defaultName 优先", () => {
    const s = state({ accounts: [acct({ name: "z", isDefault: true }), acct({ name: "b" })], defaultName: "b" });
    expect(effectiveDefault(s)?.name).toBe("b");
  });
  it("无 defaultName → 跟随 manifest isDefault", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b", isDefault: true })], defaultName: null });
    expect(effectiveDefault(s)?.name).toBe("b");
  });
  it("defaultName 指向已不存在的账号 → 回退 isDefault", () => {
    const s = state({ accounts: [acct({ name: "z", isDefault: true })], defaultName: "gone" });
    expect(effectiveDefault(s)?.name).toBe("z");
  });
  it("零账号 → null", () => {
    expect(effectiveDefault(state({ accounts: [] }))).toBeNull();
  });
});

describe("currentWorkingAccount（effectiveDefault 语义别名，account-ux U1）", () => {
  it("与 effectiveDefault 值一致：defaultName 优先", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b" })], defaultName: "b" });
    expect(currentWorkingAccount(s)?.name).toBe("b");
    expect(currentWorkingAccount(s)).toBe(effectiveDefault(s));
  });
  it("零账号 → null（与 effectiveDefault 一致）", () => {
    expect(currentWorkingAccount(state({ accounts: [] }))).toBeNull();
  });
});

describe("resolveFollowAccount（跟随解析器，account-ux U1：粘性优先）", () => {
  const s = state({
    accounts: [
      acct({ name: "z" }),
      acct({ name: "b" }),
      acct({ name: "x", loggedIn: false }), // 不可选（未登录）
    ],
  });
  it("lastAccount 可选 → 用 lastAccount（粘性优先，压过 current）", () => {
    expect(resolveFollowAccount(s, { lastAccount: "z", current: "b" })).toBe("z");
  });
  it("lastAccount 不可选 → 下沉到 current", () => {
    expect(resolveFollowAccount(s, { lastAccount: "x", current: "b" })).toBe("b");
  });
  it("lastAccount 指向不存在的号 → 下沉 current", () => {
    expect(resolveFollowAccount(s, { lastAccount: "nope", current: "b" })).toBe("b");
  });
  it("无 lastAccount → 用 current", () => {
    expect(resolveFollowAccount(s, { current: "z" })).toBe("z");
  });
  it("current 也不可选 → null（落基座）", () => {
    expect(resolveFollowAccount(s, { lastAccount: "x", current: "x" })).toBeNull();
  });
  it("两者都空 → null", () => {
    expect(resolveFollowAccount(s, {})).toBeNull();
  });
  it("null 值安全 → null", () => {
    expect(resolveFollowAccount(s, { lastAccount: null, current: null })).toBeNull();
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
  const emailBy = new Map([["z", "z@x.edu"]]);
  it("live → source:'live' + account 全名", () => {
    const m = new Map<string, SessionAccount>([
      ["s1", { pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/.claude-accts/z", account: "z", bare: false, alive: true }],
    ]);
    const b = sessionBadge("s1", "aya", m, emailBy);
    expect(b?.source).toBe("live");
    expect(b?.account).toBe("z");
  });
  it("lastAccount 兜底 → source:'last'", () => {
    const b = sessionBadge("s1", "aya", new Map(), emailBy, new Map([["s1", "b"]]));
    expect(b?.source).toBe("last");
    expect(b?.account).toBe("b");
  });
  it("未知 → source:'unknown' + account:null", () => {
    const b = sessionBadge("s1", "aya", new Map(), emailBy);
    expect(b?.source).toBe("unknown");
    expect(b?.account).toBeNull();
  });
});

describe("isSelectable", () => {
  it("isolated + 已登录 + 存在 → 可选", () => {
    expect(isSelectable(acct({}))).toBe(true);
  });
  it("未登录 → 不可选", () => {
    expect(isSelectable(acct({ loggedIn: false }))).toBe(false);
  });
  it("in-place → 不可选", () => {
    expect(isSelectable(acct({ mode: "in-place" }))).toBe(false);
  });
  it("目录不存在 → 不可选", () => {
    expect(isSelectable(acct({ exists: false }))).toBe(false);
  });
});

describe("badgeText", () => {
  it("ASCII 取前 2", () => {
    expect(badgeText("zeng")).toBe("ze");
    expect(badgeText("b")).toBe("b");
  });
  it("非 ASCII 取 1 个 code point", () => {
    expect(badgeText("张三")).toBe("张");
  });
  it("空 → ?", () => {
    expect(badgeText("")).toBe("?");
  });
});

describe("sessionBadge（§3 优先级）", () => {
  const emailBy = new Map([["z", "z@x.edu"]]);
  function live(rows: SessionAccount[]): Map<string, SessionAccount> {
    const m = new Map<string, SessionAccount>();
    for (const r of rows) if (r.sessionId) m.set(r.sessionId, r);
    return m;
  }
  it("本地会话（origin null）→ 无徽章", () => {
    expect(sessionBadge("s1", null, new Map(), emailBy)).toBeNull();
  });
  it("live 探测到账号 → 已知徽章", () => {
    const m = live([{ pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/.claude-accts/z", account: "z", bare: false, alive: true }]);
    const b = sessionBadge("s1", "aya", m, emailBy);
    expect(b?.known).toBe(true);
    expect(b?.text).toBe("z");
    expect(b?.tooltip).toContain("z@x.edu");
    expect(b?.tooltip).toContain("实时探测");
  });
  it("account:null（探测不到）→ — 不猜", () => {
    const m = live([{ pid: 1, sessionId: "s1", cwd: "/w", configDir: null, account: null, bare: true, alive: true }]);
    const b = sessionBadge("s1", "aya", m, emailBy);
    expect(b?.known).toBe(false);
    expect(b?.text).toBe("—");
  });
  it("会话不在 live 表里 → —", () => {
    const b = sessionBadge("s1", "aya", new Map(), emailBy);
    expect(b?.text).toBe("—");
  });
  it("探测到但已死 → —（不贴陈旧账号）", () => {
    const m = live([{ pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/.claude-accts/z", account: "z", bare: false, alive: false }]);
    expect(sessionBadge("s1", "aya", m, emailBy)?.known).toBe(false);
  });
});

describe("defaultName config 读写", () => {
  it("无 accounts 键 → null", async () => {
    loadCfg.mockResolvedValue({});
    expect(await getDefaultName()).toBeNull();
  });
  it("有 defaultName → 读回", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "b" } });
    expect(await getDefaultName()).toBe("b");
  });
  it("写入保留其它键（不丢字段）", async () => {
    loadCfg.mockResolvedValue({ theme: "dark", accounts: { somethingElse: 1 } });
    await setDefaultName("z");
    const written = saveCfg.mock.calls[0][0] as Record<string, unknown>;
    expect(written.theme).toBe("dark"); // 其它顶层键不丢
    const a = written.accounts as Record<string, unknown>;
    expect(a.defaultName).toBe("z");
    expect(a.somethingElse).toBe(1); // accounts 内其它键不丢
  });
  it("清除（null）→ 删掉 defaultName", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "z", keep: 1 } });
    await setDefaultName(null);
    const a = (saveCfg.mock.calls[0][0] as Record<string, unknown>).accounts as Record<string, unknown>;
    expect(a.defaultName).toBeUndefined();
    expect(a.keep).toBe(1);
  });
});

describe("fetchAccounts TTL 缓存", () => {
  it("首次 fetch 命中 invoke，TTL 内不重发", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockResolvedValue({ available: true, error: null, meta: { enabled: true, acctsDir: "/a", manifestPath: "/a/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null }, accounts: [acct({})] });
    await fetchAccounts("aya");
    await fetchAccounts("aya");
    expect(invokeMock).toHaveBeenCalledTimes(1); // 第二次走缓存
  });
  it("force=true 强制重发", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockResolvedValue({ available: true, error: null, meta: null, accounts: [] });
    await fetchAccounts("aya");
    await fetchAccounts("aya", true);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
  it("invalidate 后重发", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockResolvedValue({ available: true, error: null, meta: null, accounts: [] });
    await fetchAccounts("aya");
    invalidateAccountsCache("aya");
    await fetchAccounts("aya");
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
  it("invoke throw（远端没配）→ available:false 不崩", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockRejectedValue("远端 'x' 未配置");
    const s = await fetchAccounts("x");
    expect(s.available).toBe(false);
    expect(s.error).toContain("未配置");
  });
  it("把 config 的 defaultName 合进 state", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "b" } });
    invokeMock.mockResolvedValue({ available: true, error: null, meta: { enabled: true, acctsDir: "/a", manifestPath: "/a/x", updatedAt: null, sharedStore: null, count: 2, error: null }, accounts: [acct({ name: "z", isDefault: true }), acct({ name: "b" })] });
    const s = await fetchAccounts("aya");
    expect(s.defaultName).toBe("b");
    expect(effectiveDefault(s)?.name).toBe("b");
  });
});

describe("fetchSessionAccounts", () => {
  it("available:false → 空数组", async () => {
    invokeMock.mockResolvedValue({ available: false, error: "x", sessions: [] });
    expect(await fetchSessionAccounts("aya")).toEqual([]);
  });
  it("available:true → 返回 sessions", async () => {
    invokeMock.mockResolvedValue({ available: true, error: null, sessions: [{ pid: 1, sessionId: "s", cwd: "/w", configDir: null, account: null, bare: true, alive: true }] });
    const s = await fetchSessionAccounts("aya");
    expect(s).toHaveLength(1);
  });
});

describe("sessionBadge 源②（lastAccount 兜底，A4）", () => {
  const emailBy = new Map([
    ["z", "z@x.edu"],
    ["b", "b@y.com"],
  ]);
  function live(rows: SessionAccount[]): Map<string, SessionAccount> {
    const m = new Map<string, SessionAccount>();
    for (const r of rows) if (r.sessionId) m.set(r.sessionId, r);
    return m;
  }
  it("有 live 探测 → 用源①，忽略 lastAccount", () => {
    const m = live([
      { pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/z", account: "z", bare: false, alive: true },
    ]);
    const b = sessionBadge("s1", "aya", m, emailBy, new Map([["s1", "b"]]));
    expect(b?.text).toBe("z");
    expect(b?.tooltip).toContain("实时探测");
  });
  it("无 live 但有 lastAccount → 用源②，标注上次 + 带邮箱", () => {
    const b = sessionBadge("s1", "aya", new Map(), emailBy, new Map([["s1", "b"]]));
    expect(b?.known).toBe(true);
    expect(b?.text).toBe("b");
    expect(b?.tooltip).toContain("上次用本工具起");
    expect(b?.tooltip).toContain("b@y.com");
  });
  it("live 存在但已死 + 有 lastAccount → 回退源②", () => {
    const m = live([
      { pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/z", account: "z", bare: false, alive: false },
    ]);
    const b = sessionBadge("s1", "aya", m, emailBy, new Map([["s1", "b"]]));
    expect(b?.text).toBe("b");
    expect(b?.tooltip).toContain("上次");
  });
  it("都无 → —（含不传 lastAccountByS 也安全）", () => {
    expect(sessionBadge("s1", "aya", new Map(), emailBy, new Map())?.text).toBe("—");
    expect(sessionBadge("s1", "aya", new Map(), emailBy)?.text).toBe("—");
  });
  it("本地会话（origin null）源②也不显", () => {
    expect(sessionBadge("s1", null, new Map(), emailBy, new Map([["s1", "b"]]))).toBeNull();
  });
});

describe("accountConfigDir（A4：账号名→configDir，仅可选账号）", () => {
  it("可选账号 → 返回其 configDir", () => {
    const s = state({ accounts: [acct({ name: "z", configDir: "/h/z" })] });
    expect(accountConfigDir(s, "z")).toBe("/h/z");
  });
  it("找不到该名 → null", () => {
    const s = state({ accounts: [acct({ name: "z" })] });
    expect(accountConfigDir(s, "nope")).toBeNull();
  });
  it("不可选账号（in-place / 未登录 / 目录不在）→ null（绝不注入）", () => {
    expect(accountConfigDir(state({ accounts: [acct({ name: "z", mode: "in-place" })] }), "z")).toBeNull();
    expect(accountConfigDir(state({ accounts: [acct({ name: "z", loggedIn: false })] }), "z")).toBeNull();
    expect(accountConfigDir(state({ accounts: [acct({ name: "z", exists: false })] }), "z")).toBeNull();
  });
  it("可选但 configDir 空 → null", () => {
    const s = state({ accounts: [acct({ name: "z", configDir: "" })] });
    expect(accountConfigDir(s, "z")).toBeNull();
  });
});

describe("shouldShowAccountBadge（A4/§7 徽章门控）", () => {
  it("本地会话（origin null）→ 不显", () => {
    expect(shouldShowAccountBadge(null, new Set(["aya"]))).toBe(false);
  });
  it("ready 远端 → 显", () => {
    expect(shouldShowAccountBadge("aya", new Set(["aya"]))).toBe(true);
  });
  it("非 ready 远端（daemonless/未迁移/旧）→ 不显（避免满屏 —）", () => {
    expect(shouldShowAccountBadge("aya", new Set())).toBe(false);
    expect(shouldShowAccountBadge("box2", new Set(["aya"]))).toBe(false);
  });
});

describe("recordLastAccount（A4）", () => {
  it("invoke update_history_metadata 带 patch.lastAccount", async () => {
    invokeMock.mockResolvedValue({});
    await recordLastAccount("s1", "z");
    expect(invokeMock).toHaveBeenCalledWith("update_history_metadata", {
      sessionId: "s1",
      patch: { lastAccount: "z" },
    });
  });
  it("invoke 抛错 → 静默不抛（记忆非关键路径）", async () => {
    invokeMock.mockRejectedValue(new Error("boom"));
    await expect(recordLastAccount("s1", "z")).resolves.toBeUndefined();
  });
});

describe("withAccount（A4 统一编排 resolve+record，三站点共用）", () => {
  const okRaw = (accounts: Account[]) => ({
    available: true,
    error: null,
    meta: { enabled: true, acctsDir: "/a", manifestPath: "/a/x.json", updatedAt: null, sharedStore: null, count: accounts.length, error: null },
    accounts,
  });
  it("accountName=null → run(undefined)，不 fetch、不记账", async () => {
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", null, run, { sessionId: "s1" });
    expect(run).toHaveBeenCalledWith(undefined);
    expect(invokeMock).not.toHaveBeenCalled();
  });
  it("可选账号 + sessionId → run(configDir) + 记 lastAccount", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockResolvedValue(okRaw([acct({ name: "z", configDir: "/h/z" })]));
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", "z", run, { sessionId: "s1" });
    expect(run).toHaveBeenCalledWith("/h/z");
    expect(invokeMock).toHaveBeenCalledWith("update_history_metadata", {
      sessionId: "s1",
      patch: { lastAccount: "z" },
    });
  });
  it("可选账号但无 sessionId（新会话）→ run(configDir)，不记账", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockResolvedValue(okRaw([acct({ name: "z", configDir: "/h/z" })]));
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", "z", run, {});
    expect(run).toHaveBeenCalledWith("/h/z");
    expect(invokeMock).not.toHaveBeenCalledWith("update_history_metadata", expect.anything());
  });
  it("不可选账号 → onUnselectable + run(undefined)（退化默认）、不记账", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockResolvedValue(okRaw([acct({ name: "z", loggedIn: false })]));
    const run = vi.fn().mockResolvedValue(undefined);
    const onUnsel = vi.fn();
    await withAccount("aya", "z", run, { sessionId: "s1", onUnselectable: onUnsel });
    expect(onUnsel).toHaveBeenCalledWith("z");
    expect(run).toHaveBeenCalledWith(undefined);
    expect(invokeMock).not.toHaveBeenCalledWith("update_history_metadata", expect.anything());
  });
  it("账号库不可用（fetch reject）→ 退化默认 run(undefined) + onUnselectable", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockRejectedValue(new Error("boom"));
    const run = vi.fn().mockResolvedValue(undefined);
    const onUnsel = vi.fn();
    await withAccount("aya", "z", run, { onUnselectable: onUnsel });
    expect(run).toHaveBeenCalledWith(undefined);
    expect(onUnsel).toHaveBeenCalledWith("z");
  });

  // ---- account-ux U2：跟随模式（opt-in opts.follow）----
  it("follow：lastAccount 可选 → run(它的 configDir) + 记 lastAccount（粘性压过 current）", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "b" } });
    invokeMock.mockResolvedValue(
      okRaw([acct({ name: "z", configDir: "/h/z" }), acct({ name: "b", configDir: "/h/b" })]),
    );
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", null, run, { sessionId: "s1", follow: { lastAccount: "z" } });
    expect(run).toHaveBeenCalledWith("/h/z"); // last=z 压过 current=b
    expect(invokeMock).toHaveBeenCalledWith("update_history_metadata", {
      sessionId: "s1",
      patch: { lastAccount: "z" },
    });
  });
  it("follow：既有 pin 不可选 → 下沉 current 起会话，但**不记账**（保住原 pin，U3 审计 重要-1 clobber 防护）", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "b" } });
    invokeMock.mockResolvedValue(
      okRaw([acct({ name: "z", loggedIn: false }), acct({ name: "b", configDir: "/h/b" })]),
    );
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", null, run, { sessionId: "s1", follow: { lastAccount: "z" } });
    expect(run).toHaveBeenCalledWith("/h/b"); // z 不可选 → 用 current=b 起
    // 既有 pin=z 存在且解析结果(b)≠pin → **不 clobber**，绝不把粘性从 z 翻成 b。
    expect(invokeMock).not.toHaveBeenCalledWith("update_history_metadata", expect.anything());
  });
  it("follow：无既有 pin（no-owner）→ 落 current → 记 current（become sticky，决策②）", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "z" } });
    invokeMock.mockResolvedValue(okRaw([acct({ name: "z", configDir: "/h/z" })]));
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", null, run, { sessionId: "s1", follow: {} }); // 无 pin
    expect(run).toHaveBeenCalledWith("/h/z");
    expect(invokeMock).toHaveBeenCalledWith("update_history_metadata", {
      sessionId: "s1",
      patch: { lastAccount: "z" },
    });
  });
  it("follow 迁移守卫：无 lastAccount + 老 config 仅 defaultName → 解析出当前账号", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "z" } });
    invokeMock.mockResolvedValue(
      okRaw([acct({ name: "z", configDir: "/h/z" }), acct({ name: "b", configDir: "/h/b" })]),
    );
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", null, run, { follow: {} });
    expect(run).toHaveBeenCalledWith("/h/z");
  });
  it("follow：last 与 current 都不可选 → run(undefined) 落基座，不 toast、不记账", async () => {
    loadCfg.mockResolvedValue({}); // 无 defaultName
    invokeMock.mockResolvedValue(okRaw([acct({ name: "z", loggedIn: false })])); // 唯一账号不可选
    const run = vi.fn().mockResolvedValue(undefined);
    const onUnsel = vi.fn();
    await withAccount("aya", null, run, {
      sessionId: "s1",
      follow: { lastAccount: "z" },
      onUnselectable: onUnsel,
    });
    expect(run).toHaveBeenCalledWith(undefined);
    expect(onUnsel).not.toHaveBeenCalled(); // 跟随下沉不打扰用户
    expect(invokeMock).not.toHaveBeenCalledWith("update_history_metadata", expect.anything());
  });
  it("follow：新会话无 sessionId → run(configDir) 但不记账", async () => {
    loadCfg.mockResolvedValue({ accounts: { defaultName: "z" } });
    invokeMock.mockResolvedValue(okRaw([acct({ name: "z", configDir: "/h/z" })]));
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", null, run, { follow: {} });
    expect(run).toHaveBeenCalledWith("/h/z");
    expect(invokeMock).not.toHaveBeenCalledWith("update_history_metadata", expect.anything());
  });
  it("accountName=null 且无 follow → 仍不 fetch、落基座（A4 逐字节，回归守卫）", async () => {
    const run = vi.fn().mockResolvedValue(undefined);
    await withAccount("aya", null, run, { sessionId: "s1" });
    expect(run).toHaveBeenCalledWith(undefined);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
