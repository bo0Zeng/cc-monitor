// account-ux U7：设置「账号」组的 IA / 渲染分支测试（vitest + jsdom）。
//
// 重点不是"长得好不好看"，而是两件会真伤人的事：
//   ① 四条**降级分支**（无远端 / daemonless / 老 daemon / 未启用）的 DOM 与文案不能被 IA 重排改掉；
//   ② 维护区（加账号 / 补链，都会动远端目录）**必须默认折叠**，不能常驻摊在手边。
// U6 的教训：断言要锚在真契约上，并对关键属性做变异验证（故意改坏看会不会红）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const readRemoteConfigMock = vi.fn();
const fetchAccountsMock = vi.fn();
/**
 * `N-F1b`：本机那条读口的桩。
 *
 * 🔴 它**必须有一个默认返回值**（见下面 `beforeEach`）：本件之后，
 * 「没有配任何远端」那一支不再是一句静态说明，而是真的去调 `fetchLocalAccounts`。
 * 不给默认值 ⇒ 真函数被调 ⇒ 它会去 `invoke("list_local_accounts")`，
 * 而那条路在 jsdom 里的结局取决于 `invokeMock` 这一刻恰好被设成什么
 * —— 那种绿是**跟着别的测试的设置漂**的绿。
 */
const fetchLocalAccountsMock = vi.fn();
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../remote-config", () => ({ readRemoteConfig: () => readRemoteConfigMock() }));

import { readFileSync } from "node:fs";
import { AccountsSection, renderRelayKeyBlock } from "./accounts-section";
// `N-F2`：账本与那张清单 —— 本文件末尾那一族要断的正是「面板跑完之后账本里是什么」，
// 所以取的是**真的** `readStatus` / `computeGaps`，一个桩都不架。
import { readStatus, LOCAL_MACHINE_KEY, type MachineStatus } from "./machine-status";
import { computeGaps, summarizeGaps } from "./readiness";
import type { RelayCredentialsStatus } from "../ipc/commands";
import { showActionFailureToast } from "../error-toast";
import * as accounts from "../accounts";
import type { AccountsState, Account } from "../accounts";

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
    notice: null,
    meta: {
      enabled: true,
      acctsDir: "/a",
      manifestPath: "/a/accounts.json",
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
/**
 * `N-F1b`：本机那条路的 fixture —— `origin` 是那个哨兵，**不是**某台远端的名字。
 * 形状与远端那份逐字段相同（`fetchLocalAccounts` 头注：两条路填的是同一个 Rust 结构体）。
 */
function localState(p: Partial<AccountsState> = {}): AccountsState {
  return state({ origin: accounts.LOCAL_ORIGIN, ...p });
}
const host = (p: Record<string, unknown> = {}) => ({
  label: "aya",
  host: "h",
  port: 22,
  user: "u",
  keyPath: "",
  daemonPath: "",
  hostKeyFingerprint: "",
  addresses: [],
  jump: "",
  daemonless: false,
  ...p,
});

/** 建 section 并等它的两段 async（init → reload）落定。 */
async function mount(): Promise<HTMLElement> {
  const s = new AccountsSection();
  document.body.innerHTML = "";
  document.body.appendChild(s.element);
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
  return s.element;
}

beforeEach(() => {
  vi.restoreAllMocks();
  readRemoteConfigMock.mockReset().mockResolvedValue({ enabled: true, hosts: [host()] });
  invokeMock.mockReset().mockResolvedValue(undefined);
  fetchAccountsMock.mockReset();
  // `N-F1b`：默认给「这台机一个隔离账号都没有」——最保守的一档，
  // 想量别的态的用例自己在里面覆盖掉它。
  fetchLocalAccountsMock.mockReset().mockResolvedValue(localState({ accounts: [] }));
  vi.spyOn(accounts, "fetchAccounts").mockImplementation(() => fetchAccountsMock());
  vi.spyOn(accounts, "fetchLocalAccounts").mockImplementation(() => fetchLocalAccountsMock());
  vi.spyOn(accounts, "invalidateAccountsCache").mockImplementation(() => {});
});

/** 降级态**一律**不该长出 ready 态的三件套（表 / 横幅 / 维护区）——这正是 IA 重排最该防的回归。 */
function expectNoReadyChrome(el: HTMLElement): void {
  expect(el.querySelector(".accounts-table")).toBeNull();
  expect(el.querySelector(".accounts-current-banner")).toBeNull();
  expect(el.querySelector(".accounts-maint-wrap")).toBeNull();
}

describe("account-ux U7 设置账号组：降级分支不被 IA 重排改掉", () => {
  /**
   * ⚠⚠ `N-F1b` `NF1bD3`：**这一条的题面被 `N-F1b` 正面推翻，逐字换过。**
   *
   * 旧题（逐字）：`没有已配置的远端 → 只给一句说明，不渲染表/横幅/维护区`
   * 旧断言里被推翻的那一句（逐字）：
   *   `expect(el.querySelector(".accounts-info")?.textContent).toContain("没有已配置的远端");`
   *
   * 为什么改：`N-F1b` 做的正是「没有远端时不再只给一句说明，而是列出**这台机器**的账号」
   * ⇒ 那一句断言从「守住降级态」变成了「**钉住那个洞**」，不改它这一件就做不成。
   *
   * 换成什么：这一条**只留还成立的那一半**，并且换到「远端那一支照旧」的口径上 ——
   * 没有远端时，远端那三件套（表 / 横幅 / 维护区）一件都不该长出来，
   * 而且**远端那条读口一次都不该被调**（后者是新加的，旧版没有）。
   * 本机那一支**渲染成什么样**归 `NF1bD1` 那一族（本文件末尾），
   * 这里只留一个「它确实走了本机那条路」的非空对照，免得整块空着也让上面三条恒真。
   */
  it("没有已配置的远端 → 远端那三件套一件不出、远端读口一次不调（本机那一支归 NF1bD1）", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    const el = await mount();
    expectNoReadyChrome(el);
    expect(
      fetchAccountsMock,
      "没有配任何远端，却去调了远端那条读口 —— 那是拿一个不存在的 origin 去问远端",
    ).not.toHaveBeenCalled();
    // 非空对照：这一屏不是空的，它走的是本机那一支（内容由 NF1bD1 那一族钉）。
    expect(
      el.querySelector(".accounts-local"),
      "本机那一支整块没渲染 —— 上面三条会在一屏空白上恒真",
    ).not.toBeNull();
  });

  it("daemonless 远端 → 安静说明，不渲染表", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({ available: false, error: "该远端配置为 daemonless" }),
    );
    const el = await mount();
    expect(el.querySelector(".accounts-info")?.textContent).toContain("daemonless");
    expectNoReadyChrome(el);
  });

  it("老 daemon（不支持账号）→ 提示需更新，不渲染表", async () => {
    fetchAccountsMock.mockResolvedValue(state({ available: false, error: "daemon 过旧" }));
    const el = await mount();
    expect(el.querySelector(".accounts-info")?.textContent).toContain("需要更新");
    expectNoReadyChrome(el);
  });

  it("未启用多账号 + cc-acct-iso 已装 → 走部署向导，不渲染表", async () => {
    fetchAccountsMock.mockResolvedValue(state({ accounts: [] }));
    invokeMock.mockResolvedValue({ installed: true }); // check_remote_acct_iso：已装
    const el = await mount();
    expect(el.querySelector(".accounts-not-enabled")).not.toBeNull();
    expect(el.querySelector(".accounts-wizard")).not.toBeNull();
    expect(el.querySelector(".accounts-needs-deploy")).toBeNull();
    expectNoReadyChrome(el);
  });

  it("F5：未启用 + cc-acct-iso 未装 → 显一键部署（而非直接甩 init 向导）", async () => {
    fetchAccountsMock.mockResolvedValue(state({ accounts: [] }));
    invokeMock.mockResolvedValue({ installed: false }); // check_remote_acct_iso：没装
    const el = await mount();
    const deploy = el.querySelector(".accounts-needs-deploy");
    expect(deploy).not.toBeNull();
    expect(deploy?.textContent).toContain("一键部署 cc-acct-iso");
    expect(el.querySelector(".accounts-wizard")).toBeNull(); // 没装时不该甩 init 向导
    expectNoReadyChrome(el);
  });

  it("F5：探测 cc-acct-iso 失败 → 不堵死用户，回退 init 向导", async () => {
    fetchAccountsMock.mockResolvedValue(state({ accounts: [] }));
    invokeMock.mockRejectedValue(new Error("ssh down")); // check 抛错
    const el = await mount();
    expect(el.querySelector(".accounts-wizard")).not.toBeNull();
    expect(el.querySelector(".accounts-needs-deploy")).toBeNull();
  });

  it("拉账号抛错 → 一句失败说明，不炸", async () => {
    fetchAccountsMock.mockRejectedValue(new Error("ssh down"));
    const el = await mount();
    expect(el.querySelector(".accounts-info")?.textContent).toContain("ssh down");
    expectNoReadyChrome(el);
  });
});

