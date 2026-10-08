// A3 accounts store 纯函数 + 缓存 + config 读写测试（vitest + jsdom）。
import { describe, it, expect, vi, beforeEach } from "vitest";

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
import { fakeCfg } from "./config-patch-fake";
import { deriveUi, effectiveDefault, currentWorkingAccount, currentAccountForBadge, accountColorsActive, selectableAccounts, detectAccountMismatch, isSelectable, badgeText, sessionBadge, shouldShowAccountBadge, accountStatusBadge, apikeyEndpointStateFor, type AccountsState, type Account, type SessionAccount } from "../../../src/frontend/ui/accounts";
import { fetchAccounts, fetchSessionAccounts, parseSessionAccountLines, invalidateAccountsCache, __resetAccountsCacheForTest, fetchMachineApikeyRouting } from "../../../src/frontend/ui/account-reads";
import { getModelForAccount, setModelForAccount, moveMachinePrefs } from "../../../src/frontend/ui/account-prefs";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { toast as showActionFailureToast } from "../../../src/frontend/ui/kit/toast";
import { accountUnavailableOf, chosenAccount, refuseUnavailableAccount, type AccountUnavailable } from "../../../src/frontend/ui/launch-account";
import { ControlError } from "../../../src/frontend/ui/control-said";
import { ChanError } from "../../../src/comms/inward/chan";
import {
  accountReadCalls,
  chanArgsJson,
  chanReply,
  isChanCall,
  linesReply,
  NO_CHANNEL,
  withAccountReads,
  withHistoryReads,
  type ChanCallArgs,
} from "../../test-support/chan-fake";
import { copyPattern } from "../../test-support/copy-pattern";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const loadCfg = loadConfig as unknown as ReturnType<typeof vi.fn>;
const saveCfg = fakeCfg.saved;

function acct(p: Partial<Account>): Account {
  return {
    name: "z",
    email: "z@x.edu",
    configDir: "/h/.claude-alt/z",
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    authKind: "subscription",
    authReady: true,
    ...p,
  };
}
function state(p: Partial<AccountsState>): AccountsState {
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
    },
    accounts: [],
    ...p,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  __resetAccountsCacheForTest();
});

