// account-ux U7：设置「账号」组的 IA / 渲染分支测试（vitest + jsdom）。
//
// 重点不是"长得好不好看"，而是两件会真伤人的事：
//   ① 四条**降级分支**（无远端 / daemonless / 老 daemon / 未启用）的 DOM 与文案不能被 IA 重排改掉；
//   ② 维护区（加账号 / 补链，都会动远端目录）**必须默认折叠**，不能常驻摊在手边。
// U6 的教训：断言要锚在真契约上，并对关键属性做变异验证（故意改坏看会不会红）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const readRemoteConfigMock = vi.fn();
const fetchAccountsMock = vi.fn();
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../remote-config", () => ({ readRemoteConfig: () => readRemoteConfigMock() }));

import { AccountsSection } from "./accounts-section";
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
  vi.spyOn(accounts, "fetchAccounts").mockImplementation(() => fetchAccountsMock());
  vi.spyOn(accounts, "invalidateAccountsCache").mockImplementation(() => {});
});

/** 降级态**一律**不该长出 ready 态的三件套（表 / 横幅 / 维护区）——这正是 IA 重排最该防的回归。 */
function expectNoReadyChrome(el: HTMLElement): void {
  expect(el.querySelector(".accounts-table")).toBeNull();
  expect(el.querySelector(".accounts-current-banner")).toBeNull();
  expect(el.querySelector(".accounts-maint-wrap")).toBeNull();
}

describe("account-ux U7 设置账号组：降级分支不被 IA 重排改掉", () => {
  it("没有已配置的远端 → 只给一句说明，不渲染表/横幅/维护区", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    const el = await mount();
    expect(el.querySelector(".accounts-info")?.textContent).toContain("没有已配置的远端");
    expectNoReadyChrome(el);
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