describe("account-ux U7 已启用态：横幅 / 表格 / 维护区", () => {
  // fixture 名必须落**不同**色槽，否则"设置里的颜色 == chip/tab 的颜色"这条断言是弱绿
  //（旧 fixture "z"/"b" 恰好都是槽 5，把实现改成永远取同一个名字也照样过）。
  const A = "wei"; // 槽 0
  const B = "amy"; // 槽 6
  const ready = (over: Partial<AccountsState> = {}): AccountsState =>
    state({
      accounts: [acct({ name: A }), acct({ name: B, email: "amy@x.edu" })],
      defaultName: A,
      ...over,
    });

  it("前置：两个 fixture 落不同色槽（否则下面的颜色一致性断言是弱绿）", () => {
    expect(accountColorSlotFor(A)).not.toBe(accountColorSlotFor(B));
  });

  it("横幅显当前账号 + 实心头像 + 管辖范围", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    const banner = el.querySelector(".accounts-current-banner")!;
    expect(banner.querySelector(".accounts-current-name")?.textContent).toBe(A);
    expect(banner.querySelector(".acct-avatar")).not.toBeNull();
    expect(banner.querySelector(".acct-avatar.ghost")).toBeNull(); // 可用 → 实心
    expect(banner.textContent).toContain("正在跑的会话不受影响");
    expect(banner.classList.contains("unusable")).toBe(false);
  });

  it("当前账号不可选（未登录）→ 横幅如实说不可用 + 幽灵头像，不装作在生效", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: A, loggedIn: false })], defaultName: A }),
    );
    const el = await mount();
    const banner = el.querySelector(".accounts-current-banner")!;
    expect(banner.classList.contains("unusable")).toBe(true);
    expect(banner.textContent).toContain("不可用");
    expect(banner.querySelector(".acct-avatar.ghost")).not.toBeNull();
  });

  it("每行有账号头像，且与 chip/tab 同一套 hash 色槽（同名同槽）", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    const rows = el.querySelectorAll(".accounts-row");
    expect(rows.length).toBe(2);
    for (const [i, name] of [A, B].entries()) {
      const av = rows[i].querySelector(".acct-avatar")!;
      // 与 account-color 的槽位算法一致 → 设置里的头像颜色 == 状态栏/tab 上的颜色
      expect(av.className).toContain(`acct-c${accountColorSlotFor(name)}`);
    }
  });

  // 布局契约：styles.css 的 .accounts-table 定了 **8** 条列轨道（F10 加了用量列），行用
  // subgrid 继承。往 accountRow 里多 append 一个元素而不改 CSS，列就整体错位——jsdom 测不了
  // 布局，但能测这个数。
  it("每行子元素数 == grid 列数(8)：改一处必须改另一处", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    for (const row of el.querySelectorAll(".accounts-row")) {
      expect(row.children.length).toBe(8);
    }
  });

  // ---- K-A1 `KA6a`：api-key 号那一行的文案 ----
  //
  // ★ 这条**必须是 DOM 测试**，不能只测 `accountStatusBadge` 那个纯函数：
  // `KA6a` 要的是「用户真的看到了那句话」。取值收进 accounts.ts 之后，
  // 这一行**有没有接上**是另一件事 —— 纯函数全绿而 DOM 还渲染旧三态，
  // 用户看到的仍是「已登录」。
  it("★ KA6a：api-key 号的徽章写「api-key（未配置端点）」而不是「已登录」", async () => {
    fetchAccountsMock.mockResolvedValue(
      ready({
        accounts: [
          acct({ name: A }),
          acct({ name: B, loggedIn: false, authKind: "api-key", authReady: true }),
        ],
      }),
    );
    const el = await mount();
    const rows = [...el.querySelectorAll(".accounts-row")];
    const byName = (n: string) =>
      rows.find((r) => r.querySelector(".accounts-row-name")?.textContent === n)!;
    const apiBadge = byName(B).querySelector(".accounts-row-badge")!;
    expect(apiBadge.textContent).toBe("api-key（未配置端点）");
    expect(apiBadge.textContent).not.toContain("已登录");
    expect(apiBadge.classList.contains("warn")).toBe(true);
    // hover 得把「选得中、起得来、但请求发不出去」说清楚。
    expect(apiBadge.getAttribute("title") ?? "").toContain("鉴权失败");
    // 而「去登录」对它是假话 ⇒ 换成「打开终端」。
    const btns = [...byName(B).querySelectorAll(".accounts-row-actions button")].map(
      (b) => b.textContent,
    );
    expect(btns).toContain("打开终端");
    expect(btns).not.toContain("去登录");
    // 阴性对照同一格：订阅号那一行一个字没变。
    const subBadge = byName(A).querySelector(".accounts-row-badge")!;
    expect(subBadge.textContent).toBe("已登录");
    expect(subBadge.classList.contains("warn")).toBe(false);
  });

  // ---- `K-H2b` `KH2B7`：那句 hover 本件落地那一刻对一部分号成了假话 ----
  //
  // ★ 同样**必须是 DOM 测试**，理由与上一条逐字相同：纯函数那一侧接不接得上，
  // 是**另一件事**。`accountStatusBadge` 从本件起收第二个参数（这个号属于哪一半），
  // 而**这张表是远端专用的**（`reload` 在 `origin` 为空时直接早退，
  // 文案逐字「账号功能在远端 Linux 上」）⇒ 这里必须传 `{scope:"remote"}`。
  // 不传 ⇒ 渲染出来的是「不替它下判断」那一档，而这张表**判得出来**（它就是远端）。
  it("★ KH2B7：这张表是远端专用的 ⇒ api-key 那一行的 hover 要指名是**远端**那一半", async () => {
    fetchAccountsMock.mockResolvedValue(
      ready({
        accounts: [
          acct({ name: A }),
          acct({ name: B, loggedIn: false, authKind: "api-key", authReady: true }),
        ],
      }),
    );
    const el = await mount();
    const rows = [...el.querySelectorAll(".accounts-row")];
    const byName = (n: string) =>
      rows.find((r) => r.querySelector(".accounts-row-name")?.textContent === n)!;
    const title = byName(B).querySelector(".accounts-row-badge")!.getAttribute("title") ?? "";
    // 非空对照：这一格真的有 hover（不是空串上自问自答）。
    expect(title.length).toBeGreaterThan(20);
    // 正题：说清是哪一半 —— 只给本机配、远端这一半还不做。
    expect(title).toContain("远端");
    expect(title).toContain("本机");
    // ⚠ 那句本件落地后就成假的话，一个字都不许留在界面上（逐字原文）。
    expect(title).not.toContain("今天还不会替它配 API key 与 base URL");
    // ⚠ 也不许拿本机那条成因（「表里没有这一行」）去解释一个远端账号。
    expect(title).not.toContain("没有这个账号的一行");
  });

  it("★ KA6a 反面：缺凭据的订阅号仍写「未登录」（不许被 api-key 那一支一起放宽）", async () => {
    fetchAccountsMock.mockResolvedValue(
      ready({
        accounts: [acct({ name: A }), acct({ name: B, loggedIn: false, authKind: "subscription" })],
      }),
    );
    const el = await mount();
    const row = [...el.querySelectorAll(".accounts-row")].find(
      (r) => r.querySelector(".accounts-row-name")?.textContent === B,
    )!;
    expect(row.querySelector(".accounts-row-badge")?.textContent).toBe("未登录");
    expect([...row.querySelectorAll(".accounts-row-actions button")].map((b) => b.textContent)).toContain(
      "去登录",
    );
  });

  it("当前账号那行打 .current + ★", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    const cur = el.querySelector(".accounts-row.current")!;
    expect(cur.querySelector(".accounts-row-name")?.textContent).toBe(A);
    expect(cur.querySelector(".accounts-row-mark")?.textContent).toBe("★");
  });

  it("稳态（≥2 个账号）维护区默认折叠：加账号/补链会动远端，不该摊在手边", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    const wrap = el.querySelector<HTMLDetailsElement>("details.accounts-maint-wrap")!;
    expect(wrap).not.toBeNull();
    expect(wrap.open).toBe(false); // ← 变异验证锚点：改成恒定默认展开这里就红
    expect(wrap.querySelector("summary")?.textContent).toContain("维护");
    // A6 维护区内部功能仍在（只是被包进了 details）
    expect(wrap.querySelector(".accounts-maint-add")).not.toBeNull();
    expect(wrap.querySelector(".accounts-maint-ops")).not.toBeNull();
  });

  it("刚部署完只有 1 个账号 → 维护区默认展开（此时唯一的正路就是「加第二个账号」）", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: A })], defaultName: A }),
    );
    const el = await mount();
    const wrap = el.querySelector<HTMLDetailsElement>("details.accounts-maint-wrap")!;
    expect(wrap.open).toBe(true);
  });

  it("长 configDir 有 title 兜全文（列宽省略后仍可见）", async () => {
    const long = "/home/zbl/.claude-accts/some/very/deep/nested/path/for/account/z";
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "z", configDir: long })], defaultName: "z" }),
    );
    const el = await mount();
    const dir = el.querySelector<HTMLElement>(".accounts-row-dir")!;
    expect(dir.textContent).toBe(long);
    expect(dir.title).toBe(long);
  });
});