describe("deriveUi 降级矩阵（DESIGN §7）", () => {
  // 🔴 `K-R59`：这里此前有一条「daemonless → hidden」。定框 `K35` 之后
  //    `accounts.rs::cfg_for` 不再产出那条错误串 ⇒ `AccountsUi` 的 `hidden` 那一档
  //    **再也到不了**，连档带测一起下岗。
  // 要求：「后端需更新在任何查询失败时都显示」⇒「只在真的版本不够时显示；查询失败按码说查询失败」。
  it("🔴 K-R59 · WF2：`available:false` 按失败种类分 —— 对端不认 ⇒ needs-update；其余 ⇒ query-failed（原因原样）；不再有「安静隐藏」那一档", () => {
    const failed = deriveUi(state({ available: false, error: copyText("chanCaller.said.unreachable") }));
    expect(failed).toEqual({ kind: "query-failed", reason: copyText("chanCaller.said.unreachable") });
    // 原因串里碰巧有「过旧」也不算（从前按串猜）：种类只看 `oldBackend`。
    expect(deriveUi(state({ available: false, error: "版本过旧？" })).kind).toBe("query-failed");
  });
  it("旧 backend（对端说不认这条命令）→ needs-update", () => {
    const ui = deriveUi(state({ available: false, oldBackend: true, error: "远端后端不支持账号查询（版本过旧）——请更新 backend" }));
    expect(ui.kind).toBe("needs-update");
  });
  it("未启用（enabled:false）→ not-enabled，带 manifest 路径", () => {
    const ui = deriveUi(
      state({ meta: { enabled: false, acctsDir: "/h/.claude-alt", manifestPath: "/h/.claude-alt/accounts.json", updatedAt: null, sharedStore: null, count: 0, error: "manifest 不可读" } }),
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

describe("effectiveDefault", () => {
  it("那台清单里 isDefault 的那一个", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b", isDefault: true })] });
    expect(effectiveDefault(s)?.name).toBe("b");
  });
  it("没有标默认的 → 第一个", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b" })] });
    expect(effectiveDefault(s)?.name).toBe("z");
  });
  it("零账号 → null", () => {
    expect(effectiveDefault(state({ accounts: [] }))).toBeNull();
  });
});

describe("currentWorkingAccount（effectiveDefault 语义别名）", () => {
  it("与 effectiveDefault 值一致", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b", isDefault: true })] });
    expect(currentWorkingAccount(s)?.name).toBe("b");
    expect(currentWorkingAccount(s)).toBe(effectiveDefault(s));
  });
  it("零账号 → null（与 effectiveDefault 一致）", () => {
    expect(currentWorkingAccount(state({ accounts: [] }))).toBeNull();
  });
});

describe("currentAccountForBadge（account-ux U6：current 不可选就不能拿去判「不一致」）", () => {
  it("当前账号可选 → 返回它（与 currentWorkingAccount 同值）", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b", isDefault: true })] });
    expect(currentAccountForBadge(s)?.name).toBe("b");
  });
  // 下面三条 = DoD 明列的「current 不可选不对齐」。不过滤的话：账号徽章
  // （F09 后唯一消费者）会指着一个系统自己永远不会 follow 过去的账号说"你不一致"。
  it("当前账号未登录 → null（对齐必失败，不能拿它判「不一致」）", () => {
    const s = state({ accounts: [acct({ name: "b", isDefault: true, loggedIn: false, authReady: false })] });
    expect(currentWorkingAccount(s)?.name).toBe("b"); // effectiveDefault 照样给
    expect(currentAccountForBadge(s)).toBeNull(); // 但对齐面必须拒
  });
  it("当前账号是 in-place（逃生口，不支持按会话切号）→ null", () => {
    const s = state({ accounts: [acct({ name: "b", isDefault: true, mode: "in-place" })] });
    expect(currentAccountForBadge(s)).toBeNull();
  });
  it("当前账号目录缺失 → null", () => {
    const s = state({ accounts: [acct({ name: "b", isDefault: true, exists: false })] });
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
    const s = state({ accounts: [sel("wei"), acct({ name: "amy", loggedIn: false, authReady: false })] });
    expect(s.accounts.length).toBe(2); // 总数够
    expect(accountColorsActive(s)).toBe(false); // 但可选数不够
  });
  it("available=false（老 backend / 查询失败）→ 休眠，哪怕账号数够", () => {
    expect(
      accountColorsActive(state({ available: false, accounts: [sel("wei"), sel("amy")] })),
    ).toBe(false);
  });
  it("selectableAccounts 只留 isSelectable 的", () => {
    const s = state({
      accounts: [sel("wei"), acct({ name: "amy", mode: "in-place" }), acct({ name: "p", exists: false })],
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
  const emailBy = new Map([["z", "z@x.edu"]]);
  it("live → source:'live' + account 全名", () => {
    const m = new Map<string, SessionAccount>([
      ["s1", { pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/.claude-alt/z", account: "z", bare: false, alive: true }],
    ]);
    const b = sessionBadge("s1", "devbox", m, emailBy);
    expect(b?.source).toBe("live");
    expect(b?.account).toBe("z");
  });
  it("lastAccount 兜底 → source:'last'", () => {
    const b = sessionBadge("s1", "devbox", new Map(), emailBy, new Map([["s1", "b"]]));
    expect(b?.source).toBe("last");
    expect(b?.account).toBe("b");
  });
  it("未知 → source:'unknown' + account:null", () => {
    const b = sessionBadge("s1", "devbox", new Map(), emailBy);
    expect(b?.source).toBe("unknown");
    expect(b?.account).toBeNull();
  });
});

describe("isSelectable", () => {
  it("isolated + 已登录 + 存在 → 可选", () => {
    expect(isSelectable(acct({}))).toBe(true);
  });
  it("未登录 → 不可选", () => {
    expect(isSelectable(acct({ loggedIn: false, authReady: false }))).toBe(false);
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
    expect(sessionBadge("s1", LOCAL_ORIGIN, new Map(), emailBy)).toBeNull();
  });
  it("live 探测到账号 → 已知徽章", () => {
    const m = live([{ pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/.claude-alt/z", account: "z", bare: false, alive: true }]);
    const b = sessionBadge("s1", "devbox", m, emailBy);
    expect(b?.known).toBe(true);
    expect(b?.text).toBe("z");
    expect(b?.tooltip).toContain("z@x.edu");
    expect(b?.tooltip).toMatch(copyPattern("accounts.sessionBadge.live"));
  });
  it("account:null（探测不到）→ — 不猜", () => {
    const m = live([{ pid: 1, sessionId: "s1", cwd: "/w", configDir: null, account: null, bare: true, alive: true }]);
    const b = sessionBadge("s1", "devbox", m, emailBy);
    expect(b?.known).toBe(false);
    expect(b?.text).toBe("—");
  });
  it("会话不在 live 表里 → —", () => {
    const b = sessionBadge("s1", "devbox", new Map(), emailBy);
    expect(b?.text).toBe("—");
  });
  it("探测到但已死 → —（不贴陈旧账号）", () => {
    const m = live([{ pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/.claude-alt/z", account: "z", bare: false, alive: false }]);
    expect(sessionBadge("s1", "devbox", m, emailBy)?.known).toBe(false);
  });
});

// 默认账号 / 默认模型按机器存（`accounts.byMachine.<机器>`）：在一台上设的不许改到别台的同名号。

// F07：每账号模型偏好 config 读写——按机器分。
describe("modelByAccount config 读写（F07，按机器）", () => {
  it("无 accounts 键 → undefined", async () => {
    loadCfg.mockResolvedValue({});
    expect(await getModelForAccount("devbox", "z")).toBeUndefined();
  });
  it("有这台的 modelByAccount[z] → 读回；未设置的账号名 / 别的机器 → undefined", async () => {
    loadCfg.mockResolvedValue({ accounts: { byMachine: { devbox: { modelByAccount: { z: "opus" } } } } });
    expect(await getModelForAccount("devbox", "z")).toBe("opus");
    expect(await getModelForAccount("devbox", "b")).toBeUndefined();
    expect(await getModelForAccount("nano", "z")).toBeUndefined();
  });
  it("★ 写入保留其它键 + 多账号互不影响 + 别的机器上的同名号不受影响", async () => {
    loadCfg.mockResolvedValue({
      theme: "dark",
      accounts: { byMachine: { devbox: { other: "b", modelByAccount: { b: "sonnet" } }, nano: { modelByAccount: { z: "haiku" } } } },
    });
    await setModelForAccount("devbox", "z", "opus");
    const written = saveCfg.mock.calls[0][0] as Record<string, unknown>;
    expect(written.theme).toBe("dark");
    const by = (written.accounts as Record<string, unknown>).byMachine as Record<string, Record<string, unknown>>;
    expect(by.devbox.other).toBe("b"); // 这台其它键不丢
    expect(by.devbox.modelByAccount).toEqual({ b: "sonnet", z: "opus" });
    expect(by.nano.modelByAccount).toEqual({ z: "haiku" });
  });
  it("清除（null）→ 只删这台这个号这一条，其余保留", async () => {
    loadCfg.mockResolvedValue({ accounts: { byMachine: { devbox: { modelByAccount: { z: "opus", b: "sonnet" } } } } });
    await setModelForAccount("devbox", "z", null);
    const by = ((saveCfg.mock.calls[0][0] as Record<string, unknown>).accounts as Record<string, unknown>).byMachine as Record<string, Record<string, unknown>>;
    expect(by.devbox.modelByAccount).toEqual({ b: "sonnet" });
  });
  // Phase D 审计（阻塞项修复）：非法模型名必须在写入点被拒绝，不能只留给起会话时的
  // MODEL_DIMENSION.apply() 才发现——那样会让该账号往后每一次会话拉起都统一失败。
  it("非法模型名（含 shell 元字符/空格）→ throw，不落盘", async () => {
    loadCfg.mockResolvedValue({ accounts: {} });
    await expect(setModelForAccount("devbox", "z", "opus; rm -rf /")).rejects.toThrow(copyPattern("accounts.setModel.invalid"));
    await expect(setModelForAccount("devbox", "z", "Claude Opus 4.5")).rejects.toThrow(copyPattern("accounts.setModel.invalid")); // 空格非法
    expect(saveCfg).not.toHaveBeenCalled();
  });
  // 规则换成共享那一份（生成物）之后，真实模型名都放行：
  // 原先 TS 那份会拒这几条。正例的全集在共用金样 `identifier-rules.golden.json`（`identifier-rules-parity.vitest.ts`）。
  it("真实模型名（`sonnet[1m]` · Bedrock · Vertex）写得进去", async () => {
    loadCfg.mockResolvedValue({ accounts: {} });
    for (const m of ["sonnet[1m]", "us.anthropic.claude-sonnet-4-5-20250929-v1:0", "claude-sonnet-4-5@20250929"]) {
      await expect(setModelForAccount("devbox", "z", m)).resolves.toBeUndefined();
    }
  });
  it("清除（null）不受校验约束——恒允许", async () => {
    loadCfg.mockResolvedValue({ accounts: { byMachine: { devbox: { modelByAccount: { z: "opus" } } } } });
    await expect(setModelForAccount("devbox", "z", null)).resolves.toBeUndefined();
  });
  it("机器改名：那台的偏好整段搬到新名字下", async () => {
    loadCfg.mockResolvedValue({ accounts: { byMachine: { devbox: { other: "z", modelByAccount: { z: "opus" } }, nano: { other: "a" } } } });
    await moveMachinePrefs("devbox", "aya2");
    const by = ((saveCfg.mock.calls[0][0] as Record<string, unknown>).accounts as Record<string, unknown>).byMachine as Record<string, unknown>;
    expect(by).toEqual({ aya2: { other: "z", modelByAccount: { z: "opus" } }, nano: { other: "a" } });
  });
});

describe("fetchAccounts TTL 缓存", () => {
  // backend 的本机 origin（`<local>`）⇒ 问本机后端，不拿它去问远端。
  // 本机与远端同一条路：经通道问 `<local>` 那条长连接的 `accounts-list`（后端出成品）。
  it("A3 / C4c：`<local>` 经通道问本机后端（`accounts-list` 发给 `<local>`），不是拿 `<local>` 去问远端配置", async () => {
    __resetAccountsCacheForTest();
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta: null, accounts: [acct({})] }))));
    const st = await fetchAccounts("<local>");
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(accountReadCalls(invokeMock.mock.calls, "list_local_accounts")).toHaveLength(1);
    expect(st.available).toBe(true);
    // 与 `fetchLocalAccounts` 共用同一格缓存（账号面的本机键），TTL 内不重发。
    await fetchAccounts("<local>");
    expect(invokeMock).toHaveBeenCalledTimes(1);
    __resetAccountsCacheForTest();
  });
  it("没问到 ⇒ 带着这次运行里那台最近一次答成的那一份（画「上次的」），此刻的事实照旧是不可用", async () => {
    __resetAccountsCacheForTest();
    loadCfg.mockResolvedValue({});
    const meta = { enabled: true, acctsDir: "/a", manifestPath: "/a/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null };
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta, accounts: [acct({})] }))));
    const first = await fetchAccounts("devbox");
    expect(first.available).toBe(true);
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => Promise.reject(copyText("machineCard.test.unreachable", { e: "ETIMEDOUT" })))));
    const down = await fetchAccounts("devbox", true);
    expect(down.available).toBe(false);
    expect(down.accounts, "上次的混进了此刻的事实").toEqual([]);
    expect(down.last?.accounts).toEqual(first.accounts);
    expect(down.last?.meta).toEqual(first.meta);
    expect((await fetchAccounts("nano", true)).last ?? null, "没答成过的那台也说有上次").toBeNull();
    __resetAccountsCacheForTest();
  });
  it("首次 fetch 命中 invoke，TTL 内不重发", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta: { enabled: true, acctsDir: "/a", manifestPath: "/a/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null }, accounts: [acct({})] }))));
    await fetchAccounts("devbox");
    await fetchAccounts("devbox");
    expect(invokeMock).toHaveBeenCalledTimes(1); // 第二次走缓存
  });
  it("force=true 强制重发", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta: null, accounts: [] }))));
    await fetchAccounts("devbox");
    await fetchAccounts("devbox", true);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
  it("invalidate 后重发", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => ({ available: true, error: null, meta: null, accounts: [] }))));
    await fetchAccounts("devbox");
    invalidateAccountsCache("devbox");
    await fetchAccounts("devbox");
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
  it("invoke throw（远端没配）→ available:false 不崩", async () => {
    loadCfg.mockResolvedValue({});
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(() => (Promise.reject("远端 'x' 未配置")))));
    const s = await fetchAccounts("x");
    expect(s.available).toBe(false);
    expect(s.error).toContain("未配置");
  });
});