describe("F10：账号行用量单元格（懒加载 + 五种状态）", () => {
  const ready = (): AccountsState =>
    state({ accounts: [acct({ name: "z" })], defaultName: "z" });

  /** `account_usage` 走 invoke，与本文件其余 IPC（如 `check_remote_acct_iso`）共用同一个
   *  invokeMock——按命令名分流，别互相污染。 */
  function mockUsageInvoke(resp: { captured: boolean; raw?: string | null; error?: string | null }): void {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "account_usage"
        ? Promise.resolve({ captured: resp.captured, raw: resp.raw ?? null, error: resp.error ?? null })
        : Promise.resolve(undefined),
    );
  }
  const usageBtn = (el: HTMLElement): HTMLButtonElement | null =>
    el.querySelector<HTMLButtonElement>(".accounts-usage-btn");
  const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

  it("初始态是「查看用量」按钮，不自动探测", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    invokeMock.mockResolvedValue(undefined);
    const el = await mount();
    expect(usageBtn(el)?.textContent).toBe("查看用量");
    // invoke 只应有 check_remote_acct_iso 这类既有调用被间接触发过（本组件 init 时可能调用），
    // 断言的重点是 account_usage 这个命令名从未被叫到——不自动探测。
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "account_usage")).toBe(false);
  });

  it("点击后：查询中 → ok（含百分比+重置文案）", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    mockUsageInvoke({ captured: true, raw: "Current session\n  38%\nResets in 2h 14m" });
    const el = await mount();
    usageBtn(el)?.click();
    expect(el.querySelector(".accounts-usage-pending")?.textContent).toBe("查询中…");
    await flush();
    const outcome = el.querySelector(".accounts-usage-outcome");
    expect(outcome?.textContent).toContain("38%");
    expect(outcome?.textContent).toContain("重置");
  });

  it("not-logged-in → 明确短句 + 复制诊断文本按钮（判定基于猜测正则，可能误判）", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    mockUsageInvoke({ captured: true, raw: "Please sign in at console.anthropic.com" });
    const el = await mount();
    usageBtn(el)?.click();
    await flush();
    expect(el.querySelector(".accounts-usage-outcome")?.textContent).toContain("未登录");
    expect(el.querySelector(".accounts-usage-copy-raw")).not.toBeNull();
  });

  it("cli-missing → 明确短句 + 复制诊断文本按钮（判定基于猜测正则，可能误判）", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    mockUsageInvoke({ captured: true, raw: "bash: claude: command not found" });
    const el = await mount();
    usageBtn(el)?.click();
    await flush();
    expect(el.querySelector(".accounts-usage-outcome")?.textContent).toContain("没有 claude 命令");
    expect(el.querySelector(".accounts-usage-copy-raw")).not.toBeNull();
  });

  it("unrecognized → 短句 + 复制诊断文本按钮（不是空白）", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    mockUsageInvoke({ captured: true, raw: "╭─ 全新界面 ─╮" });
    const el = await mount();
    usageBtn(el)?.click();
    await flush();
    expect(el.querySelector(".accounts-usage-outcome")?.textContent).toContain("暂时读不到");
    expect(el.querySelector(".accounts-usage-copy-raw")).not.toBeNull();
  });

  it("probe-failed（Rust 层报错，如无 tmux）→ 显示原始错误文案", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    mockUsageInvoke({ captured: false, error: "远端未安装 tmux" });
    const el = await mount();
    usageBtn(el)?.click();
    await flush();
    expect(el.querySelector(".accounts-usage-outcome")?.textContent).toContain("远端未安装 tmux");
  });

  it("「刷新」按钮重新触发探测（force，不走缓存）", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    mockUsageInvoke({ captured: true, raw: "50%\nResets in 1h" });
    const el = await mount();
    usageBtn(el)?.click();
    await flush();
    const before = invokeMock.mock.calls.filter(([cmd]) => cmd === "account_usage").length;
    el.querySelector<HTMLButtonElement>(".accounts-usage-refresh")?.click();
    await flush();
    const after = invokeMock.mock.calls.filter(([cmd]) => cmd === "account_usage").length;
    expect(after).toBe(before + 1);
  });
});

describe("Z01 账号 0 在设置账号表里的呈现", () => {
  const zero = acct({ name: "0", configDir: null, mode: "bare", email: "me@x.edu" });

  it("账号 0 有一行，且路径列说的是它的真实含义（不是空白、不是空串）", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "z" }), zero], defaultName: "z" }),
    );
    const el = await mount();
    const dirs = [...el.querySelectorAll(".accounts-row-dir")].map((d) => d.textContent);
    expect(dirs).toHaveLength(2);
    expect(dirs[1]).toBe("（不设 CLAUDE_CONFIG_DIR）");
    expect(dirs[1]).not.toBe("");
  });

  // Z01 时这里钉的是「明说暂不支持」的占位；**Z03 把它做通了** ⇒ 契约变了，断言跟着变：
  // 账号 0 现在**真的会探**，而且载荷必须是 `unset CLAUDE_CONFIG_DIR; ` 打头（不是裸载荷）。
  // U8c-2a：载荷不再走 IPC（由 Rust 内核编译）⇒ 这里改钉「账号 0 的**表态**真的送出去了」。
  // 「显式 unset、绝不裸载荷」那条 fail-closed 纪律由
  // `backend::control::payload 的 usage_probe_payload_is_two_states_and_never_bare` 钉住（两态都断言带前缀）。
  it("账号 0 的用量会真的去探，且送的是账号 0 的显式表态（configDir === null）", async () => {
    fetchAccountsMock.mockResolvedValue(state({ accounts: [zero], defaultName: null }));
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "account_usage"
        ? Promise.resolve({ captured: true, raw: "42%\nResets in 2h", error: null })
        : Promise.resolve(undefined),
    );
    const el = await mount();
    el.querySelector<HTMLButtonElement>(".accounts-usage-btn")?.click();
    await new Promise((r) => setTimeout(r, 0));
    const calls = invokeMock.mock.calls.filter(([c]) => c === "account_usage");
    expect(calls).toHaveLength(1);
    const args = calls[0][1] as Record<string, unknown>;
    // 字面 null = 账号 0 的**显式**表态。省掉这个键在 Rust 侧同样落 `None`，
    // 但那是巧合不是契约 —— 钉字面 null 让「有没有表态」在这一层就可见。
    expect("configDir" in args).toBe(true);
    expect(args.configDir).toBeNull();
    expect(args).not.toHaveProperty("launchPayload");
  });

  it("降级说明会被渲染成显眼的一条（绝不静默）", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "z" })], notice: "远端 daemon 版本较旧：看不到账号 0" }),
    );
    const el = await mount();
    const warn = el.querySelector(".accounts-hint-warn");
    expect(warn?.textContent).toContain("账号 0");
  });

  it("变异反证：没有 notice 时不该冒出这条横幅", async () => {
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })] }));
    const el = await mount();
    expect(el.querySelector(".accounts-hint-warn")).toBeNull();
  });
});