// 这一组原先驱动两条 Tauri 命令的 `available` 形状；它们退役了，
//   「会话 ↔ 账号」经通道问后端 `accounts-sessions`（本机与远端同一条路）。量的是那一跳的真实形状。
describe("fetchSessionAccounts（经通道 `accounts-sessions`）", () => {
  it("那台没有控制通道 → 空数组（不猜）", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    expect(await fetchSessionAccounts("devbox")).toEqual([]);
  });
  it("后端答了几行 → 解回几行；问的是那台机器、那条帧命令、请求体是空对象", async () => {
    invokeMock.mockImplementation(withHistoryReads((cmd: string, args: unknown) =>
      Promise.resolve(
        isChanCall(cmd, args, "accounts-sessions")
          ? linesReply([{ pid: 1, sessionId: "s", cwd: "/w", configDir: null, account: null, bare: true, alive: true }])
          : undefined,
      ),
    ));
    const s = await fetchSessionAccounts("devbox");
    expect(s).toHaveLength(1);
    const call = invokeMock.mock.calls.find((c) => c[0] === "chan_call");
    const args = call?.[1] as ChanCallArgs;
    expect(args.origin).toBe("devbox");
    expect(args.op).toBe("accounts-sessions");
    expect(chanArgsJson(args)).toEqual({});
    expect(args.leftMs, "期限由调用方给（30 秒那一档），不是库里的默认").toBeGreaterThan(29_000);
  });
  it("本机也走同一条路（`<local>` 就是一个 origin）", async () => {
    invokeMock.mockImplementation(withHistoryReads((cmd: string, args: unknown) =>
      Promise.resolve(isChanCall(cmd, args, "accounts-sessions") ? linesReply([]) : undefined),
    ));
    expect(await fetchSessionAccounts("<local>")).toEqual([]);
    const args = invokeMock.mock.calls.find((c) => c[0] === "chan_call")?.[1] as ChanCallArgs;
    expect(args.origin).toBe("<local>");
    expect(
      invokeMock.mock.calls.some((c) => /session_accounts/.test(String(c[0]))),
      "退役的那两条 Tauri 命令又被调了",
    ).toBe(false);
  });
});

// 「停本机后端之前数一数会断几条」那一问（`settings/backend-section.ts` 用）：现问、不走缓存，问不到就是 null ——
//   空表在这里的意思是「没有会话会断」，把失败折成空表就是一句假话。
// 逐行解释从 Rust（`accounts.rs::SessionAccount` 的 serde）搬到 `parseSessionAccountLines`，
//   Rust 那侧两条金样（`accounts_tests.rs` 原来那两条）逐字节搬到这里。
describe("parseSessionAccountLines（`--session-accounts` 的逐行）", () => {
  it("一行逐格读回（裸起的账号 0）", () => {
    const rows = parseSessionAccountLines([
      `{"pid":66936,"sessionId":"9d66c46d","cwd":"/w","configDir":null,"account":null,"bare":true,"alive":true}`,
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0].pid).toBe(66936);
    expect(rows[0].bare).toBe(true);
    expect(rows[0].alive).toBe(true);
    expect(rows[0].account).toBeNull();
  });
  it("坏行跳过、不毁整次：不是 JSON / 不是对象 / 缺 pid / 某格类型不对", () => {
    const rows = parseSessionAccountLines([
      "not json",
      "[1,2]",
      `{"sessionId":"no-pid"}`,
      `{"pid":2,"bare":"yes"}`,
      `{"pid":4,"viaRelay":"yes"}`,
      `{"pid":3}`,
    ]);
    expect(rows.map((r) => r.pid)).toEqual([3]);
    expect(rows[0]).toEqual({
      pid: 3,
      sessionId: null,
      cwd: null,
      configDir: null,
      account: null,
      bare: false,
      alive: false,
      // 老后端没有这个键 ⇒ null（不知道），不是坏行。
      viaRelay: null,
    });
  });
  it("〔HX1 · D-f〕`viaRelay`：true / false / null 逐字带回；缺 ⇒ null；类型不对 ⇒ 整行坏", () => {
    const [t, f, n, gone] = parseSessionAccountLines([
      `{"pid":1,"alive":true,"viaRelay":true}`,
      `{"pid":2,"alive":true,"viaRelay":false}`,
      `{"pid":3,"alive":true,"viaRelay":null}`,
      `{"pid":4,"alive":true}`,
    ]);
    expect([t.viaRelay, f.viaRelay, n.viaRelay, gone.viaRelay]).toEqual([true, false, null, null]);
    expect(parseSessionAccountLines([`{"pid":5,"viaRelay":1}`])).toEqual([]);
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
    const b = sessionBadge("s1", "devbox", m, emailBy, new Map([["s1", "b"]]));
    expect(b?.text).toBe("z");
    expect(b?.tooltip).toMatch(copyPattern("accounts.sessionBadge.live"));
  });
  it("无 live 但有 lastAccount → 用源②，标注上次 + 带邮箱", () => {
    const b = sessionBadge("s1", "devbox", new Map(), emailBy, new Map([["s1", "b"]]));
    expect(b?.known).toBe(true);
    expect(b?.text).toBe("b");
    expect(b?.tooltip).toMatch(copyPattern("accounts.sessionBadge.last"));
    expect(b?.tooltip).toContain("b@y.com");
  });
  it("live 存在但已死 + 有 lastAccount → 回退源②", () => {
    const m = live([
      { pid: 1, sessionId: "s1", cwd: "/w", configDir: "/h/z", account: "z", bare: false, alive: false },
    ]);
    const b = sessionBadge("s1", "devbox", m, emailBy, new Map([["s1", "b"]]));
    expect(b?.text).toBe("b");
    expect(b?.tooltip).toContain(copyText("resumeMenu.account.last"));
  });
  it("都无 → —（含不传 lastAccountByS 也安全）", () => {
    expect(sessionBadge("s1", "devbox", new Map(), emailBy, new Map())?.text).toBe("—");
    expect(sessionBadge("s1", "devbox", new Map(), emailBy)?.text).toBe("—");
  });
  it("本地会话（origin null）源②也不显", () => {
    expect(sessionBadge("s1", LOCAL_ORIGIN, new Map(), emailBy, new Map([["s1", "b"]]))).toBeNull();
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
    acct({ name: "0", configDir: null, mode: "bare", loggedIn: true, exists: true });

  it("暂不可选：从 UI 起它需要 unset 注入，launch-plan 今天只会 export", () => {
    expect(isSelectable(zero())).toBe(false);
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
// `isSelectable` 为假 ⇒ `selectableAccounts` 不收它 ⇒ 不能设为当前号 ·
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
      // 两个 Rust 生产者都会填它（值由 `acct_core::auth_ready` 算）。
      authReady: true,
    });
  /** 订阅号，**同样没有凭据文件** —— `KAY3` 的阴性对照。 */
  const subNoCred = (name = "sub") =>
    acct({
      name,
      configDir: `/h/.claude-alt/${name}`,
      loggedIn: false,
      authKind: "subscription",
      authReady: false,
    });

  it("★ KAY2②：isSelectable 为真", () => {
    expect(isSelectable(apiKey())).toBe(true);
  });

  it("★ KAY3：订阅号缺凭据 —— isSelectable 仍为**假**（这道保护不许被一起放宽）", () => {
    expect(isSelectable(subNoCred())).toBe(false);
    // 连带：它进不了可选列表。
    const st = state({ accounts: [subNoCred()] });
    expect(selectableAccounts(st)).toEqual([]);
  });

  it("★ KAY2 级联①：selectableAccounts 收它", () => {
    const st = state({ accounts: [apiKey(), subNoCred()] });
    expect(selectableAccounts(st).map((a) => a.name)).toEqual(["api"]);
  });

  it("★ KAY2 级联③：能当当前号 / 上徽章（跟随选中它由那台后端判，见后端判号那一族）", () => {
    const st = state({ accounts: [apiKey()] });
    expect(currentWorkingAccount(st)?.name).toBe("api");
    expect(currentAccountForBadge(st)?.name).toBe("api");
    // 阴性对照同一格：订阅号缺凭据时落空。
    const bad = state({ accounts: [subNoCred()] });
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

  it("★〔DUP1〕只读后端算好的 authReady，不看 loggedIn（两个方向各一格）", () => {
    // 规则的唯一住址是 `acct_core::auth_ready`；这两格原来是可缺的，缺了由一个 TS 包装回落到 loggedIn ——
    // 那是订阅分支在 TS 里的第二份（登记表 `tests/frontend/ui/judgment-single-home.vitest.ts` J1）。
    // 解码器早已逐键要求这两格，回落不可达，包装删了 ⇒ 两格与 loggedIn 反着给，结论跟 authReady 走。
    expect(isSelectable(acct({ name: "o", loggedIn: true, authReady: false }))).toBe(false);
    expect(isSelectable(acct({ name: "o", loggedIn: false, authKind: "api-key", authReady: true }))).toBe(true);
  });
});

describe("K-A1 KA6a：api-key 号的 UI 文案不许说「已登录」", () => {
  const apiKey = () =>
    acct({ name: "api", loggedIn: false, authKind: "api-key", authReady: true });

  it("★ 徽章写「api-key（未配置端点）」而不是「已登录」", () => {
    const b = accountStatusBadge(apiKey());
    expect(b.text).toBe(copyText("accounts.badge.apikeyNoEndpoint"));
    expect(b.text).not.toContain(copyText("accounts.badge.signedIn"));
    expect(b.warn).toBe(true);
    // hover 要把「选得中、起得来、但请求发不出去」这件事说清（不是一句「不可用」）。
    expect(b.title).toMatch(copyPattern("accounts.badge.apikeyNoEndpointHint"));
  });

  it("★ 有凭据文件的 api-key 号也一样 —— 它压根不看那个文件", () => {
    const b = accountStatusBadge(
      acct({ name: "api", loggedIn: true, authKind: "api-key", authReady: true }),
    );
    expect(b.text).toBe(copyText("accounts.badge.apikeyNoEndpoint"));
  });

  it("订阅号那三态一格没变（阴性对照）", () => {
    expect(accountStatusBadge(acct({})).text).toBe(copyText("accounts.badge.signedIn"));
    expect(accountStatusBadge(acct({})).warn).toBe(false);
    expect(accountStatusBadge(acct({ loggedIn: false, authReady: false })).text).toBe(copyText("accounts.badge.notSignedIn"));
    expect(accountStatusBadge(acct({ mode: "in-place" })).text).toBe(copyText("accounts.badge.inPlace"));
  });

  it("逃生口优先于 api-key（in-place 压根不支持切号，先说那件事）", () => {
    const b = accountStatusBadge(
      acct({ mode: "in-place", authKind: "api-key", authReady: true }),
    );
    expect(b.text).toBe(copyText("accounts.badge.inPlace"));
  });
});

describe("K-H2b KH2B7：api-key 号那一格的三态，与「实现的三态」逐格对拍", () => {
  const apiKey = () =>
    acct({ name: "acct-a", loggedIn: false, authKind: "api-key", authReady: true });

  // ★★ 本 describe 存在的理由，逐字：**本件落地那一刻，那句 hover 就对一部分号成了假话**
  //（本机、apikey 表里有它那一行、中转在跑的那些号，cc-monitor **真的**会替它配 base URL）。
  // 而「改了事实没改说它的那句话」是本区花过六轮的那一族（风险 6v / 裁定 K20）。
  // ⇒ 这里把**实现的三态**与**徽章的三态**钉成一一对应：少一格、串一格，都红。

  it("★ 表里有这一行 · 中转在跑 ⇒ 「经中转」，且不再是警示态", () => {
    const b = accountStatusBadge(apiKey(), { hasRow: true, running: true });
    expect(b.text).toBe(copyText("accounts.badge.apikeyRelayed"));
    expect(b.warn).toBe(false);
    // 它保证的是哪一截，必须写在 hover 里 —— 不许暗示「这个 key 一定能用」。
    // 按文案键断言，不按原文：原先钉着「ANTHROPIC_BASE_URL」，那是配置键名直出（R1），与 CP1 裁词相冲。
    expect(b.title).toBe(copyText("accounts.badge.apikeyRelayedHint"));
  });

  it("★ 表里有这一行 · 中转没跑 ⇒ 「中转未运行」，且说明会被当场拒", () => {
    const b = accountStatusBadge(apiKey(), { hasRow: true, running: false });
    expect(b.text).toBe(copyText("accounts.badge.apikeyRelayDown"));
    expect(b.warn).toBe(true);
    // `KH2B2`②：这一条**不许**被说成静默失败 —— 起会话那一侧会当场拒。
    // 按文案键断言，不按原文（原先钉着「当场拒」三个字，改说法就红）。
    expect(b.title).toBe(copyText("accounts.badge.apikeyRelayDownHint"));
  });

  it("★ 表里没有这一行 ⇒ 仍是「未配置端点」，而且说得出**为什么**", () => {
    const b = accountStatusBadge(apiKey(), { hasRow: false, running: true });
    expect(b.text).toBe(copyText("accounts.badge.apikeyNoEndpoint"));
    expect(b.title).toContain(copyText("accounts.badge.whyNoRow"));
    // 阴性对照：本机远端同一条路，不许再说成「远端不做」。
    expect(b.title).not.toContain(copyText("rsConfigSurface.host.remote"));
  });

  it("★ 远端那一台与本机同一条路：那台答的事实成立 ⇒ 同样是「经中转」，hover 不说「本机」也不说「远端不做」", () => {
    // 远端那台起的会话由那台的 ccm 定往哪发、那台的中转按那台 key 表里这一行换上 key（与本机同一条路）。
    const b = accountStatusBadge(apiKey(), { hasRow: true, running: true });
    expect(b.text).toBe(copyText("accounts.badge.apikeyRelayed"));
    expect(b.warn).toBe(false);
    for (const st of [{ hasRow: true, running: true }, { hasRow: true, running: false }, { hasRow: false, running: true }]) {
      const t = accountStatusBadge(apiKey(), st).title;
      expect(t).not.toContain("本机");
      expect(t).not.toContain(copyText("rsConfigSurface.host.remote"));
    }
  });

  it("★ 调用方没说是哪一半 ⇒ **不替它下判断**，只把两条前置说清", () => {
    const b = accountStatusBadge(apiKey());
    expect(b.text).toBe(copyText("accounts.badge.apikeyNoEndpoint"));
    expect(b.title).toContain(copyText("accounts.badge.whyUnknown"));
    expect(b.title).toContain(copyText("accounts.badge.whyUnknown"));
    // ⚠ 这一档**不许**断言「表里没有这一行」——那是它看不见的事实。
    expect(b.title).not.toContain(copyText("accounts.badge.whyNoRow"));
  });

  it("★ 那句已经成假的话，三档里一句都不许再出现（分母 = 我列的这 3 档 + 缺席）", () => {
    // 逐字：本件之前的原文是「cc-monitor 今天还不会替它配 API key 与 base URL」。
    const LIE = "今天还不会替它配 API key 与 base URL";
    const states: Array<Parameters<typeof accountStatusBadge>[1]> = [
      undefined,
      { hasRow: false, running: false },
      { hasRow: true, running: false },
      { hasRow: true, running: true },
    ];
    // 非空对照：先证明这把尺子认得出那句话（否则下面整个循环可能只是因为 needle 打错而全绿）。
    expect("cc-monitor " + LIE).toContain(LIE);
    for (const st of states) {
      expect(accountStatusBadge(apiKey(), st).title).not.toContain(LIE);
      // 顺带：任何一档都不许说成「已登录」（KA6a 的原话，人群扩到了新那几档）。
      expect(accountStatusBadge(apiKey(), st).text).not.toContain(copyText("accounts.badge.signedIn"));
    }
  });

  // ★★★ 规则那一段：一份**后端读数**（`ApikeyRoutingView`）怎么落到某一个账号上，
  // 以及三档**真的分得开**。喂进来的是读数的形状，**不是**直接喂 `{hasRow,running}`
  // —— 后者会把 `apikeyEndpointStateFor` 那一格整个绕过去。
  //
  // ⚠ **取数那一跳（`invoke`）今天还没接上**，卡点写在 `accounts.ts` 那段头注里
  // （`tests/frontend/ui/ipc/commands.vitest.ts` 的两个钉死计数不在本件写区）。⇒ 本组买的是**规则**，
  // 不是「界面上真的显出来了」。
  it("★ 产出方：经通道问 `apikey-routing`，入参是 agent ＋ 那几个 configDir（〔US1〕）", async () => {
    invokeMock.mockResolvedValue(chanReply({ routed: ["/h/.claude-alt/acct-a"], running: true }));
    const got = await fetchMachineApikeyRouting(LOCAL_ORIGIN, ["/h/.claude-alt/acct-a", "/h/.claude-alt/acct-b"]);
    // 帧命令名打错在生产上是**运行时**那台后端回 unsupported（不是编译错）⇒ 在这里钉死它。
    const calls = invokeMock.mock.calls;
    expect(calls.map((c) => c[0])).toEqual(["chan_call"]);
    const a = calls[0][1] as ChanCallArgs;
    expect([a.origin, a.op]).toEqual(["<local>", "apikey-routing"]);
    expect(chanArgsJson(a)).toEqual({
      agent: "claude-code",
      configDirs: ["/h/.claude-alt/acct-a", "/h/.claude-alt/acct-b"],
    });
    expect(got).toEqual({ routed: ["/h/.claude-alt/acct-a"], running: true });
  });

  it("★★ 走真产出方 → 三档：读数从那条命令来，三个账号落到三个不同的徽章上", async () => {
    // ⚠ 与下面那条的差别就是**这一格**：这里的 routing 是 `fetchMachineApikeyRouting` 的返回值
    //（即那条命令的产物），不是判据手写的字面量 ⇒ 命令名 / 入参 / 字段名任一处坏掉，这里就散。
    invokeMock.mockResolvedValue(chanReply({ routed: ["/h/.claude-alt/acct-a"], running: true }));
    const routing = await fetchMachineApikeyRouting(LOCAL_ORIGIN, [
      "/h/.claude-alt/acct-a",
      "/h/.claude-alt/acct-b",
    ]);
    const withDir = (name: string, dir: string | null) =>
      acct({ name, configDir: dir, loggedIn: false, authKind: "api-key", authReady: true });
    const a = withDir("acct-a", "/h/.claude-alt/acct-a");
    const b = withDir("acct-b", "/h/.claude-alt/acct-b");
    expect(accountStatusBadge(a, apikeyEndpointStateFor(a, routing)).text).toBe(copyText("accounts.badge.apikeyRelayed"));
    expect(accountStatusBadge(b, apikeyEndpointStateFor(b, routing)).text).toBe(
      copyText("accounts.badge.apikeyNoEndpoint"),
    );
    // 非空对照：同一条产出方、只把 `running` 翻过来 ⇒ 第三档真的分得开。
    invokeMock.mockResolvedValue(chanReply({ routed: ["/h/.claude-alt/acct-a"], running: false }));
    const stopped = await fetchMachineApikeyRouting(LOCAL_ORIGIN, ["/h/.claude-alt/acct-a"]);
    expect(accountStatusBadge(a, apikeyEndpointStateFor(a, stopped)).text).toBe(
      copyText("accounts.badge.apikeyRelayDown"),
    );
  });

  it("★ 读数 → 三档：同一份读数，三个账号落到三个不同的徽章上", () => {
    const routing = { routed: ["/h/.claude-alt/acct-a"], running: true };
    const withDir = (name: string, dir: string | null) =>
      acct({ name, configDir: dir, loggedIn: false, authKind: "api-key", authReady: true });

    // ① 表里有这一行 + 中转在跑 ⇒ 「经中转」。
    const a = withDir("acct-a", "/h/.claude-alt/acct-a");
    expect(accountStatusBadge(a, apikeyEndpointStateFor(a, routing)).text).toBe(copyText("accounts.badge.apikeyRelayed"));
    // ② 表里没有这一行 ⇒ 仍是「未配置端点」，而且说得出为什么。
    const b = withDir("acct-b", "/h/.claude-alt/acct-b");
    const bb = accountStatusBadge(b, apikeyEndpointStateFor(b, routing));
    expect(bb.text).toBe(copyText("accounts.badge.apikeyNoEndpoint"));
    expect(bb.title).toContain(copyText("accounts.badge.whyNoRow"));
    // ③ 同一个账号、只把「中转在不在跑」翻过来 ⇒ 第三档（非空对照：两档真的分得开）。
    const stopped = { routed: ["/h/.claude-alt/acct-a"], running: false };
    expect(accountStatusBadge(a, apikeyEndpointStateFor(a, stopped)).text).toBe(copyText("accounts.badge.apikeyRelayDown"));
    // ④ 账号 0（没有 configDir）⇒ 推不出 id ⇒ **不表态**，回落到缺席那一档。
    const zero = withDir("0", null);
    expect(apikeyEndpointStateFor(zero, routing)).toBeUndefined();
    expect(accountStatusBadge(zero, apikeyEndpointStateFor(zero, routing)).title).toContain(
      copyText("accounts.badge.whyUnknown"),
    );
  });

  it("★ 订阅号一格不受影响（阴性对照：新参数不许改到别的 kind）", () => {
    for (const st of [undefined, { hasRow: false, running: false } as const, { hasRow: true, running: true } as const]) {
      expect(accountStatusBadge(acct({}), st).text).toBe(copyText("accounts.badge.signedIn"));
      expect(accountStatusBadge(acct({ loggedIn: false, authReady: false }), st).text).toBe(copyText("accounts.badge.notSignedIn"));
      expect(accountStatusBadge(acct({ mode: "in-place" }), st).text).toBe(copyText("accounts.badge.inPlace"));
    }
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

describe("launch-account：要哪个号 · 那台说选不了时给的那个选择", () => {
  const refusal = (code: string, data?: unknown): Uint8Array =>
    new TextEncoder().encode(JSON.stringify(data === undefined ? { code, message: "m" } : { code, message: "m", data }));
  const U: AccountUnavailable = { requested: "z", pinned: true, listKnown: true, alternative: "b" };

  it("点名那一格：账号 0 ⇒ base；名字 ⇒ 按名字（线上只有名字那一形）", () => {
    expect(chosenAccount(null)).toEqual({ kind: "base" });
    expect(chosenAccount("z")).toEqual({ kind: "named", name: "z" });
  });

  it("认得出「那台说要的号选不了」：通道层 / 控制层两种错都认；别的码 · 形状不对 · 别的层 ⇒ 不认", () => {
    const peer = (body: Uint8Array) => ({ layer: "peer" as const, why: "refused" as const, body });
    expect(accountUnavailableOf(new ChanError(peer(refusal("account_unavailable", U))))).toEqual(U);
    expect(accountUnavailableOf(new ControlError("s", "d", peer(refusal("account_unavailable", U))))).toEqual(U);
    expect(accountUnavailableOf(new ChanError(peer(refusal("refused"))))).toBeNull();
    expect(accountUnavailableOf(new ChanError(peer(refusal("account_unavailable", { requested: 1 }))))).toBeNull();
    expect(accountUnavailableOf(new ChanError(peer(refusal("account_unavailable", null))))).toBeNull();
    expect(accountUnavailableOf(new ChanError({ layer: "ours", why: "Cancelled" }))).toBeNull();
    expect(accountUnavailableOf(new Error("x"))).toBeNull();
  });

  it("四种说法照那一形分：上次的号 / 点名的号 × 有替代 / 只能不指定；清单读不出 ⇒ 说读不到、只给「不指定」", () => {
    const said = (u: AccountUnavailable): string => {
      vi.mocked(showActionFailureToast).mockClear();
      refuseUnavailableAccount({ machine: "devbox", u, choose: vi.fn() });
      return vi.mocked(showActionFailureToast).mock.calls[0][1] as string;
    };
    expect(said(U)).toBe(copyText("accountPick.refused.pinGoneToCurrent", { name: "z", current: "b" }));
    expect(said({ ...U, alternative: null })).toBe(copyText("accountPick.refused.pinGoneToBase", { name: "z" }));
    expect(said({ ...U, pinned: false })).toBe(copyText("accountPick.refused.explicitGoneToCurrent", { name: "z", current: "b" }));
    expect(said({ ...U, pinned: false, alternative: null })).toBe(copyText("accountPick.refused.explicitGoneToBase", { name: "z" }));
    expect(said({ ...U, listKnown: false })).toBe(copyText("accountPick.refused.listUnknown", { machine: "devbox", name: "z" }));
    // 点了 ⇒ 以点名再起：有替代 ⇒ 那个号；清单读不出 / 没有替代 ⇒ 账号 0。
    for (const [u, want] of [
      [U, { kind: "named", name: "b" }],
      [{ ...U, listKnown: false }, { kind: "base" }],
    ] as const) {
      vi.mocked(showActionFailureToast).mockClear();
      const choose = vi.fn();
      refuseUnavailableAccount({ machine: LOCAL_ORIGIN, u, choose });
      (vi.mocked(showActionFailureToast).mock.calls[0][2] as { onClick: () => void }).onClick();
      expect(choose).toHaveBeenCalledWith(want);
    }
  });
});