describe("Z05：rc 片段一键生成（待贴文本，绝不代写）", () => {
  const FENCED =
    "# ===== BEGIN cc-acct-iso =====\nexport CLAUDE_CONFIG_DIR='/h/.claude-accts/z'\n" +
    "zcc() { CLAUDE_CONFIG_DIR='/h/.claude-accts/z' command claude \"$@\"; }\n" +
    "0cc() { env -u CLAUDE_CONFIG_DIR command claude \"$@\"; }\n# ===== END cc-acct-iso =====\n";

  const ready = (): AccountsState =>
    state({ accounts: [acct({ name: "z" })], defaultName: "z" });

  async function clickRc(resp: unknown, reject = false): Promise<HTMLElement> {
    fetchAccountsMock.mockResolvedValue(ready());
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "remote_acct_iso_shellinit"
        ? reject
          ? Promise.reject(new Error(String(resp)))
          : Promise.resolve(resp)
        : Promise.resolve(undefined),
    );
    const el = await mount();
    const btn = [...el.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === "生成 rc 片段…",
    );
    expect(btn, "维护区应有「生成 rc 片段…」按钮").toBeTruthy();
    btn!.click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    return el;
  }

  it("抓到完整片段 ⇒ 渲染成待贴块（走 T03 组件，不是手搓的复制按钮）", async () => {
    const el = await clickRc(FENCED);
    const block = el.querySelector(".paste-block");
    expect(block, "必须是 T03 的 paste-block").toBeTruthy();
    expect(el.querySelector<HTMLTextAreaElement>(".paste-block-out")?.value).toBe(FENCED);
  });

  it("★ 三句话都上屏（贴到哪 / 怎么合并 / 怎样才生效）", async () => {
    const el = await clickRc(FENCED);
    expect(el.querySelector(".paste-block-target")?.textContent).toContain(".bashrc");
    expect(el.querySelector(".paste-block-merge")?.textContent).toContain("追加");
    expect(el.textContent).toContain("source");
  });

  it("★ 半截片段（缺 END 围栏）⇒ 点复制被拦下并说明理由，且不碰剪贴板", async () => {
    // `invalidReason` 是**点复制时**才求值的（组件设计：拒绝时绝不把半成品写进剪贴板），
    // 所以断言要点到按钮上、读 toast，而不是去 DOM 里找文案。
    const toast = vi.mocked(showActionFailureToast);
    toast.mockClear();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const el = await clickRc("# ===== BEGIN cc-acct-iso =====\nzcc() { :; }\n");
    const copy = [...el.querySelectorAll<HTMLButtonElement>(".paste-block button")].find(
      (b) => b.textContent === "复制",
    );
    expect(copy, "待贴块应有复制按钮").toBeTruthy();
    copy!.click();
    await new Promise((r) => setTimeout(r, 0));
    const msgs = toast.mock.calls.map((c) => `${c[0]}|${c[1]}`).join(" ");
    expect(msgs).toContain("还不能贴");
    expect(msgs).toContain("片段不完整");
    expect(writeText, "被拒时绝不能碰剪贴板").not.toHaveBeenCalled();
  });

  it("抓取失败 ⇒ 不渲染任何待贴块（绝不给半截东西）", async () => {
    const el = await clickRc("远端没能产出 rc 片段", true);
    expect(el.querySelector(".paste-block")).toBeNull();
  });

  it("★ 绝不代写任何文件：整条路径上零 writeFile/写盘命令", async () => {
    await clickRc(FENCED);
    const written = invokeMock.mock.calls
      .map(([c]) => String(c))
      .filter((c) => /write|deploy|sftp|install/i.test(c));
    expect(written, `不该有任何写入类命令：${written.join(",")}`).toEqual([]);
  });
});

// 与 account-color.ts 同一套算法（测试里独立实现一遍，避免"照着实现抄"——
// 若实现改了槽位算法，这里会红，提醒设置/chip/tab 三处颜色会脱节）。
function accountColorSlotFor(name: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h % 8;
}

// ─────────────────────────────────────────────────────────────────────────────
// K-H2a：中转那把第三方 API key 的**前端那一半**（`KS6` 永不回显 / `KS9` 路径 /
// `KS11` 界面出声 / `KS7` 不进前端整份读写的那份配置）。
// ─────────────────────────────────────────────────────────────────────────────
describe("K-H2a：中转 API key 的前端一半", () => {
  // ⚠ 用 `process.cwd()` 相对路径而不是 `import.meta.url`：本仓 vitest 跑在仓根，
  //   而 `import.meta.url` 在这套 transform 下不是 file: scheme（实测 `The URL must be of scheme file`）。
  const src = () => readFileSync("src/settings/accounts-section.ts", "utf8");

  function status(p: Partial<RelayCredentialsStatus> = {}): RelayCredentialsStatus {
    return {
      configured: true,
      masked: "sk-a**********WXYZ",
      path: "/h/.claude/claudecode-frontend/relay-credentials.json",
      notice: null,
      problem: null,
      ...p,
    };
  }

  // `K-H2c`：那一块今天要**配给某一个账号**。默认给两个号，第一个已经在中转表里。
  // ⚠ 名字与 configDir 末段**刻意不同名**（`n1` vs `dir-one`）：断言里凡是用到 id 的地方，
  //   同名会让「前端拿名字当 id」与「后端从 configDir 推 id」两种实现**都绿**。
  const ACCTS = [
    { name: "n1", configDir: "/h/.claude-accts/dir-one", routed: true },
    { name: "n2", configDir: "/h/.claude-accts/dir-two", routed: false },
  ];

  it("KS6：输入框**从不预填** —— 已配置时也一样，要改就重新输", () => {
    const el = renderRelayKeyBlock(status(), ACCTS, () => {});
    const input = el.querySelector<HTMLInputElement>("input.relay-key-input");
    expect(input, "那个输入框不见了 —— 下面的断言会零命中地绿").toBeTruthy();
    expect(input!.value).toBe("");
    // 它是密码框（截图 / 录屏那两个出口）。
    expect(input!.type).toBe("password");
    // 非空对照：这一块**确实**知道「已经配过了」（不是整块空着才让上面恒真）。
    expect(el.textContent).toContain("已配置");
    expect(input!.placeholder).toContain("替换");
  });

  it("KS6：界面上只出现掩码，明文一个字节都进不来（类型上就没有那个字段）", () => {
    const el = renderRelayKeyBlock(status({ masked: "sk-a**********WXYZ" }), ACCTS, () => {});
    expect(el.textContent).toContain("sk-a**********WXYZ");
    // 明文那个值**根本递不进来** —— 这一条量的是类型面：多传一个字段 tsc 会红。
    // 行为面这里能量的是：整块里没有任何长得像完整 key 的东西（没有 `*` 的长串）。
    const looksLikePlaintext = /sk-[A-Za-z0-9-]{20,}/.test(el.textContent ?? "");
    expect(looksLikePlaintext, `界面上出现了像明文 key 的串：${el.textContent}`).toBe(false);
  });

  it("KS6 机检：本文件里那个输入框的 `.value` **只许被赋成空串**", () => {
    // 人群 = 本文件生产段里所有对 `input.value` 的赋值。
    const assigns = [...src().matchAll(/input\.value\s*=\s*([^;]+);/g)].map((m) => m[1].trim());
    expect(assigns.length, "一处赋值都没扫到 —— 抽取器坏了，本条在空转").toBeGreaterThan(0);
    for (const rhs of assigns) {
      expect(
        rhs,
        `输入框被赋了一个不是空串的值（${rhs}）—— 那就是回显。` +
          "KS6 逐字：一旦回显，key 就从「只住在后端」变成「每次打开那个界面都往前端传一遍」。",
      ).toBe('""');
    }
  });

  it("KS11：权限过宽时**在界面上出声**；没问题时不出声", () => {
    const warned = renderRelayKeyBlock(
      status({ notice: "同机器上的别人也读得到它（mode 是 0644…）。怎么修：跑 `chmod 600 …`" }),
      ACCTS,
      () => {},
    );
    const n = warned.querySelector(".relay-key-notice");
    expect(n, "过宽了却没在界面上显出来").toBeTruthy();
    expect(n!.textContent).toContain("chmod 600");
    // 非空对照：没问题时那一块**不该**出现（否则上面是恒真）。
    expect(
      renderRelayKeyBlock(status(), ACCTS, () => {}).querySelector(".relay-key-notice"),
    ).toBeNull();
  });

  it("KS9：那份文件的路径要显出来 —— 能手编但没人知道在哪 = 不能手编", () => {
    const el = renderRelayKeyBlock(status(), ACCTS, () => {});
    expect(el.textContent).toContain("relay-credentials.json");
    expect(el.querySelector(".relay-key-path")?.getAttribute("title")).toContain("编辑器");
  });

  it("文件读坏了要说出来，**不许静默当成「没配」**", () => {
    const el = renderRelayKeyBlock(
      status({ configured: false, masked: "", problem: "凭据文件不是合法 JSON（…）" }),
      ACCTS,
      () => {},
    );
    expect(el.querySelector(".relay-key-problem")?.textContent).toContain("不是合法 JSON");
    // 非空对照：没问题时那一块不出现。
    expect(
      renderRelayKeyBlock(status(), ACCTS, () => {}).querySelector(".relay-key-problem"),
    ).toBeNull();
  });

  it("存一次：明文原样交给回调，交完输入框**立刻清空**", () => {
    const seen: string[] = [];
    const el = renderRelayKeyBlock(status({ configured: false, masked: "" }), ACCTS, (k) => {
      seen.push(k);
    });
    const input = el.querySelector<HTMLInputElement>("input.relay-key-input")!;
    input.value = "  sk-ant-TYPED-BY-HAND  ";
    el.querySelector<HTMLButtonElement>("button.relay-key-save")!.click();
    expect(seen).toEqual(["sk-ant-TYPED-BY-HAND"]);
    expect(input.value, "存完输入框没清空 —— 明文在 DOM 里留着").toBe("");
    // 空输入不触发（否则会把 key 存成空串，等于悄悄清掉用户的配置）。
    el.querySelector<HTMLButtonElement>("button.relay-key-save")!.click();
    expect(seen).toEqual(["sk-ant-TYPED-BY-HAND"]);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // `K-H2c` `KH2C1` 前端那两堵墙：那一块说得出「配给哪个账号」，而且**不自己推 id**。
  // ───────────────────────────────────────────────────────────────────────────

  it("KH2C1：存的时候把**选中那个账号的 configDir** 一起交出去 —— 换一个号就换一个值", () => {
    const seen: Array<[string, string]> = [];
    const el = renderRelayKeyBlock(status({ configured: false, masked: "" }), ACCTS, (k, d) => {
      seen.push([k, d]);
    });
    const picker = el.querySelector<HTMLSelectElement>("select.relay-key-account");
    expect(picker, "账号选择器不见了 —— 下面全是空真").toBeTruthy();
    // 选项的 value 是**不透明的 configDir**，不是名字（名字只用来显示）。
    expect([...picker!.options].map((o) => o.value)).toEqual([
      "/h/.claude-accts/dir-one",
      "/h/.claude-accts/dir-two",
    ]);
    expect([...picker!.options].map((o) => o.textContent)).toEqual(["n1", "n2"]);

    const input = el.querySelector<HTMLInputElement>("input.relay-key-input")!;
    const save = el.querySelector<HTMLButtonElement>("button.relay-key-save")!;
    input.value = "sk-ant-FOR-ONE";
    save.click();
    // ★ 换一个号，同一个输入框，交出去的**第二格必须跟着变**。
    picker!.value = "/h/.claude-accts/dir-two";
    picker!.dispatchEvent(new Event("change"));
    input.value = "sk-ant-FOR-TWO";
    save.click();
    expect(seen).toEqual([
      ["sk-ant-FOR-ONE", "/h/.claude-accts/dir-one"],
      ["sk-ant-FOR-TWO", "/h/.claude-accts/dir-two"],
    ]);
  });

  it("KH2C1：状态那一行说的是**选中那个号**配没配，`status.configured` 说的是顶层那一把 —— 两行分开", () => {
    const el = renderRelayKeyBlock(status(), ACCTS, () => {});
    const state = el.querySelector(".relay-key-state")!;
    // 选中的是 routed=true 那个 ⇒ 说「已经有它那一行」。
    expect(state.textContent).toContain("n1");
    expect(state.textContent).toContain("已经有它那一行");
    // ★ 换到 routed=false 那个 ⇒ 同一行必须翻面（非空对照：这把尺子分得出两种结局）。
    const picker = el.querySelector<HTMLSelectElement>("select.relay-key-account")!;
    picker.value = "/h/.claude-accts/dir-two";
    picker.dispatchEvent(new Event("change"));
    expect(state.textContent).toContain("n2");
    expect(state.textContent).toContain("还没有它那一行");
    // 顶层那一把是**另一行**（`KH2C3`：读得出来，但界面不再往那儿写）。
    const legacy = el.querySelector(".relay-key-legacy");
    expect(legacy?.textContent, "顶层那一把没有单独显 —— 它会被读成当前这个号的状态").toContain(
      "sk-a**********WXYZ",
    );
    expect(legacy!.textContent).toContain("不再往那一格写");
  });

  it("KH2C1：一个能配的账号都没有时**存不出去** —— 不许悄悄落到顶层那一格", () => {
    const seen: Array<[string, string]> = [];
    const el = renderRelayKeyBlock(status({ configured: false, masked: "" }), [], (k, d) => {
      seen.push([k, d]);
    });
    expect(el.querySelector("select.relay-key-account"), "没有账号却还挂着选择器").toBeNull();
    const input = el.querySelector<HTMLInputElement>("input.relay-key-input")!;
    const save = el.querySelector<HTMLButtonElement>("button.relay-key-save")!;
    expect(input.disabled).toBe(true);
    expect(save.disabled).toBe(true);
    // 即便有人绕过 disabled 直接点，也不许交出去（`disabled` 在 jsdom 里不拦 `.click()`）。
    input.value = "sk-ant-NOWHERE-TO-GO";
    save.click();
    expect(seen, "没有账号可配却把 key 交出去了 —— 那一把会落到哪儿？").toEqual([]);
    // 而且要说清为什么配不了（不是一个空白的死胡同）。
    expect(el.querySelector(".relay-key-state")!.textContent).toContain("没有能配的账号");
  });

  it("KH2C1 机检：前端**一个字都不推账号 id** —— 那条规则全仓只有 Rust 那一份", () => {
    const code = src();
    // 人群 = 本文件生产段。针 = 四种「自己取末段名」的常见写法。
    for (const needle of ["split(\"/\")", "split('/')", "basename(", "lastIndexOf(\"/\")"]) {
      expect(
        code.includes(needle),
        `前端出现了 \`${needle}\` —— 那是在长**第二份**「从 configDir 取账号 id」的规则。\n` +
          "`relay_account_id_of_dir` 的头注逐字：两边各写一个 basename 规则，" +
          "漂开的那天症状是「设置里说走中转、起会话时没走」，而两边看起来都没错。",
      ).toBe(false);
    }
    // ★ 非空对照：这把尺子**认得出**东西（不是恒 false）。
    // ⚠ 这里刻意**不用**语料变量上那个裸的子串包含判断：`scanning-guard-registry`
    //   立着一条递减棘轮（本轮实测撞过两次：9 > 上限 8，第二次撞的是**这句注释自己**
    //   —— 那个扫描器扫的是原始源码，注释里写成代码形状照样计数）。
    //   它禁的理由与这里要的东西同向：子串比事实小。⇒ 用带词边界的正则。
    expect(/\bconfigDir\b/.test(code), "同一把尺子连 `configDir` 都量不到 —— 它恒 false").toBe(
      true,
    );
    // ★ 正题的另一半：那条命令**确实**收到了 configDir（不是「什么都没传所以没推 id」）。
    expect(
      /write_relay_credentials_key\(\{\s*key,\s*configDir\s*\}\)/.test(code),
      "那条写命令没把 configDir 一起交出去 —— 后端就只能落到顶层那一格",
    ).toBe(true);
  });

  it("KS7 机检：那把 key 在前端**只流向一条命令**，绝不进 `save_config`", () => {
    const code = src();
    // ① 前端拿到的明文只出现在一处出口。
    const calls = [...code.matchAll(/commands\.(\w+)\(/g)].map((m) => m[1]);
    expect(calls, "一条命令调用都没扫到 —— 抽取器坏了").toContain("write_relay_credentials_key");
    expect(
      calls.filter((c) => c === "save_config"),
      "账号这一组里出现了 `save_config` —— key 有可能被塞进前端「读—改—写」整份的那份配置",
    ).toEqual([]);
    // ② 那个字段名不许出现在本文件里（它是**后端那份文件**的 schema，不是前端配置的）。
    expect(
      code.includes("api_key"),
      "前端源码里出现了 `api_key` —— 那个字段是后端那份文件的 schema，前端不该认识它",
    ).toBe(false);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// `N-F1b`：**没有配任何远端时，这一节讲的是这台机器。**
//
// 病（`N-F1` 摸底 / 定框 `N1` 订正段）：本机账号**今天就读得出来**
//（`fetchLocalAccounts` 自 `a354c83` 起在盘上、状态栏那个 chip 现在就在渲染它），
// 空的只有面板这一节 —— 它在 `origin` 为空时直接早返回，逐字劝用户「先去配一台远端 Linux」。
// 于是一台本来就有三个账号的机器，用户打开设置看到的是「你先去买一台远端」。
//
// ★ 这一族**两侧都要断**（件文件 `NF1bD1` 的 acceptor 失效路径逐字写着）：
// 只断「那句话没了」的话，把那一行 `this.info(...)` 删掉就能骗过整族判据。
// ⇒ 正面（列出来了、几行、名字逐个对上）与反面（旧那句话一个字不出）各有断言。
// ─────────────────────────────────────────────────────────────────────────────
describe("N-F1b 没有远端时：设置面板列得出这台机器的账号", () => {
  /** 三个名字刻意落不同色槽，顺带让「渲染了几行」那条不会因为同名而弱绿。 */
  const L1 = "wei";
  const L2 = "amy";
  const L3 = "kit";
  const three = () =>
    localState({
      accounts: [
        acct({ name: L1, configDir: "/h/.claude-accts/wei" }),
        acct({ name: L2, email: "amy@x.edu", configDir: "/h/.claude-accts/amy" }),
        acct({ name: L3, email: "kit@x.edu", configDir: "/h/.claude-accts/kit" }),
      ],
      defaultName: L1,
    });

  /** 没有配任何远端 —— 本族每条都从这里出发。 */
  function noRemotes(): void {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
  }

  /**
   * `NF1bD2` 的**人群**：本机那一支真渲染出来的每一个字符串。
   *
   * 分母怎么数的：本机那块（`.accounts-local`）子树里
   *   ① 每个**叶子**元素的 `textContent`（去空白后非空的才进）—— 非叶子会把子串重复计一遍；
   *   ② 每个元素的 `title` 属性（hover 也是用户看得见的文案，`KH2B7` 那一条正是钉在 title 上的）。
   * 取不到那块（整块没渲染）就回空数组 ⇒ 调用方那条「分母不许是 0」的断言会先红。
   */
  function localStrings(el: HTMLElement): string[] {
    const root = el.querySelector(".accounts-local");
    if (!root) return [];
    const out: string[] = [];
    for (const n of [root, ...root.querySelectorAll("*")]) {
      if (n.children.length === 0) {
        const t = (n.textContent ?? "").trim();
        if (t) out.push(t);
      }
      const title = (n.getAttribute("title") ?? "").trim();
      if (title) out.push(title);
    }
    return out;
  }

  // ---- `NF1bD1` 正面：真列出来了 ----

  it("★ NF1bD1 正面：喂 3 个本机账号 → 真渲染出 3 行，名字逐个对上", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(three());
    const el = await mount();
    const rows = [...el.querySelectorAll(".accounts-local-row")];
    expect(rows.length, "本机账号没被渲染成行 —— 这一件的正题就是这个数").toBe(3);
    expect(
      rows.map((r) => r.querySelector(".accounts-local-row-name")?.textContent),
      "行渲染出来了但名字对不上 —— 那是渲染了别的东西，不是渲染了这三个号",
    ).toEqual([L1, L2, L3]);
    // 计数那一行也得说得出同一个数（两处不许各说各的）。
    expect(el.querySelector(".accounts-local-count")?.textContent).toContain(
      `3 ${accounts.LOCAL_ACCOUNTS_COPY.countSuffix}`,
    );
    // 当前账号那一格接上了 `currentWorkingAccount`（defaultName = L1）。
    expect(el.querySelector(".accounts-local-row.current .accounts-local-row-name")?.textContent).toBe(
      L1,
    );
  });

  // ---- `NF1bD1` 反面：旧那句话一个字都不许留 ----

  it("★ NF1bD1 反面：旧那句「先在「连接」组配一台远端」一个字都不出现", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(three());
    const el = await mount();
    const seen = el.textContent ?? "";
    // 非空对照：这一屏真的有东西（否则下面两条在空串上恒真）。
    expect(seen.length, "整屏是空的 —— 下面两条会恒真").toBeGreaterThan(20);
    expect(seen).not.toContain("没有已配置的远端");
    expect(seen).not.toContain("先在「连接」组配一台远端");
    expect(seen).not.toContain("账号功能在远端 Linux 上");
  });

  it("★ NF1bD1 空态：一个隔离账号都没有 → 说「还没有隔离账号 + 下一步」，不是「先去配远端」", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(localState({ accounts: [] }));
    const el = await mount();
    expect(el.querySelector(".accounts-local-empty-title")?.textContent).toBe(
      accounts.LOCAL_ACCOUNTS_COPY.emptyTitle,
    );
    // 「下一步」那一格不许空着 —— 一个说不出下一步的空态等于一条死胡同。
    const next = el.querySelector(".accounts-local-empty-next")?.textContent ?? "";
    expect(next.length, "空态没有下一步 —— 用户被停在这里").toBeGreaterThan(10);
    expect(next).toBe(accounts.LOCAL_ACCOUNTS_COPY.emptyNext);
    expect(el.textContent ?? "").not.toContain("先在「连接」组配一台远端");
    // 阴性对照同一格：空态不许长出行来。
    expect(el.querySelectorAll(".accounts-local-row").length).toBe(0);
  });

  it("★ NF1bD1 诚实降级：本机读口读不出来 → 如实说读不出来，**不许**渲染成「你没有账号」", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(
      localState({ available: false, error: "取不到 HOME", accounts: [] }),
    );
    const el = await mount();
    const fail = el.querySelector(".accounts-local-fail")?.textContent ?? "";
    expect(fail).toContain(accounts.LOCAL_ACCOUNTS_COPY.loadFailed);
    expect(fail, "后端给了原因却没显出来 —— 用户修不了一个不说原因的失败").toContain("取不到 HOME");
    // ★ 正题的另一半：读**失败**与「这台机真的一个号都没有」是两件事，不许合成一句。
    expect(el.querySelector(".accounts-local-empty-title"), "把读失败渲染成了空态").toBeNull();
  });

  it("★ NF1bD1 诚实降级：本机读口抛错 → 同样如实说，不炸", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockRejectedValue(new Error("backend down"));
    const el = await mount();
    const fail = el.querySelector(".accounts-local-fail")?.textContent ?? "";
    expect(fail).toContain(accounts.LOCAL_ACCOUNTS_COPY.loadFailed);
    expect(fail).toContain("backend down");
  });

  // ---- `NF1bD2`：本机口吻的文案，不复用远端那套 ----

  it("★ NF1bD2：本机那一支真渲染出来的字符串里，「远端」零命中（分母现算并断非空）", async () => {
    const harvested: string[] = [];
    // 三个本机态各量一次：有账号 / 零账号 / 读不出来。只量一个态的话，
    // 另外两个态里塞一句带「远端」的话不会红。
    const cases: Array<[string, AccountsState | Error]> = [
      ["有账号", three()],
      ["零账号", localState({ accounts: [] })],
      ["读不出来", localState({ available: false, error: "取不到 HOME", accounts: [] })],
    ];
    for (const [, st] of cases) {
      noRemotes();
      fetchLocalAccountsMock.mockReset().mockResolvedValue(st);
      harvested.push(...localStrings(await mount()));
    }
    // 分母：抽取器自检 —— 数不出东西的话下面那条是空真。
    expect(
      harvested.length,
      `本机那一支只收到 ${harvested.length} 个字符串 —— 抽取器或渲染坏了，下面那条会零命中地绿`,
    ).toBeGreaterThan(10);
    expect(
      harvested.filter((s) => s.includes("远端")),
      "本机那一支上出现了「远端」——这一节讲的是这台机器",
    ).toEqual([]);
    // 点名那一句：件计划 `NF1bD2` 逐字禁的就是它（`deriveUi` 的 not-enabled 那一支）。
    expect(harvested.filter((s) => s.includes("该远端尚未启用多账号"))).toEqual([]);
    // 阴性对照：同一把尺子在**远端**那一支上**认得出**「远端」——它不是恒空。
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host()] });
    fetchAccountsMock.mockResolvedValue(state({ available: false, error: "该远端配置为 daemonless" }));
    const remoteEl = await mount();
    expect(
      (remoteEl.textContent ?? "").includes("远端"),
      "远端那一支上也找不到「远端」—— 上面那把尺子量不到东西",
    ).toBe(true);
  });

  it("★ NF1bD2：本机文案表里逐条不含「远端」（分母 = 表的条目数，现算）", () => {
    const table = Object.entries(accounts.LOCAL_ACCOUNTS_COPY);
    // 分母现算（`brief` 13b：报一个基数也是复述 ⇒ 不写死条数）。
    expect(table.length, "文案表是空的 —— 下面那条是空真").toBeGreaterThan(0);
    expect(
      table.filter(([, v]) => v.includes("远端")).map(([k]) => k),
      "本机文案表里有一条带「远端」",
    ).toEqual([]);
  });

  /**
   * `NF1bD2` 的后半：**文案只许有一个家。**
   *
   * # 为什么这把尺子不是「面板源码里 grep 那几句」
   *
   * 第一版就是那么写的，跑出来**当场三条假阳**：`countSuffix`（`个账号`）·
   * `manifestPrefix`（`清单`）· `currentMark`（`当前`）在面板里各有命中 ——
   * 而那些命中全是**远端那条路自己的文案**（`已启用 · N 个账号 · manifest …`、
   * `设为当前账号`）。短词是两条路共用的词汇，「不出现在面板里」对它们根本不成立。
   * ⇒ 那把尺子会逼着人去改**产品文案**来迁就判据。〔`brief` 第 12 条那一族：
   * 断言用的子串别取自与被测性质无关的东西〕
   *
   * # 换成的尺子
   *
   * 人群仍是**本机那一支渲染出来的字符串**（`NF1bD2` 的 acceptor 逐字要求）。
   * 做法：把「合法来源」逐个从渲染串里抠掉，剩下的**不许再有汉字**。
   * 合法来源三类，逐类现算、逐类都写在下面：
   *   ① 本机文案表 `LOCAL_ACCOUNTS_COPY` 的全部取值；
   *   ② 徽章那一族的取值 —— 它们的家在 `accounts.ts` 的 `accountStatusBadge`
   *      （另一个家，但**也是一个家**，不是面板里写死的）；
   *   ③ 这一轮桩喂进去的动态值（账号名 / 邮箱 / configDir / 清单路径 / 后端错误串）。
   * 剩下汉字 ⇒ 那句话既不在表里、也不是数据 ⇒ 它被就地写在面板里了。
   *
   * ⚠ **诚实边界**：这把尺子量的是**渲染出来的文本**。
   * 有人在面板里就地写一句**与表里某条逐字相同**的话，它看不出来（残渣是空的）。
   * 它买到的是「面板没有第二套**说法**」，不是「面板里没有第二份**字面量**」。
   */
  it("★ NF1bD2 只有一个家：本机那一支渲染出来的汉字，全部来自文案表 / 徽章 / 数据", async () => {
    const st = three();
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(st);
    const el = await mount();
    const rendered = localStrings(el);
    expect(rendered.length, "本机那一支没渲染出东西 —— 下面那条是空真").toBeGreaterThan(5);

    const allowed = [
      // ① 本机文案表（现算，不写死条数）
      ...Object.values(accounts.LOCAL_ACCOUNTS_COPY),
      // ② 徽章那一族：家在 accounts.ts，逐个账号现算
      ...st.accounts.flatMap((a) => {
        const b = accounts.accountStatusBadge(a);
        return [b.text, b.title];
      }),
      // ③ 这一轮桩喂进去的动态值
      ...st.accounts.flatMap((a) => [a.name, a.email, a.configDir ?? ""]),
      st.meta?.manifestPath ?? "",
      String(st.accounts.length),
    ].filter((s) => s.length > 0);
    // 长的先抠，短的后抠 —— 反过来会把长句拆碎、留下假残渣。
    allowed.sort((a, b) => b.length - a.length);

    const residue = rendered
      .map((s) => {
        let left = s;
        for (const a of allowed) left = left.split(a).join("");
        return [s, left.replace(/[\s·|/:：，。]/g, "")] as const;
      })
      .filter(([, left]) => /[一-鿿]/.test(left));
    expect(
      residue,
      "本机那一支上出现了既不在文案表里、也不是数据的汉字 —— 那句话被就地写在面板里了",
    ).toEqual([]);

    // 抽取器自检：这把尺子**认得出**残渣（不是恒空）。喂一句谁都没登记过的话进去。
    const probe = "这一句谁都没登记过";
    let leftProbe = `${accounts.LOCAL_ACCOUNTS_COPY.heading}${probe}`;
    for (const a of allowed) leftProbe = leftProbe.split(a).join("");
    expect(leftProbe, "同一把尺子连一句没登记过的话都抠不出来 —— 它恒空").toBe(probe);
  });

  // ---- `NF1bD3`：远端那条路一个字节没动 ----

  it("★ NF1bD3：配了远端时照旧走远端那条读口，本机那一支一格不长", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host()] });
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: L1 })], defaultName: L1 }),
    );
    const el = await mount();
    expect(fetchAccountsMock, "配了远端却没走远端那条读口").toHaveBeenCalled();
    expect(fetchLocalAccountsMock, "配了远端却去读了本机的账号").not.toHaveBeenCalled();
    expect(el.querySelector(".accounts-local"), "远端页上长出了本机那一块").toBeNull();
    // 远端那三件套照旧在（非空对照：这条不是在一屏空白上判的）。
    expect(el.querySelector(".accounts-table")).not.toBeNull();
    expect(el.querySelector(".accounts-current-banner")).not.toBeNull();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// `N-F2`：**本机也进「还差什么」那张清单** —— 那张清单本来就把本机算进去了，缺的是写点。
//
// 病（件文件 `§0a` / 定框 `N2` 09-05 订正段，本族开工时逐条现打复核过）：
// `computeGaps` 的入参里本来就有 `LOCAL_MACHINE_KEY`，而 `notApplicable` 只把本机的
// `daemon` / `connection` 排掉 ⇒ 本机的 `acctIso` / `accounts` 是**适用**的两格；
// 可它们的唯一写点 `AccountsSection.note()` 第一行是 `if (!this.origin) return`，
// 而本机这条路上 `origin` 恒空 ⇒ 那两格**永远停在「没测过」**，
// 于是 `summarizeGaps` 恒非 null，`remote-section` 里「全绿就整块不出现」那一支是死代码。
//
// ★ 本族**两侧都断**（件文件 `NF2D2` 逐字要求）：
//   ① 本机侧：跑完之后账本里那两格不再是 `unknown`，且**三档各写各的**；
//   ② 远端侧：远端那条路写进去的东西**一格不变**（只断①的话，把远端那几行顺手改坏也不会红）。
// ─────────────────────────────────────────────────────────────────────────────
describe("N-F2 本机那两格真的被写进账本", () => {
  /** 每条都从一本干净的账本出发 —— 账本住 localStorage，跨用例会串。 */
  beforeEach(() => localStorage.clear());

  const L1 = "wei";
  const L2 = "amy";
  const L3 = "kit";
  const threeLocal = () =>
    localState({
      accounts: [
        acct({ name: L1, configDir: "/h/.claude-accts/wei" }),
        acct({ name: L2, email: "amy@x.edu", configDir: "/h/.claude-accts/amy" }),
        acct({ name: L3, email: "kit@x.edu", configDir: "/h/.claude-accts/kit" }),
      ],
      defaultName: L1,
    });

  function noRemotes(): void {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
  }

  /** 跑一遍本机那条路，回来时账本里本机那一栏长什么样。 */
  async function localLedgerAfter(st: AccountsState | Error): Promise<MachineStatus> {
    noRemotes();
    if (st instanceof Error) fetchLocalAccountsMock.mockReset().mockRejectedValue(st);
    else fetchLocalAccountsMock.mockReset().mockResolvedValue(st);
    await mount();
    return readStatus(LOCAL_MACHINE_KEY);
  }

  /** 只留 `kind` 与 `detail`：`at` 是 `Date.now()`，比它等于在断时钟。 */
  function shape(s: MachineStatus): Record<string, { kind: string; detail?: string }> {
    const out: Record<string, { kind: string; detail?: string }> = {};
    for (const [k, v] of Object.entries(s)) {
      if (v) out[k] = { kind: v.kind, detail: v.detail };
    }
    return out;
  }

  // ---- 先证会红：今天的行为长什么样 ----

  it("★ NF2D2 地板：本机那条路**跑之前**，账本里本机那一栏是空的（这一族不是空真）", () => {
    // 这条是分母自检。没有它，下面每一条「写进去了」都可能是在断一本本来就有内容的账。
    expect(readStatus(LOCAL_MACHINE_KEY)).toEqual({});
    // 而那两格是**适用**的：`notApplicable` 只排掉本机的 daemon / connection。
    const before = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: readStatus,
      hostOs: "windows",
    });
    expect(
      before.map((g) => `${g.facet}:${g.kind}`),
      "本机在 Windows 上的适用格不是恰好这两格 —— 下面几条的题面就得重写",
    ).toEqual(["acctIso:unknown", "accounts:unknown"]);
  });

  // ---- 三档各写各的 ----

  it("★ NF2D2 档一（读出来了·有号）：两格都 ok，且账号那格说得出是几个", async () => {
    const led = await localLedgerAfter(threeLocal());
    expect(shape(led)).toEqual({
      accounts: { kind: "ok", detail: "3 个" },
      acctIso: { kind: "ok", detail: "已启用" },
    });
  });

  it("★ NF2D2 档一（读出来了·零个号）：清单读到了 = ok，隔离没启用 = fail —— 两格不许合成一句", async () => {
    // 「读到了但一个号都没有」与「读不出来」是两件事：前者 accounts 该绿。
    const led = await localLedgerAfter(localState({ accounts: [] }));
    expect(shape(led)).toEqual({
      accounts: { kind: "ok", detail: "已读取" },
      acctIso: { kind: "fail", detail: "未启用" },
    });
  });

  it("★ NF2D2 档二（后端不在）：两格 fail，且 detail 说得出是这一档", async () => {
    const led = await localLedgerAfter(
      localState({ available: false, error: "本机后端不在，问不出…", accounts: [] }),
    );
    expect(shape(led)).toEqual({
      accounts: { kind: "fail", detail: "后端不在" },
      acctIso: { kind: "fail", detail: "后端不在" },
    });
  });

  it("★ NF2D2 档三（读不动）：两格 fail，且 detail 与上一档**不同**", async () => {
    const led = await localLedgerAfter(new Error("backend down"));
    expect(shape(led)).toEqual({
      accounts: { kind: "fail", detail: "读不动" },
      acctIso: { kind: "fail", detail: "读不动" },
    });
  });

  it("★ NF2D2 三档两两不同 —— 只断「调了 recordFacet」的话，三档写成同一个值也全绿", async () => {
    // 分母：**面板看得见的那三档**（读出来了 / 后端不在 / 读不动），逐档各跑一次真面板。
    // ⚠ 诚实边界：后端那侧其实是三档（`Listed` / `NoBackend` / `Unreadable`），
    // 而 `NoBackend` 与 `Unreadable` 到前端都变成 `available:false` + 一句 error 文案，
    // 前端分不出来 —— 分开它们要后端多带一个字段回来（`src-tauri`，本件射程外）。
    const cases: Array<[string, AccountsState | Error]> = [
      ["读出来了", threeLocal()],
      ["后端不在", localState({ available: false, error: "x", accounts: [] })],
      ["读不动", new Error("backend down")],
    ];
    const seen: string[] = [];
    for (const [, st] of cases) {
      const led = await localLedgerAfter(st);
      // 两格都得写到 —— 漏一格，那一格就还停在「没测过」。
      expect(led.accounts, "accounts 这一格没被写").toBeDefined();
      expect(led.acctIso, "acctIso 这一格没被写").toBeDefined();
      seen.push(JSON.stringify(shape(led)));
    }
    expect(seen.length, "分母塌了 —— 一档都没跑").toBe(3);
    expect(
      new Set(seen).size,
      `三档在账本上写成了同一个样子：${seen.join(" | ")}`,
    ).toBe(3);
  });

  it("★ NF2D2 本机侧总账：三档跑完，那两格**没有一档**还停在「没测过」", async () => {
    for (const st of [
      threeLocal(),
      localState({ accounts: [] }),
      localState({ available: false, error: "x", accounts: [] }),
      new Error("boom"),
    ] as Array<AccountsState | Error>) {
      const led = await localLedgerAfter(st);
      const gaps = computeGaps({
        origins: [LOCAL_MACHINE_KEY],
        statusOf: readStatus,
        hostOs: "windows",
      });
      // 「没测过」= `unknown`。测过了但确认缺（`missing`）是另一回事，这条不管那个。
      expect(
        gaps.filter((g) => g.kind === "unknown").map((g) => g.facet),
        `跑完之后本机还有格子停在「没测过」（账本：${JSON.stringify(shape(led))}）`,
      ).toEqual([]);
    }
  });

  // ---- 远端那条路一个字节不变 ----

  it("★ NF2D2 远端侧：远端那五支写进账本的东西逐格不变，且本机那一栏一格不长", async () => {
    // 分母 = 远端那条路今天**全部**五支（`reload` 里 catch + `deriveUi` 的四个 kind），
    // 逐支各跑一次真面板。少一支，那一支上顺手改坏一行不会红。
    const H = host().label;
    const remote = async (
      set: () => void,
    ): Promise<Record<string, { kind: string; detail?: string }>> => {
      localStorage.clear();
      readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host()] });
      fetchAccountsMock.mockReset();
      set();
      await mount();
      // 本机那一栏一格不长 —— 配了远端时这一节讲的不是本机。
      expect(readStatus(LOCAL_MACHINE_KEY), "远端页上把东西写进了本机那一栏").toEqual({});
      return shape(readStatus(H));
    };

    expect(
      await remote(() => fetchAccountsMock.mockRejectedValue(new Error("net"))),
      "拉取失败那一支",
    ).toEqual({ accounts: { kind: "fail", detail: "拉取失败" } });

    expect(
      await remote(() =>
        fetchAccountsMock.mockResolvedValue(
          state({ available: false, error: "该主机配置为 daemonless", accounts: [] }),
        ),
      ),
      "daemonless 那一支",
    ).toEqual({
      accounts: { kind: "na", detail: "daemonless" },
      acctIso: { kind: "na", detail: "daemonless" },
    });

    expect(
      await remote(() =>
        fetchAccountsMock.mockResolvedValue(
          state({ available: false, error: "daemon 过旧", accounts: [] }),
        ),
      ),
      "老 daemon 那一支",
    ).toEqual({ accounts: { kind: "fail", detail: "daemon 需更新" } });

    expect(
      await remote(() => fetchAccountsMock.mockResolvedValue(state({ accounts: [] }))),
      "未启用那一支",
    ).toEqual({
      accounts: { kind: "ok", detail: "已读取" },
      acctIso: { kind: "fail", detail: "未启用" },
    });

    expect(
      await remote(() =>
        fetchAccountsMock.mockResolvedValue(
          state({ accounts: [acct({ name: L1 }), acct({ name: L2 })], defaultName: L1 }),
        ),
      ),
      "已启用那一支",
    ).toEqual({
      accounts: { kind: "ok", detail: "2 个" },
      acctIso: { kind: "ok", detail: "已启用" },
    });
  });

  // ---- `NF2D3`：从**真的一次面板运行**接到「整块该不该出现」 ----

  it("★ NF2D3：本机全绿 + 一台远端都没有 ⇒ summarizeGaps 返回 null（那一块整块不出现的前提）", async () => {
    // 分母写清（`NF2D3` 的 acceptor 逐字要求，防「一台机器都没有」蒙混）：
    //   · 机器数 = 1（本机），**不是空清单**；
    //   · 这台机在 Windows 上的适用格 = { acctIso, accounts }（daemon / connection 不适用、
    //     ccm 的对应物是「终端集成」那块）—— 上面那条「地板」用例已把这个集合逐项断过；
    //   · 这两格由**真的一次面板运行**写绿，账本不是手工摆出来的。
    const led = await localLedgerAfter(threeLocal());
    expect(shape(led), "前提没成立：这一次面板运行没把两格写绿").toEqual({
      accounts: { kind: "ok", detail: "3 个" },
      acctIso: { kind: "ok", detail: "已启用" },
    });
    const origins = [LOCAL_MACHINE_KEY];
    expect(origins.length, "分母是空的 —— 下面那条 null 是空真").toBe(1);
    const gaps = computeGaps({ origins, statusOf: readStatus, hostOs: "windows" });
    expect(gaps).toEqual([]);
    expect(
      summarizeGaps(gaps),
      "本机全绿、没有远端，那张清单却还有话说 ⇒「全绿就整块不出现」仍是死代码",
    ).toBeNull();
  });

  it("★ NF2D3 先证会红：把那两格改回「没测过」⇒ 清单又出现，且逐字写着「没测过」", async () => {
    await localLedgerAfter(threeLocal());
    // 只把本机那一栏抹掉 = 回到本件之前的行为（那两格从来没人写）。
    localStorage.clear();
    expect(readStatus(LOCAL_MACHINE_KEY)).toEqual({});
    const gaps = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: readStatus,
      hostOs: "windows",
    });
    expect(gaps.map((g) => g.facet)).toEqual(["acctIso", "accounts"]);
    const s = summarizeGaps(gaps);
    expect(s).not.toBeNull();
    expect(s, "回到旧行为时它该说「还没测过」").toContain("还没测过");
  });
});
