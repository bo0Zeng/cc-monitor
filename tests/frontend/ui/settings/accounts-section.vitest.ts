// account-ux U7：设置「账号」组的 IA / 渲染分支测试（vitest + jsdom）。
//
// 重点不是"长得好不好看"，而是两件会真伤人的事：
//   ① 三条**降级分支**（无远端 / 老 backend / 未启用）的 DOM 与文案不能被 IA 重排改掉；
//     〔`K-R59` 09-11：原先是四条 —— `daemonless` 那一支随定框 `K35` 整档删除。〕
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
// 〔FIX4 · ⑬〕开终端那三步（`terminal_dial` → `terminal-ssh` → `open_terminal_window`）经 `terminalShim` 译回旧的 `launch_remote_terminal`。
vi.mock("@tauri-apps/api/core", async () => {
  const { isTerminalStep, terminalShim } = await import("../../../test-support/chan-fake");
  const term = terminalShim((...a: unknown[]) => invokeMock(...a));
  return {
    invoke: (...a: unknown[]) => {
      const [cmd, args] = a as [string, unknown];
      if (isTerminalStep(cmd, args)) return term(cmd, args);
      return invokeMock(...a);
    },
  };
});
// 改账号库那几件都先在界面里确认：判据替用户答（默认「确定」），并记下问了什么。
const askConfirmMock = vi.fn();
vi.mock("../../../../src/frontend/ui/ask-dialog", () => ({ askConfirm: (...a: unknown[]) => askConfirmMock(...a) }));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/remote-config", () => ({ readRemoteConfig: () => readRemoteConfigMock() }));

import { readFileSync } from "node:fs";
// 〔US1〕API key 那两问改走通道（`chan_call`，op = `apikey-read` / `apikey-routing`）：判据按 op 分派、回成品字节。
// 账号库那几条（`accounts-*`）由那台后端做：判据用罐头答（`accountsFakeInvoke`），只看界面问了哪台、交了什么意图。
import { accountsFakeInvoke, chanArgsJson, chanReply, fakeLoginCmd, isAccountsOp, isChanCall, type ChanCallArgs } from "../../../test-support/chan-fake";

/** 这一趟里问过的账号库命令：`[op, origin, 入参]`。 */
function accountOps(calls: Array<[string, unknown]>): Array<[string, string, Record<string, unknown>]> {
  return calls
    .filter(([c, a]) => isAccountsOp(c, a))
    .map(([, a]) => [(a as ChanCallArgs).op, (a as ChanCallArgs).origin, chanArgsJson(a as ChanCallArgs) as Record<string, unknown>]);
}

// 〔HX2 · 第四波 4D〕写 key 改走通道（`chan_call`，op = `apikey-key-set`）：判据把那一发译回「交给哪台 ＋ 交了什么」
//   （`{origin, configDir, key, baseUrl?}`），断言照旧是那个形状；先前是 Tauri 命令 `write_apikey_credentials_key`〔散文墓碑〕的实参。
const isKeySet = (c: unknown, a: unknown): boolean => isChanCall(c as string, a, "apikey-key-set");
import {
  AccountsSection,
  renderApikeyEditor,
  renderApikeyFileBlock,
} from "../../../../src/frontend/ui/settings/accounts-section";
// `N-F2`：账本与那张清单 —— 本文件末尾那一族要断的正是「面板跑完之后账本里是什么」，
// 所以取的是**真的** `readStatus` / `computeGaps`，一个桩都不架。
import {
  readStatus,
  recordFacet,
  LOCAL_MACHINE_KEY,
  type MachineStatus,
} from "../../../../src/frontend/ui/settings/machine-status";
import { computeGaps, summarizeGaps } from "../../../../src/frontend/ui/settings/readiness";
import type { ApikeyCredentialsStatus } from "../../../../src/frontend/ui/apikey-reads";
import { showActionFailureToast } from "../../../../src/frontend/ui/error-toast";
import * as accounts from "../../../../src/frontend/ui/accounts";
// 〔FE1〕读面从 `accounts.ts` 拆去了 `account-reads.ts`，桩打在它真住的模块上。
import * as accountReads from "../../../../src/frontend/ui/account-reads";
import type { AccountsState, Account } from "../../../../src/frontend/ui/accounts";
import { setCurrentMachine, __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { NEW_ACCOUNT_COPY } from "../../../../src/frontend/ui/settings/account-new-form";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/backend-policy";
import { POSIX_NO_WINDOW_MARKER } from "../../../../src/frontend/ui/remote-launch-run";
import COPY_TABLE from "../../../../src/shared/copy/table.json";

/** 〔第三波 S3〕文案表里本机那一支的条目（key 以 `accountsLocal.` 打头）—— 现算，不写死条数。 */
function localCopyEntries(): Array<[string, string]> {
  const out = Object.entries(COPY_TABLE.entries as Record<string, { zh: string }>)
    .filter(([k]) => k.startsWith("accountsLocal."))
    .map(([k, e]) => [k, e.zh] as [string, string]);
  if (out.length === 0) throw new Error("文案表里一条 accountsLocal.* 都没有 —— 下游的人群是空的");
  return out;
}
/** 同上，按具名占位符切成字面段（插进去的值归「数据」那一类，不归文案）。 */
function localCopyFragments(): string[] {
  return localCopyEntries().flatMap(([, zh]) => zh.split(/\{[A-Za-z][A-Za-z0-9]*\}/));
}

function acct(p: Partial<Account>): Account {
  return {
    name: "z",
    email: "z@x.edu",
    configDir: "/h/.claude-accts/z",
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
    origin: "aya",
    available: true,
    oldBackend: false,
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
  return state({ origin: LOCAL_ORIGIN, ...p }); // 〔C4b〕账号面的本机就是 `LOCAL_ORIGIN`（`"__local__"` 已退役）
}
const host = (p: Record<string, unknown> = {}) => ({
  label: "aya",
  host: "h",
  port: 22,
  user: "u",
  keyPath: "",
  hostKeyFingerprint: "",
  addresses: [],
  jump: "",
  ...p,
});

/** 建 section 并等它的两段 async（init → reload）落定。 */
async function mount(): Promise<HTMLElement> {
  const s = loaded(new AccountsSection());
  document.body.innerHTML = "";
  document.body.appendChild(s.element);
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
  return s.element;
}

beforeEach(() => {
  vi.restoreAllMocks();
  __resetMachineContextForTests();
  // 〔第三波 S3〕共用 store 的 `null` 就是本机（`machine-context.ts` 头注）；本分节从此**只认 store**，
  // 不再在 `null` 上兜底去读主远端。本文件大半条目量的是远端那一支 ⇒ 默认站在 aya 那一页上；
  // 量本机那一支的条目显式 `setCurrentMachine(null)`（各处的 `noRemotes()` 顺手做了 ——
  // 一台远端都没配的机器只有本机那一页）。
  setCurrentMachine("aya");
  readRemoteConfigMock.mockReset().mockResolvedValue({ enabled: true, hosts: [host()] });
  // 〔AL1 · 2026-09-24〕从前本机那一支挂着一块「按账号生成命令」（挂上去就先预览一次），
  // 这里要给那条命令一个形状对的最小答案。那一块搬去了机器页 ⇒ 所有命令照旧回 `undefined`。
  invokeMock.mockReset().mockImplementation((cmd: unknown, args: unknown) =>
    isAccountsOp(cmd as string, args) ? accountsFakeInvoke(args) : Promise.resolve(undefined),
  );
  askConfirmMock.mockReset().mockResolvedValue(true);
  fetchAccountsMock.mockReset();
  // `N-F1b`：默认给「这台机一个隔离账号都没有」——最保守的一档，
  // 想量别的态的用例自己在里面覆盖掉它。
  fetchLocalAccountsMock.mockReset().mockResolvedValue(localState({ accounts: [] }));
  vi.spyOn(accountReads, "fetchAccounts").mockImplementation(() => fetchAccountsMock());
  vi.spyOn(accountReads, "fetchLocalAccounts").mockImplementation(() => fetchLocalAccountsMock());
  vi.spyOn(accountReads, "invalidateAccountsCache").mockImplementation(() => {});
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
    setCurrentMachine(LOCAL_ORIGIN); // 〔第三波 S3〕一台远端都没配 ⇒ 只有本机那一页
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

  it("老后端（不支持账号）→ 提示需更新，不渲染表", async () => {
    fetchAccountsMock.mockResolvedValue(state({ available: false, oldBackend: true, error: "backend 过旧" }));
    const el = await mount();
    expect(el.querySelector(".accounts-info")?.textContent).toContain("需要更新");
    expectNoReadyChrome(el);
  });

  it("未启用多账号 → 内联「启用多账号」，不渲染表；一件要装的东西都没有", async () => {
    fetchAccountsMock.mockResolvedValue(state({ accounts: [] }));
    const el = await mount();
    expect(el.querySelector(".accounts-not-enabled")).not.toBeNull();
    expect(el.querySelector(".accounts-wizard")).not.toBeNull();
    expect(el.querySelector(".accounts-needs-deploy"), "又长出了「先安装」那一块").toBeNull();
    expectNoReadyChrome(el);
  });

  // 启用：填名字 ⇒ 问那台后端预演（将要做的那几步上屏）⇒ 点「启用」⇒ 界面里确认 ⇒ 那台后端建库。一条终端都不开。
  it("启用多账号：预演上屏 → 确认 → 只问那台后端 accounts-init，不开终端", async () => {
    const calls: Array<[string, unknown]> = [];
    invokeMock.mockImplementation((cmd: unknown, args: unknown) => {
      calls.push([cmd as string, args]);
      return isAccountsOp(cmd as string, args) ? accountsFakeInvoke(args) : Promise.resolve(undefined);
    });
    fetchAccountsMock.mockResolvedValue(state({ accounts: [] }));
    const el = await mount();
    const input = el.querySelector<HTMLInputElement>(".accounts-wiz-name")!;
    const btn = el.querySelector<HTMLButtonElement>(".accounts-wiz-btns button")!;
    expect(btn.disabled, "名字没填就能点").toBe(true);
    input.value = "z";
    input.dispatchEvent(new Event("input"));
    for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r, 0));
    expect(el.querySelector(".accounts-wiz-preview")?.textContent).toContain("（预演）accounts-init z");
    expect(btn.disabled).toBe(false);
    btn.click();
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
    expect(askConfirmMock).toHaveBeenCalledTimes(1);
    expect(String(askConfirmMock.mock.calls[0][0])).toContain("（预演）accounts-init z");
    expect(accountOps(calls)).toEqual([
      ["accounts-init", "aya", { name: "z", dryRun: true }],
      ["accounts-init", "aya", { name: "z" }],
    ]);
    expect(calls.some(([c]) => c === "launch_remote_terminal"), "又去开终端替人敲命令").toBe(false);
  });

  it("启用多账号：确认框点了取消 ⇒ 只预演、不建库", async () => {
    const calls: Array<[string, unknown]> = [];
    invokeMock.mockImplementation((cmd: unknown, args: unknown) => {
      calls.push([cmd as string, args]);
      return isAccountsOp(cmd as string, args) ? accountsFakeInvoke(args) : Promise.resolve(undefined);
    });
    askConfirmMock.mockResolvedValue(false);
    fetchAccountsMock.mockResolvedValue(state({ accounts: [] }));
    const el = await mount();
    const input = el.querySelector<HTMLInputElement>(".accounts-wiz-name")!;
    input.value = "z";
    input.dispatchEvent(new Event("input"));
    for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r, 0));
    el.querySelector<HTMLButtonElement>(".accounts-wiz-btns button")!.click();
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
    expect(accountOps(calls).map(([, , a]) => a)).toEqual([{ name: "z", dryRun: true }]);
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
      state({ accounts: [acct({ name: A, loggedIn: false, authReady: false })], defaultName: A }),
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

  // 布局契约：styles.css 的 .accounts-table 定了列轨道，行用 subgrid 继承。
  // 往 accountRow 里多 append 一个元素而不改 CSS，列就整体错位——jsdom 测不了布局，
  // 但能测这个数。
  // 🔴 〔`设计/50` 09-18〕**8 → 7**：F10 加的那条用量列随用量 ③ 轴整轴退役。
  // ⚠ **CSS 那一半不在本轮写区里**（`styles.css` 归另一路）：`.accounts-table` 的
  //    `grid-template-columns` 今天仍是 8 条轨道 ⇒ **最后一条轨道会空着**。
  //    已随本件上报，改 CSS 的那一拍要把这两处一起看。
  it("每行子元素数 == grid 列数(7)：改一处必须改另一处", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    for (const row of el.querySelectorAll(".accounts-row")) {
      expect(row.children.length).toBe(7);
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
    expect(apiBadge.textContent).toBe("API key（未配置端点）");
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
        accounts: [acct({ name: A }), acct({ name: B, loggedIn: false, authReady: false, authKind: "subscription" })],
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

  it("维护区默认折叠：补链会动远端，不该摊在手边", async () => {
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    const wrap = el.querySelector<HTMLDetailsElement>("details.accounts-maint-wrap")!;
    expect(wrap).not.toBeNull();
    expect(wrap.open).toBe(false); // ← 变异验证锚点：改成恒定默认展开这里就红
    expect(wrap.querySelector("summary")?.textContent).toContain("维护");
    expect(wrap.querySelector(".accounts-maint-ops")).not.toBeNull();
  });

  it("🔴 A2（`70 §4.3` ①②）：新建账号**不在维护折叠里**、**不是红色** —— 只有 1 个号时也一样", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: A })], defaultName: A }),
    );
    const el = await mount();
    const form = el.querySelector<HTMLElement>(".accounts-new");
    expect(form, "新建账号表单不见了").not.toBeNull();
    expect(form!.closest("details"), "新建账号又被收进了折叠组").toBeNull();
    // 维护区里不再有加账号那一套（`accounts-maint-add` 整个类已撤）。
    expect(el.querySelector(".accounts-maint-add")).toBeNull();
    const wrap = el.querySelector<HTMLDetailsElement>("details.accounts-maint-wrap")!;
    expect(wrap.open, "只剩自检/补链的维护区没有理由默认展开").toBe(false);
    // 「创建」按钮：不带 danger。非空对照：那颗按钮确实找得到。
    const create = [...form!.querySelectorAll("button")].find((b) => b.textContent === "创建");
    expect(create, "找不到「创建」按钮 —— 下面那条是空真").toBeTruthy();
    expect(create!.classList.contains("danger")).toBe(false);
    expect(create!.className).toContain("settings-btn-primary");
    // 红色只留给删账号：整个分节里的红按钮恰好是那一个号的「删除」。
    expect([...el.querySelectorAll("button.danger")].map((b) => b.textContent)).toEqual([copyText("accounts.row.remove")]);
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

// 〔`设计/50` 删用量〕原先这里是「F10/K-R101：账号行用量单元格（懒加载 + 两种状态）」整组
// （「查看用量」按钮 · 查询中占位 · 那一屏原文逐字渲染 · 空屏 · 探测失败 · 刷新）。
// 用量 ③ 轴（探针）整轴退役 ⇒ **被测对象没了**，不是断言变少了。
// 换成一条**翻面**判据：账号表里不许再长出那个单元格，也不许再发那条命令。
describe("设计/50：账号表上的用量单元格已退役（翻面判据）", () => {
  it("挂载后没有用量单元格 / 「查看用量」按钮，也没发任何 invoke", async () => {
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    expect(el.querySelector(".accounts-row-usage")).toBeNull();
    expect(el.querySelector(".accounts-usage-btn")).toBeNull();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "account_usage")).toBe(false);
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
    expect(dirs[1]).toBe("不设 CLAUDE_CONFIG_DIR");
    expect(dirs[1]).not.toBe("");
  });

  // 〔`设计/50` 删用量〕原先这里还有一条「账号 0 的用量会真的去探，且送的是账号 0 的
  // 显式表态（configDir === null）」—— 它钉的是 `Z03` 做通的那件事。
  // 用量 ③ 轴整轴退役 ⇒ 那条路没了。⚠ **「空值 ≠ 未设」这条纪律没丢**：
  // 它在起会话那条路上由 `backend::control::payload` 的两态断言继续钉着。

  it("降级说明会被渲染成显眼的一条（绝不静默）", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "z" })], notice: "远端后端版本较旧：看不到账号 0" }),
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

describe("维护区：核对 / 修复 / 回滚都问那台后端，确认在界面里", () => {
  const tick = () => new Promise((r) => setTimeout(r, 0));
  const ready = (): AccountsState => state({ accounts: [acct({ name: "z" })], defaultName: "z" });
  function wire(over: Record<string, Record<string, unknown>> = {}): Array<[string, unknown]> {
    const calls: Array<[string, unknown]> = [];
    invokeMock.mockImplementation((cmd: unknown, args: unknown) => {
      calls.push([cmd as string, args]);
      if (isAccountsOp(cmd as string, args)) return accountsFakeInvoke(args, over[(args as ChanCallArgs).op] ?? {});
      return Promise.resolve(undefined);
    });
    return calls;
  }
  const button = (el: HTMLElement, text: string): HTMLButtonElement => {
    const b = [...el.querySelectorAll<HTMLButtonElement>(".accounts-maint-ops button")].find((x) => x.textContent === text);
    expect(b, `维护区里找不到「${text}」`).toBeTruthy();
    return b!;
  };

  it("维护区只有核对 / 修复 / 回滚三颗，没有「生成 rc 片段」与「补链 sync」那一套", async () => {
    wire();
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    const labels = [...el.querySelectorAll(".accounts-maint-ops button")].map((b) => b.textContent);
    expect(labels).toEqual([
      copyText("accounts.maintenance.verify"),
      copyText("accounts.maintenance.sync"),
      copyText("accounts.maintenance.rollback"),
    ]);
    expect(el.querySelector(".accounts-rc-paste")).toBeNull();
  });

  it("核对：问那台一次 accounts-verify，逐条列出来（有要修的就说几处）", async () => {
    const calls = wire({
      "accounts-verify": {
        pass: false,
        fails: 1,
        warns: 0,
        checks: [{ level: "fail", account: "z", text: "共享链接 skills 指向 /x" }],
      },
    });
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    button(el, copyText("accounts.maintenance.verify")).click();
    for (let i = 0; i < 3; i++) await tick();
    expect(accountOps(calls)).toEqual([["accounts-verify", "aya", {}]]);
    const report = el.querySelector(".accounts-maint-report")!;
    expect(report.textContent).toContain(copyText("accounts.verify.fail", { fails: "1", warns: "0" }));
    expect(report.textContent).toContain("x z：共享链接 skills 指向 /x");
  });

  it("修复：先预演，确认框里列着那几步；确认了才真做，做完重读", async () => {
    const calls = wire();
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    const before = fetchAccountsMock.mock.calls.length;
    button(el, copyText("accounts.maintenance.sync")).click();
    for (let i = 0; i < 6; i++) await tick();
    expect(accountOps(calls).map(([op, o, a]) => [op, o, a])).toEqual([
      ["accounts-repair", "aya", { dryRun: true }],
      ["accounts-repair", "aya", {}],
    ]);
    expect(String(askConfirmMock.mock.calls[0][0])).toContain("（预演）accounts-repair");
    expect(fetchAccountsMock.mock.calls.length, "做完没重读").toBeGreaterThan(before);
  });

  it("修复：预演说没事可做 ⇒ 不问、不做", async () => {
    const calls = wire({ "accounts-repair": { steps: [], applied: false, backup: null, aliases: null } });
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    button(el, copyText("accounts.maintenance.sync")).click();
    for (let i = 0; i < 4; i++) await tick();
    expect(askConfirmMock).not.toHaveBeenCalled();
    expect(accountOps(calls).map(([, , a]) => a)).toEqual([{ dryRun: true }]);
  });

  it("回滚：预演答出用哪份备份，确认后按那一份还原", async () => {
    const calls = wire({ "accounts-rollback": { backup: "20260930-120000" } });
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    button(el, copyText("accounts.maintenance.rollback")).click();
    for (let i = 0; i < 6; i++) await tick();
    expect(accountOps(calls).map(([, , a]) => a)).toEqual([{ dryRun: true }, { backup: "20260930-120000" }]);
    expect(String(askConfirmMock.mock.calls[0][0])).toContain("20260930-120000");
  });

  it("★ 整条路上零写盘命令、零「开终端替你敲」：改账号库只经那台后端", async () => {
    const calls = wire();
    fetchAccountsMock.mockResolvedValue(ready());
    const el = await mount();
    for (const t of ["accounts.maintenance.verify", "accounts.maintenance.sync", "accounts.maintenance.rollback"] as const) {
      button(el, copyText(t)).click();
      for (let i = 0; i < 6; i++) await tick();
    }
    const names = calls.map(([c, a]) => (c === "chan_call" ? (a as ChanCallArgs).op : c));
    expect(names.filter((n) => /write|save|patch|launch_remote_terminal|files-/.test(String(n)))).toEqual([]);
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
// K-H2a：账号的第三方 API key（apikey 表）的**前端那一半**（`KS6` 永不回显 / `KS9` 路径 /
// `KS11` 界面出声 / `KS7` 不进前端整份读写的那份配置）。
// ─────────────────────────────────────────────────────────────────────────────
describe("K-H2a：第三方 API key 的前端一半", () => {
  // ⚠ 用 `process.cwd()` 相对路径而不是 `import.meta.url`：本仓 vitest 跑在仓根，
  //   而 `import.meta.url` 在这套 transform 下不是 file: scheme（实测 `The URL must be of scheme file`）。
  const src = () => readFileSync("src/frontend/ui/settings/accounts-section.ts", "utf8");

  function status(p: Partial<ApikeyCredentialsStatus> = {}): ApikeyCredentialsStatus {
    return {
      configured: true,
      masked: "sk-a**********WXYZ",
      path: "/h/.cc-monitor/apikey-credentials.json",
      notice: null,
      problem: null,
      ...p,
    };
  }

  // `K-H2c`：key 今天是**配给某一个账号**的。默认给两个号，第一个已经在 apikey 表里。
  // ⚠ 名字与 configDir 末段**刻意不同名**（`n1` vs `dir-one`）：断言里凡是用到 id 的地方，
  //   同名会让「前端拿名字当 id」与「后端从 configDir 推 id」两种实现**都绿**。
  const ACCTS = [
    { name: "n1", configDir: "/h/.claude-accts/dir-one", routed: true },
    { name: "n2", configDir: "/h/.claude-accts/dir-two", routed: false },
  ];

  it("KS6：输入框**从不预填** —— 已配置时也一样，要改就重新输", () => {
    const el = renderApikeyEditor(ACCTS[0], () => {}).editor;
    const input = el.querySelector<HTMLInputElement>("input.accounts-row-apikey-input");
    expect(input, "那个输入框不见了 —— 下面的断言会零命中地绿").toBeTruthy();
    expect(input!.value).toBe("");
    // 它是密码框（截图 / 录屏那两个出口）。
    expect(input!.type).toBe("password");
    // 非空对照：这一格**确实**知道「已经配过了」（不是整块空着才让上面恒真）。
    expect(el.textContent).toContain("已经有它那一行");
    expect(input!.placeholder).toContain("替换");
    // 顶层那一把的掩码只在文件那一块（它说的不是任何一个账号）。
    expect(renderApikeyFileBlock(status()).textContent).toContain("已配置");
  });

  it("KS6：界面上只出现掩码，明文一个字节都进不来（类型上就没有那个字段）", () => {
    const el = renderApikeyFileBlock(status({ masked: "sk-a**********WXYZ" }));
    el.appendChild(renderApikeyEditor(ACCTS[0], () => {}).editor);
    expect(el.textContent).toContain("sk-a**********WXYZ");
    const looksLikePlaintext = /sk-[A-Za-z0-9-]{20,}/.test(el.textContent ?? "");
    expect(looksLikePlaintext, `界面上出现了像明文 key 的串：${el.textContent}`).toBe(false);
  });

  it("KS6 机检：输入框的 `.value` **只许被赋成空串**（人群 = 账号分节 ＋ 新建账号表单）", () => {
    // 人群 = 两份生产文件里所有对某个 `…input.value` / `…In.value` 的赋值。
    // A2 之后 key 也会经过新建表单（`account-new-form.ts`）⇒ 那份必须一起进人群。
    const code = src() + "\n" + readFileSync("src/frontend/ui/settings/account-new-form.ts", "utf8");
    const assigns = [...code.matchAll(/(?:input|In)\.value\s*=\s*([^;]+);/g)].map((m) =>
      m[1].trim(),
    );
    expect(assigns.length, "一处赋值都没扫到 —— 抽取器坏了，本条在空转").toBeGreaterThan(3);
    for (const rhs of assigns) {
      expect(
        rhs,
        `输入框被赋了一个不是空串的值（${rhs}）—— 那就是回显。` +
          "KS6 逐字：一旦回显，key 就从「只住在后端」变成「每次打开那个界面都往前端传一遍」。",
      ).toBe('""');
    }
  });

  it("KS11：权限过宽时**在界面上出声**；没问题时不出声", () => {
    const warned = renderApikeyFileBlock(
      status({ notice: "同机器上的别人也读得到它（mode 是 0644…）。怎么修：跑 `chmod 600 …`" }),
    );
    const n = warned.querySelector(".apikey-file-notice");
    expect(n, "过宽了却没在界面上显出来").toBeTruthy();
    expect(n!.textContent).toContain("chmod 600");
    expect(renderApikeyFileBlock(status()).querySelector(".apikey-file-notice")).toBeNull();
  });

  it("KS9：那份文件的路径要显出来 —— 能手编但没人知道在哪 = 不能手编", () => {
    const el = renderApikeyFileBlock(status());
    expect(el.textContent).toContain("apikey-credentials.json");
    expect(el.querySelector(".apikey-file-path")?.getAttribute("title")).toContain("编辑器");
  });

  it("文件读坏了要说出来，**不许静默当成「没配」**", () => {
    const el = renderApikeyFileBlock(
      status({ configured: false, masked: "", problem: "凭据文件不是合法 JSON（…）" }),
    );
    expect(el.querySelector(".apikey-file-problem")?.textContent).toContain("不是合法 JSON");
    expect(renderApikeyFileBlock(status()).querySelector(".apikey-file-problem")).toBeNull();
  });

  it("存一次：明文原样交给回调，交完输入框**立刻清空**", () => {
    const seen: string[] = [];
    const el = renderApikeyEditor(ACCTS[1], (k: string) => {
      seen.push(k);
    }).editor;
    const input = el.querySelector<HTMLInputElement>("input.accounts-row-apikey-input")!;
    input.value = "  sk-ant-TYPED-BY-HAND  ";
    el.querySelector<HTMLButtonElement>("button.accounts-row-apikey-save")!.click();
    expect(seen).toEqual(["sk-ant-TYPED-BY-HAND"]);
    expect(input.value, "存完输入框没清空 —— 明文在 DOM 里留着").toBe("");
    // 空输入不触发（否则会把 key 存成空串，等于悄悄清掉用户的配置）。
    el.querySelector<HTMLButtonElement>("button.accounts-row-apikey-save")!.click();
    expect(seen).toEqual(["sk-ant-TYPED-BY-HAND"]);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // `K-H2c` `KH2C1` ＋ `设计/70 §4.4` 关键二：**「哪个账号」只问一次** ——
  // 配 key 是账号那一行自己的一格；整块里**没有账号下拉**。
  // ───────────────────────────────────────────────────────────────────────────

  it("KH2C1：每一格交出去的是**它那一个账号的 configDir** —— 两个号两个值", () => {
    const seen: Array<[string, string]> = [];
    const onSave = (k: string, d: string) => {
      seen.push([k, d]);
    };
    const one = renderApikeyEditor(ACCTS[0], onSave).editor;
    const two = renderApikeyEditor(ACCTS[1], onSave).editor;
    for (const [el, key] of [
      [one, "sk-ant-FOR-ONE"],
      [two, "sk-ant-FOR-TWO"],
    ] as const) {
      el.querySelector<HTMLInputElement>("input.accounts-row-apikey-input")!.value = key;
      el.querySelector<HTMLButtonElement>("button.accounts-row-apikey-save")!.click();
    }
    expect(seen).toEqual([
      ["sk-ant-FOR-ONE", "/h/.claude-accts/dir-one"],
      ["sk-ant-FOR-TWO", "/h/.claude-accts/dir-two"],
    ]);
  });

  it("KH2C1：状态那一行说的是**这个号**配没配；顶层那一把只在文件那一块 —— 两处分开", () => {
    expect(renderApikeyEditor(ACCTS[0], () => {}).editor.textContent).toContain("n1：API key 表里已经有它那一行");
    // 非空对照：routed=false 那个必须翻面（这把尺子分得出两种结局）。
    expect(renderApikeyEditor(ACCTS[1], () => {}).editor.textContent).toContain("n2：API key 表里还没有它那一行");
    const legacy = renderApikeyFileBlock(status()).querySelector(".apikey-file-legacy");
    expect(legacy?.textContent, "顶层那一把没有单独显").toContain("sk-a**********WXYZ");
    expect(legacy!.textContent).toContain(copyText("accounts.apikeyFile.legacyTop", { masked: "sk-a**********WXYZ" }));
    // 编辑格里**不许**出现顶层那一把 —— 那会被读成这个号的状态。
    expect(renderApikeyEditor(ACCTS[1], () => {}).editor.textContent).not.toContain("sk-a");
  });

  it("🔴 `70 §4.3` ③：整个账号分节里**没有账号下拉** —— 「哪个账号」由那一行回答", async () => {
    fetchAccountsMock.mockResolvedValue(
      state({
        accounts: [
          acct({ name: "n1", configDir: "/h/.claude-accts/dir-one" }),
          acct({ name: "n2", configDir: "/h/.claude-accts/dir-two" }),
          acct({ name: "0", configDir: null, mode: "bare" }),
        ],
        defaultName: "n1",
      }),
    );
    invokeMock.mockImplementation((cmd: string, args: unknown) =>
      Promise.resolve(
        isChanCall(cmd, args, "apikey-read")
          ? chanReply(status())
          : isChanCall(cmd, args, "apikey-routing")
            ? chanReply({ routed: ["/h/.claude-accts/dir-one"], running: true })
            : undefined,
      ),
    );
    const el = await mount();
    expect(el.querySelector("select"), "账号分节里又长出了一个下拉").toBeNull();
    // 有 configDir 的两个号各有一颗按钮、一格编辑器；账号 0 没有（配了也不会被用上）。
    const toggles = [...el.querySelectorAll<HTMLButtonElement>("button.accounts-row-apikey-toggle")];
    expect(toggles.map((b) => b.textContent)).toEqual(["换 API key", "配 API key"]);
    const editors = [...el.querySelectorAll<HTMLElement>(".accounts-row-apikey")];
    expect(editors.length).toBe(2);
    expect(editors.every((e) => e.hidden)).toBe(true);
    toggles[1].click();
    expect(editors[1].hidden).toBe(false);
    expect(editors[1].textContent).toContain("n2");
    // 表外还挂着文件那一块（路径 / 顶层那一把）。
    expect(el.querySelector(".apikey-file-block .apikey-file-path")?.textContent).toContain(
      "apikey-credentials.json",
    );
  });

  it("KH2C1 机检：前端**一个字都不推账号 id** —— 那条规则全仓只有 Rust 那一份", () => {
    const code = src();
    // 人群 = 本文件生产段。针 = 四种「自己取末段名」的常见写法。
    for (const needle of ["split(\"/\")", "split('/')", "basename(", "lastIndexOf(\"/\")"]) {
      expect(
        code.includes(needle),
        `前端出现了 \`${needle}\` —— 那是在长**第二份**「从 configDir 取账号 id」的规则。\n` +
          "`apikey_account_id_of_dir` 的头注逐字：两边各写一个 basename 规则，" +
          "漂开的那天症状是「设置里说走 apikey 端点改写、起会话时没走」，而两边看起来都没错。",
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
      // 〔ST2〕参数里可以多一格 baseUrl（表单那一路）；〔RM1a〕打头的是 origin（按这一页那台机器），
      // 接着照旧是 key 与它自己的 configDir。
      // 〔HX2 · 4D〕今天那一处是经通道的发送口：`writeApikeyKey(这台, configDir, key, baseUrl)`。
      /writeApikeyKey\(this\.machineOrigin\(\),\s*configDir,\s*key\b/.test(code),
      "那条写命令没把 configDir 一起交出去 —— 后端就只能落到顶层那一格",
    ).toBe(true);
  });

  // 〔CFG1〕配置写口从 `save_config`（整份）换成 `patch_config`（按键补丁）〔散文墓碑〕—— 名字跟着换，否则这一格对一个已不存在的命令名恒绿。
  it("KS7 机检：那把 key 在前端**只流向一条命令**，绝不进配置写口 `patch_config`", () => {
    const code = src();
    // ① 前端拿到的明文只出现在一处出口。
    const calls = [...code.matchAll(/commands\.(\w+)\(/g)].map((m) => m[1]);
    // 〔FIX4 · ⑬〕开终端那两处改走 `terminal-open.ts::openTerminal` 之后，本文件零条 `commands.*`（Tauri 命令）⇒
    //   「抽取器坏了」那一格换成两向：零条 ⇔ 本文件不 import `commands`（import 了却一条没扫到 = 抽取器瞄偏了）。
    expect(calls.length > 0, "扫到的 `commands.*` 条数与本文件 import 不 import `commands` 对不上 —— 抽取器坏了").toBe(
      code.includes('from "../ipc/commands"'),
    );
    // 〔HX2 · 4D〕那一处出口今天是经通道的发送口（`writeApikeyKey`），恰好一处；Tauri 那条写命令不在了。
    expect([...code.matchAll(/\bwriteApikeyKey\(/g)].length, "key 的出口不是恰好一处").toBe(1);
    expect(calls, "Tauri 那条写 key 的命令回来了").not.toContain("write_apikey_" + "credentials_key");
    expect(
      calls.filter((c) => c === "patch_config"),
      "账号这一组里出现了 `patch_config` —— key 有可能被塞进前端的那份配置（config.json）",
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
    setCurrentMachine(LOCAL_ORIGIN); // 一台远端都没配 ⇒ 只有本机那一页
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

  // ---- 〔AL1 · 2026-09-24〕`K-R49` 那一块搬走了：这一节里**一块别名都不许有** ----

  /**
   * 🔴 从前这里钉的是反方向（「本机有账号时这一节长出『按账号生成命令』那一块」）。
   * `设计/70 §3.3` · `设计/71 §13`：别名是每台机器一份的东西，并进机器页「本机 → 工具 → 别名」；
   * 清单归用户，不再是账号表的投影（`71 §8`）⇒ 账号这一节不再摆它，也不再有那句把人指去
   * 「设置 → 行为」的散文（`70` 第三刀步 12）。接线的正面（本机页上**有**那一块）归 `panel-groups.vitest.ts`。
   */
  it("★ AL1：本机有账号时，这一节里既没有别名那一块，也没有指去别处的那句散文", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(three());
    const el = await mount();
    expect(el.querySelector(".ccm-acct-alias, .machine-aliases")).toBeNull();
    expect(el.textContent ?? "").not.toContain("按账号生成命令");
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
    setCurrentMachine("aya"); // 〔第三波 S3〕站到 aya 那一页上（上面的 `noRemotes()` 把 store 置回了本机）
    fetchAccountsMock.mockResolvedValue(state({ available: false, oldBackend: true, error: "backend 过旧" }));
    const remoteEl = await mount();
    expect(
      (remoteEl.textContent ?? "").includes("远端"),
      "远端那一支上也找不到「远端」—— 上面那把尺子量不到东西",
    ).toBe(true);
  });

  it("★ NF1bD2：本机文案表里逐条不含「远端」（分母 = 表的条目数，现算）", () => {
    // 〔第三波 S3〕人群加上文案表里本机那一支的条目（`accountsLocal.*`）—— 两个家都是本机的家。
    const table = [...Object.entries(accounts.LOCAL_ACCOUNTS_COPY), ...localCopyEntries()];
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
      // ①b 〔第三波 S3〕文案表里本机那一支的条目（`accountsLocal.*`）：带占位符的按占位符切成段
      ...localCopyFragments(),
      // ①c 〔第三波 S3〕本机也挂了新建账号那张表单 —— 它的字住它自己那张表（`NEW_ACCOUNT_COPY`），
      //     那也是**一个家**（远端那一页用的是同一张）。两句「弹出终端」的提示本机换掉了（见 ①b）。
      ...Object.values(NEW_ACCOUNT_COPY),
      // ② 徽章那一族：家在 accounts.ts，逐个账号现算
      ...st.accounts.flatMap((a) => {
        const b = accounts.accountStatusBadge(a);
        return [b.text, b.title];
      }),
      // ②b 行上的动作与维护区：家在文案表（`accounts.*`，与远端那一页同一套）与 `accounts.ts::accountLoginActionLabel`
      ...st.accounts.map((a) => accounts.accountLoginActionLabel(a).label),
      copyText("accounts.row.remove"),
      copyText("accounts.maintenance.title"),
      copyText("accounts.maintenance.verify"),
      copyText("accounts.maintenance.sync"),
      copyText("accounts.maintenance.rollback"),
      copyText("accounts.maintenance.syncHint"),
      copyText("accounts.maintenance.rollbackHint"),
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

  it("★ NF1bD3：站在 aya 那一页上照旧走远端那条读口，本机那一支一格不长", async () => {
    // 〔第三波 S3〕题面原为「配了远端时照旧走远端那条读口」—— 那是「store 为空就兜底读主远端」
    // 那一形的口径；兜底删了之后，「走不走远端」由**你站在哪一页**决定，不由「配没配远端」决定。
    // 反方向（本机页 + 配了远端 ⇒ 本机那一支）见下一条。
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
// 〔第三波 S3〕**本机页就是本机** —— 配了远端的机器上，本机那一页这一节原先显的是主远端的账号。
//
// 病：`init` 在 store 为 `null` 时兜底取主远端（E59 的「兜底落点」），`followMachine(null)` 原地不动。
// 而 store 的初值恰好是 `null`、本机页一出现 per-machine 那几块就落在它上面 ⇒ 配了远端的机器上，
// 本机那一支（连同 A3 那两条本机命令）一次都走不到；从 aya 那一页切回本机页，这一节也还停在 aya。
// ─────────────────────────────────────────────────────────────────────────────
describe("S3：本机页就是本机（配了远端也一样）", () => {
  it("★ 本机页（store = null）＋ 配了远端 ⇒ 走本机那条读口，远端读口一次不调", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host()] });
    setCurrentMachine(LOCAL_ORIGIN);
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    expect(fetchLocalAccountsMock, "本机页上没去读本机的账号").toHaveBeenCalled();
    expect(fetchAccountsMock, "本机页上去读了远端的账号").not.toHaveBeenCalled();
    expect(el.querySelector(".accounts-local"), "本机页上没有本机那一块").not.toBeNull();
    expect(el.querySelector(".accounts-table"), "本机页上长出了远端那张表").toBeNull();
  });

  it("★ 从 aya 那一页切回本机页 ⇒ 这一节跟着切到本机（不停在 aya）", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host()] });
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    // 前提：确实先站在 aya 上（否则下面那条「切回来」是空真）。
    expect(el.querySelector(".accounts-table"), "前提：先得在 aya 那一页上").not.toBeNull();
    expect(fetchLocalAccountsMock).not.toHaveBeenCalled();
    setCurrentMachine(LOCAL_ORIGIN);
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(fetchLocalAccountsMock, "切回本机页后没去读本机").toHaveBeenCalled();
    expect(el.querySelector(".accounts-local"), "切回本机页后这一节还停在 aya").not.toBeNull();
    expect(el.querySelector(".accounts-table")).toBeNull();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 〔第三波 S3〕本机那一支的空态：没启用 ⇒ 就地启用（问本机后端 `accounts-init`）；启用着但零个号 ⇒ 新建表单。
// 一件要装的东西都没有、一条要人手敲的维护命令都没有。
// ─────────────────────────────────────────────────────────────────────────────
describe("S3：本机那一支的空态就地可用", () => {
  const tick = () => new Promise((r) => setTimeout(r, 0));
  function noRemotes(): void {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    setCurrentMachine(LOCAL_ORIGIN); // 一台远端都没配 ⇒ 只有本机那一页
  }
  const off = (): AccountsState => localState({ accounts: [], meta: { ...state({}).meta!, enabled: false } });

  it("★ 没启用 ⇒ 本机那一块里就是启用向导（引言是本机的话），没有「先装」那一句", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(off());
    const el = await mount();
    const box = el.querySelector(".accounts-local")!;
    expect(box.querySelector(".accounts-wizard"), "本机没启用时没有启用向导").not.toBeNull();
    expect(box.querySelector(".accounts-ne-title")?.textContent).toBe(copyText("accountsLocal.init.title"));
    expect(box.textContent ?? "").not.toContain("远端");
    expect(box.querySelector(".accounts-needs-deploy")).toBeNull();
  });

  it("★ 没启用 · 填名字点启用 ⇒ 问的是**本机**那台后端 accounts-init（先预演、确认后才做）", async () => {
    noRemotes();
    const calls: Array<[string, unknown]> = [];
    invokeMock.mockImplementation((cmd: unknown, args: unknown) => {
      calls.push([cmd as string, args]);
      return isAccountsOp(cmd as string, args) ? accountsFakeInvoke(args) : Promise.resolve(undefined);
    });
    fetchLocalAccountsMock.mockResolvedValue(off());
    const el = await mount();
    const input = el.querySelector<HTMLInputElement>(".accounts-local .accounts-wiz-name")!;
    input.value = "main";
    input.dispatchEvent(new Event("input"));
    for (let i = 0; i < 3; i++) await tick();
    el.querySelector<HTMLButtonElement>(".accounts-local .accounts-wiz-btns button")!.click();
    for (let i = 0; i < 6; i++) await tick();
    expect(accountOps(calls)).toEqual([
      ["accounts-init", LOCAL_ORIGIN, { name: "main", dryRun: true }],
      ["accounts-init", LOCAL_ORIGIN, { name: "main" }],
    ]);
  });

  it("★ 启用着但零个号 ⇒ 下一步那一句 ＋ 本机的新建表单", async () => {
    noRemotes();
    fetchLocalAccountsMock.mockResolvedValue(localState({ accounts: [] }));
    const el = await mount();
    expect(el.querySelector(".accounts-local-empty-next")?.textContent).toBe(accounts.LOCAL_ACCOUNTS_COPY.emptyNext);
    expect(el.querySelector(".accounts-local .accounts-new"), "零个号时没有新建表单").not.toBeNull();
    expect(el.querySelector(".accounts-local .accounts-wizard")).toBeNull();
  });

  it("★ 有号 ⇒ 本机那一块也有维护区（核对问的是本机后端），没有「生成 rc 片段」", async () => {
    noRemotes();
    const calls: Array<[string, unknown]> = [];
    invokeMock.mockImplementation((cmd: unknown, args: unknown) => {
      calls.push([cmd as string, args]);
      return isAccountsOp(cmd as string, args) ? accountsFakeInvoke(args) : Promise.resolve(undefined);
    });
    fetchLocalAccountsMock.mockResolvedValue(localState({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    expect(el.querySelector(".accounts-local-rc")).toBeNull();
    const verify = [...el.querySelectorAll<HTMLButtonElement>(".accounts-local .accounts-maint-ops button")].find(
      (b) => b.textContent === copyText("accounts.maintenance.verify"),
    )!;
    verify.click();
    for (let i = 0; i < 3; i++) await tick();
    expect(accountOps(calls)).toEqual([["accounts-verify", LOCAL_ORIGIN, {}]]);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 〔第三波 S3〕本机清单上的徽章接上本机那一半的两格事实（〔US1〕经通道 `apikey-routing`）。
// `accountStatusBadge` 的 `{ scope: "local" }` 三档自 `K-H2b` 起「有实现、没接线」。
// ─────────────────────────────────────────────────────────────────────────────
describe("S3：本机清单的徽章说本机那一半的真话", () => {
  const KEYED = acct({
    name: "k",
    email: "k@x.edu",
    configDir: "/h/.claude-accts/k",
    loggedIn: false,
    authKind: "api-key",
    authReady: true,
  });
  async function badgeWith(routing: (() => Promise<unknown>) | null): Promise<HTMLElement> {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    setCurrentMachine(LOCAL_ORIGIN);
    fetchLocalAccountsMock.mockResolvedValue(localState({ accounts: [KEYED], defaultName: "k" }));
    invokeMock.mockImplementation((cmd: string, args: unknown) =>
      isChanCall(cmd, args, "apikey-routing") && routing
        ? routing().then(chanReply)
        : Promise.resolve(undefined),
    );
    const el = await mount();
    return el.querySelector<HTMLElement>(".accounts-local-row-badge")!;
  }

  it("★ 表里有它 ＋ 中转在跑 ⇒ 「经本机中转」；只问本机这几个号的目录", async () => {
    const b = await badgeWith(() => Promise.resolve({ routed: [KEYED.configDir], running: true }));
    expect(b.textContent).toBe(
      accounts.accountStatusBadge(KEYED, { scope: "local", hasRow: true, running: true }).text,
    );
    const asked = invokeMock.mock.calls.filter(([c, a]) => isChanCall(c as string, a, "apikey-routing"));
    expect(asked).toHaveLength(1);
    // 本机这一页问的仍是本机（逐字送后端那个本机串）；`agent` 随请求带（后端不猜是哪一家）。
    const a = asked[0][1] as Parameters<typeof chanArgsJson>[0];
    expect(a.origin).toBe(LOCAL_ORIGIN);
    expect(chanArgsJson(a)).toEqual({ agent: "claude-code", configDirs: [KEYED.configDir] });
  });

  it("★ 表里有它 ＋ 中转没跑 ⇒ 「中转未运行」；表里没它 ⇒ 说表里没它 —— 三档两两不同", async () => {
    const seen = [
      await badgeWith(() => Promise.resolve({ routed: [KEYED.configDir], running: true })),
      await badgeWith(() => Promise.resolve({ routed: [KEYED.configDir], running: false })),
      await badgeWith(() => Promise.resolve({ routed: [], running: true })),
    ].map((b) => `${b.textContent}|${b.title}`);
    expect(new Set(seen).size, seen.join("\n")).toBe(3);
    expect(seen[1]).toContain(
      accounts.accountStatusBadge(KEYED, { scope: "local", hasRow: true, running: false }).text,
    );
    expect(seen[2]).toContain(
      accounts.accountStatusBadge(KEYED, { scope: "local", hasRow: false, running: true }).title,
    );
  });

  it("★ 问不到（抛错 / 形状不对）⇒ 不替它下判断：与「没被告知」那一支逐字相同，不当成「表里没有」", async () => {
    const untold = accounts.accountStatusBadge(KEYED);
    for (const r of [
      () => Promise.reject(new Error("后端不在")),
      () => Promise.resolve(undefined),
      () => Promise.resolve({ routed: "x" }),
    ]) {
      const b = await badgeWith(r);
      expect(b.textContent).toBe(untold.text);
      expect(b.title).toBe(untold.title);
    }
  });

  it("★ 本机那三档上屏的字里没有「远端」", async () => {
    for (const r of [
      () => Promise.resolve({ routed: [KEYED.configDir], running: true }),
      () => Promise.resolve({ routed: [KEYED.configDir], running: false }),
      () => Promise.resolve({ routed: [], running: true }),
    ]) {
      const b = await badgeWith(r);
      expect(`${b.textContent}${b.title}`.length).toBeGreaterThan(5);
      expect(`${b.textContent}${b.title}`).not.toContain("远端");
    }
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// `N-F2`：**本机也进「还差什么」那张清单** —— 那张清单本来就把本机算进去了，缺的是写点。
//
// 病（件文件 `§0a` / 定框 `N2` 09-05 订正段，本族开工时逐条现打复核过）：
// `computeGaps` 的入参里本来就有 `LOCAL_MACHINE_KEY`，而 `notApplicable` 只把本机的
// `backend` / `connection` 排掉 ⇒ 本机的账号那一格是**适用**的；
// 可它的唯一写点 `AccountsSection.note()` 第一行是 `if (!this.origin) return`，
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
    setCurrentMachine(LOCAL_ORIGIN); // 一台远端都没配 ⇒ 只有本机那一页
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
    // 那两格是**适用**的：`notApplicable` 今天只排掉本机的 connection（〔WF1 · ㉔〕Windows 上的 ccm 豁免撤了）。
    // ⚠ `K-R59`（09-11）**多出第三格 `backend`**：那条「本机不需要后端」的豁免撤了
    //（`C7` 之后本机也有后端进程）。它**不归本分节写** —— 写点住
    // `remote-section.ts::noteLocalBackend`，由 `remote-section.vitest.ts` 那一族接。
    const before = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: readStatus,
    });
    expect(
      before.map((g) => `${g.facet}:${g.kind}`),
      "本机的适用格不是恰好这三格 —— 下面几条的题面就得重写",
    ).toEqual(["backend:unknown", "ccm:unknown", "accounts:unknown"]);
  });

  // ---- 三档各写各的 ----

  it("★ NF2D2 档一（读出来了·有号）：账号那格 ok，且说得出是几个", async () => {
    const led = await localLedgerAfter(threeLocal());
    expect(shape(led)).toEqual({ accounts: { kind: "ok", detail: "3 个" } });
  });

  it("★ NF2D2 档一（读出来了·零个号）：清单读到了 = ok（与「读不出来」不是一回事）", async () => {
    const led = await localLedgerAfter(localState({ accounts: [] }));
    expect(shape(led)).toEqual({ accounts: { kind: "ok", detail: "已读取" } });
  });

  it("★ NF2D2 档二（后端不在）：fail，且 detail 说得出是这一档", async () => {
    const led = await localLedgerAfter(
      localState({ available: false, error: "本机后端不在，问不出…", accounts: [] }),
    );
    expect(shape(led)).toEqual({ accounts: { kind: "fail", detail: "后端不在" } });
  });

  it("★ NF2D2 档三（读不动）：fail，且 detail 与上一档**不同**", async () => {
    const led = await localLedgerAfter(new Error("backend down"));
    expect(shape(led)).toEqual({ accounts: { kind: "fail", detail: "读不动" } });
  });

  /** 〔VIS2 · `设计/15 §4.5` 缺口二〕「启用没启用」记在 `accounts`（没启用 ⇒ fail「多账号没启用」）。远端本机逐行，期望逐行手写。 */
  it("★ VIS2 缺口二：启用没启用记在 accounts —— 远端本机逐行", async () => {
    const off = (p: Partial<AccountsState> = {}) => ({
      ...p,
      meta: { ...state({}).meta!, enabled: false },
    });
    const remote = async (st: AccountsState) => {
      localStorage.clear();
      readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host()] });
      setCurrentMachine("aya");
      fetchAccountsMock.mockReset().mockResolvedValue(st);
      await mount();
      return shape(readStatus(host().label));
    };
    const local = async (st: AccountsState) => {
      localStorage.clear();
      return shape(await localLedgerAfter(st));
    };
    const multiOff = { kind: "fail", detail: "多账号没启用" };
    const got = [
      await remote(state(off())),
      await remote(state({ accounts: [acct({ name: L1 })], defaultName: L1 })),
      await local(localState(off())),
      await local(threeLocal()),
    ];
    expect(got).toEqual([
      { accounts: multiOff },
      { accounts: { kind: "ok", detail: "1 个" } },
      { accounts: multiOff },
      { accounts: { kind: "ok", detail: "3 个" } },
    ]);
  });

  it("★ NF2D2 三档两两不同 —— 只断「调了 recordFacet」的话，三档写成同一个值也全绿", async () => {
    // 分母：**面板看得见的那三档**（读出来了 / 后端不在 / 读不动），逐档各跑一次真面板。
    // ⚠ 诚实边界：后端那侧其实是三档（`Listed` / `NoBackend` / `Unreadable`），
    // 而 `NoBackend` 与 `Unreadable` 到前端都变成 `available:false` + 一句 error 文案，
    // 前端分不出来 —— 分开它们要后端多带一个字段回来（`src/frontend/shell`，本件射程外）。
    const cases: Array<[string, AccountsState | Error]> = [
      ["读出来了", threeLocal()],
      ["后端不在", localState({ available: false, error: "x", accounts: [] })],
      ["读不动", new Error("backend down")],
    ];
    const seen: string[] = [];
    for (const [, st] of cases) {
      const led = await localLedgerAfter(st);
      // 那一格得写到 —— 漏了，它就还停在「没测过」。
      expect(led.accounts, "accounts 这一格没被写").toBeDefined();
      seen.push(JSON.stringify(shape(led)));
    }
    expect(seen.length, "分母塌了 —— 一档都没跑").toBe(3);
    expect(
      new Set(seen).size,
      `三档在账本上写成了同一个样子：${seen.join(" | ")}`,
    ).toBe(3);
  });

  /**
   * ⚠ 射程逐字写清（`K-R59` 09-11 收窄）：本条只管**本分节负责的那一格**
   * （`accounts`）。本机的 `backend` 从 09-11 起也是一格适用的，
   * 但它的写点在 `remote-section.ts::noteLocalBackend` —— 这一族**一次都没跑过它**，
   * 把它算进来只会得到一条恒红，而且红的是别人的账。
   */
  it("★ NF2D2 本机侧总账：三档跑完，本分节那一格**没有一档**还停在「没测过」", async () => {
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
      });
      // 「没测过」= `unknown`。测过了但确认缺（`missing`）是另一回事，这条不管那个。
      const mine = new Set(["accounts"]);
      expect(
        gaps
          .filter((g) => g.kind === "unknown" && mine.has(g.facet))
          .map((g) => g.facet),
        `跑完之后本机还有格子停在「没测过」（账本：${JSON.stringify(shape(led))}）`,
      ).toEqual([]);
      // 阴性对照：这把尺子不是恒空 —— 射程外那一格今天确实停在「没测过」，
      // 而那正是**另一个写点**的活（`remote-section.ts::noteLocalBackend`）。
      expect(gaps.map((g) => g.facet)).toContain("backend");
    }
  });

  // ---- 远端那条路一个字节不变 ----

  it("★ NF2D2 远端侧：远端那五支写进账本的东西逐格不变，且本机那一栏一格不长", async () => {
    // 分母 = 远端那条路今天**全部**六支（`reload` 里 catch + `deriveUi` 的各个 kind；〔WF2〕多了 query-failed 一支），
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
          state({ available: false, oldBackend: true, error: "backend 过旧", accounts: [] }),
        ),
      ),
      "老后端那一支",
    ).toEqual({ accounts: { kind: "fail", detail: "后端需更新" } });

    // 〔WF2 · WIN3 读数 C〕第六支：没问出来 ⇒ 说查询失败，不说需更新。
    expect(
      await remote(() =>
        fetchAccountsMock.mockResolvedValue(
          state({ available: false, error: "现在够不着那台机器的后端", accounts: [] }),
        ),
      ),
      "查询失败那一支",
    ).toEqual({ accounts: { kind: "fail", detail: "没查到" } });

    expect(
      await remote(() => fetchAccountsMock.mockResolvedValue(state({ accounts: [] }))),
      "未启用那一支",
    ).toEqual({ accounts: { kind: "ok", detail: "已读取" } });

    expect(
      await remote(() =>
        fetchAccountsMock.mockResolvedValue(
          state({ accounts: [acct({ name: L1 }), acct({ name: L2 })], defaultName: L1 }),
        ),
      ),
      "已启用那一支",
    ).toEqual({ accounts: { kind: "ok", detail: "2 个" } });
  });

  // ---- `NF2D3`：从**真的一次面板运行**接到「整块该不该出现」 ----

  it("★ NF2D3：本机全绿 + 一台远端都没有 ⇒ summarizeGaps 返回 null（那一块整块不出现的前提）", async () => {
    // 分母写清（`NF2D3` 的 acceptor 逐字要求，防「一台机器都没有」蒙混）：
    //   · 机器数 = 1（本机），**不是空清单**；
    //   · 这台机的适用格 = { backend, ccm, accounts }（connection 不适用）—— 上面那条「地板」用例已把这个集合逐项断过；
    //   · `accounts` 那一格由**真的一次面板运行**写绿，账本不是手工摆出来的。
    const led = await localLedgerAfter(threeLocal());
    expect(shape(led), "前提没成立：这一次面板运行没把那一格写绿").toEqual({
      accounts: { kind: "ok", detail: "3 个" },
    });
    // ⚠ 第三格（`K-R59` 新算数的 `backend`）**不归本分节写**：它的生产写点是
    //   `remote-section.ts::noteLocalBackend`，由 `remote-section.vitest.ts` 用真的一次
    //   面板运行钉着。这里手工补上它，**只是为了让「整块该不该出现」这一跳还量得动** ——
    //   如实说明：这一格是摆出来的，不是本族跑出来的。
    recordFacet(LOCAL_MACHINE_KEY, "backend", { kind: "ok", detail: "已连上" });
    // 〔WF1 · ㉔〕`ccm` 那一格同理（写点 `machine-aliases.ts::noteLocalCcm`，由 `remote-section.vitest.ts` 钉），摆出来的。
    recordFacet(LOCAL_MACHINE_KEY, "ccm", { kind: "ok", detail: "是它" });
    const origins = [LOCAL_MACHINE_KEY];
    expect(origins.length, "分母是空的 —— 下面那条 null 是空真").toBe(1);
    const gaps = computeGaps({ origins, statusOf: readStatus });
    expect(gaps).toEqual([]);
    expect(
      summarizeGaps(gaps),
      "本机全绿、没有远端，那张清单却还有话说 ⇒「全绿就整块不出现」仍是死代码",
    ).toBeNull();
  });

  it("★ NF2D3 先证会红：把那几格改回「没测过」⇒ 清单又出现，且逐字写着「没测过」", async () => {
    await localLedgerAfter(threeLocal());
    // 只把本机那一栏抹掉 = 回到本件之前的行为（那几格从来没人写）。
    localStorage.clear();
    expect(readStatus(LOCAL_MACHINE_KEY)).toEqual({});
    const gaps = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: readStatus,
    });
    expect(gaps.map((g) => g.facet)).toEqual(["backend", "ccm", "accounts"]);
    const s = summarizeGaps(gaps);
    expect(s).not.toBeNull();
    expect(s, "回到旧行为时它该说「还没测过」").toContain("还没测过");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// A2（`设计/70 §8` 判据 #9）：**账号一步建成** —— 填名字 + 选「第三方 apikey」+ 填 key → 点创建
// ⇒ 那台后端一趟做完（建目录 · 链接 · 清单 · key 进 apikey 表 · 别名）。**中途不需要去第二个控件选账号，也不开终端。**
// ─────────────────────────────────────────────────────────────────────────────
describe("A2：新建账号一张表单 ⇒ 那台后端一趟建好", () => {
  const tick = () => new Promise((r) => setTimeout(r, 0));

  function wire(over: Record<string, unknown> = {}, launchFails = false) {
    const calls: Array<[string, unknown]> = [];
    invokeMock.mockImplementation((cmd: unknown, args: unknown) => {
      calls.push([cmd as string, args]);
      if (cmd === "launch_remote_terminal" && launchFails) return Promise.reject(new Error("没有终端"));
      if (isChanCall(cmd as string, args, "apikey-read")) {
        return Promise.resolve(chanReply({ configured: false, masked: "", path: "/h/x.json", notice: null, problem: null }));
      }
      if (isAccountsOp(cmd as string, args)) {
        return accountsFakeInvoke(args, (chanArgsJson(args as ChanCallArgs) as { dryRun?: boolean }).dryRun ? {} : over);
      }
      return Promise.resolve(undefined);
    });
    return calls;
  }

  async function fill(el: HTMLElement, name: string, opts: { key?: string; baseUrl?: string; cred?: string; def?: boolean } = {}): Promise<HTMLElement> {
    const form = el.querySelector<HTMLElement>(".accounts-new")!;
    const nameIn = form.querySelector<HTMLInputElement>("input.accounts-maint-name")!;
    nameIn.value = name;
    nameIn.dispatchEvent(new Event("input"));
    if (opts.key !== undefined) {
      const r = form.querySelector<HTMLInputElement>('input[type=radio][value="apikey"]')!;
      r.checked = true;
      r.dispatchEvent(new Event("change"));
      if (opts.baseUrl !== undefined) {
        const baseIn = form.querySelector<HTMLInputElement>('.accounts-new-key input[data-field="base-url"]')!;
        baseIn.value = opts.baseUrl;
        baseIn.dispatchEvent(new Event("input"));
      }
      const keyIn = form.querySelector<HTMLInputElement>(".accounts-new-key input[type=password]")!;
      keyIn.value = opts.key;
      keyIn.dispatchEvent(new Event("input"));
    }
    if (opts.cred !== undefined) {
      const credIn = form.querySelector<HTMLInputElement>(".accounts-new-adv input")!;
      credIn.value = opts.cred;
      credIn.dispatchEvent(new Event("input"));
    }
    if (opts.def) {
      const d = form.querySelector<HTMLInputElement>('input[data-field="is-default"]')!;
      d.checked = true;
      d.dispatchEvent(new Event("change"));
    }
    await tick(); // 等表单问完那台后端预演（答回来之前「创建」是灰的）
    await tick();
    return form;
  }
  async function create(form: HTMLElement): Promise<void> {
    [...form.querySelectorAll("button")].find((b) => b.textContent === NEW_ACCOUNT_COPY.create)!.click();
    for (let i = 0; i < 6; i++) await tick();
  }

  it("★ 判据 #9：API 号一趟建好 —— 预演不带 key，真建那一发带 key；不开终端、不另写 key、没有下拉", async () => {
    const calls = wire();
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    const form = await fill(el, "b", { key: "sk-ant-FOR-B", baseUrl: "https://api.example.com/v1" });
    expect(form.querySelector(".accounts-wiz-preview")?.textContent).toContain("（预演）accounts-add b");
    expect(form.textContent, "别名名字该是那台后端答的").toContain("bcc");
    await create(form);
    expect(accountOps(calls).filter(([, , a]) => a.dryRun !== true)).toEqual([
      ["accounts-add", "aya", { name: "b", kind: "api-key", baseUrl: "https://api.example.com/v1", key: "sk-ant-FOR-B" }],
    ]);
    for (const [, , a] of accountOps(calls).filter(([, , a]) => a.dryRun === true)) {
      expect(a.key, "预演那几发带上了 key 的明文").toBeUndefined();
    }
    expect(calls.some(([c, a]) => isKeySet(c, a)), "key 又走了第二条写口").toBe(false);
    expect(calls.some(([c]) => c === "launch_remote_terminal"), "API 号不需要终端").toBe(false);
    expect(el.textContent, "key 的明文上了屏").not.toContain("sk-ant-FOR-B");
    expect(el.textContent, "Base URL 输入框没清空、还挂在屏上").not.toContain("api.example.com");
    expect(el.querySelector("select")).toBeNull();
  });

  it("★ 订阅号没导入凭据 ⇒ 建好后在终端里起 claude 登录，那一行是后端答的", async () => {
    const calls = wire();
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    await create(await fill(el, "b", { def: true }));
    expect(accountOps(calls).filter(([, , a]) => a.dryRun !== true)).toEqual([
      ["accounts-add", "aya", { name: "b", kind: "subscription", isDefault: true }],
    ]);
    const launches = calls.filter(([c]) => c === "launch_remote_terminal");
    expect(launches.map(([, a]) => (a as { remoteCmd: string }).remoteCmd)).toEqual([fakeLoginCmd("b")]);
  });

  it("订阅号从旧凭据导入 ⇒ 路径原样交给那台后端，不开终端", async () => {
    const calls = wire({ loginCmd: null });
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    await create(await fill(el, "b", { cred: "~/snap/b.json" }));
    expect(accountOps(calls).filter(([, , a]) => a.dryRun !== true)).toEqual([
      ["accounts-add", "aya", { name: "b", kind: "subscription", credFile: "~/snap/b.json" }],
    ]);
    expect(calls.some(([c]) => c === "launch_remote_terminal")).toBe(false);
  });

  it("号建好了、key 没写进去 ⇒ 说出来（错误提示），不假装成了", async () => {
    wire({ keyMasked: null, keyProblem: "账号建好了，API key 没存进去：盘满了" });
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    await create(await fill(el, "b", { key: "sk-1" }));
    const toasts = vi.mocked(showActionFailureToast).mock.calls;
    expect(toasts.some((c) => String(c[1]).includes("盘满了") && (c[2] as { level?: string } | undefined)?.level === "error")).toBe(true);
  });

  it("那台后端预演就拒了（比如重名）⇒ 那一句上屏、「创建」一直是灰的", async () => {
    invokeMock.mockImplementation((cmd: unknown, args: unknown) =>
      isAccountsOp(cmd as string, args) ? Promise.reject({ err: "Refused", body: Array.from(new TextEncoder().encode(JSON.stringify({ code: "refused", message: "已经有叫 z 的账号了。" }))) }) : Promise.resolve(undefined),
    );
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    const form = await fill(el, "z");
    expect(form.querySelector(".accounts-maint-err")?.textContent).toContain("已经有叫 z 的账号了。");
    const btn = [...form.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === NEW_ACCOUNT_COPY.create)!;
    expect(btn.disabled).toBe(true);
  });

  it("删号：确认（默认号多说一句、带 force）→ 那台后端删", async () => {
    const calls = wire();
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "z", isDefault: true }), acct({ name: "b", configDir: "/h/.claude-accts/b" })], defaultName: "z" }),
    );
    const el = await mount();
    const dels = [...el.querySelectorAll<HTMLButtonElement>("button.danger")];
    expect(dels.map((b) => b.textContent)).toEqual([copyText("accounts.row.remove"), copyText("accounts.row.remove")]);
    dels[0].click();
    for (let i = 0; i < 6; i++) await tick();
    expect(String(askConfirmMock.mock.calls[0][0])).toContain("默认账号");
    expect(accountOps(calls)).toEqual([["accounts-remove", "aya", { name: "z", force: true }]]);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 〔第三波 S3〕本机页也能新建账号：同一张表单、同一条 `accounts-add`，问的是**本机**后端。
// 订阅号要登录时，本机 Linux 刻意不开终端窗口（后端回一句带标记的话）⇒ 登录那一行复制给用户在自己的终端里跑。
// ─────────────────────────────────────────────────────────────────────────────
describe("S3：本机页新建账号", () => {
  const tick = () => new Promise((r) => setTimeout(r, 0));
  /** 后端在 POSIX 本机上回的那句话（带跨语言标记，唯一出处在 `launch.rs`）。 */
  const NO_WINDOW = `本机不是 Windows：cc-monitor ${POSIX_NO_WINDOW_MARKER}（会话容器是 tmux）`;

  function wire(launch: "ok" | "no-window" | "fail") {
    const calls: Array<[string, unknown]> = [];
    invokeMock.mockImplementation((cmd: unknown, args: unknown) => {
      calls.push([cmd as string, args]);
      if (cmd === "launch_remote_terminal") {
        if (launch === "no-window") return Promise.reject(NO_WINDOW);
        if (launch === "fail") return Promise.reject(new Error("没有终端"));
      }
      if (isAccountsOp(cmd as string, args)) return accountsFakeInvoke(args);
      return Promise.resolve(undefined);
    });
    return calls;
  }
  const writeText = vi.fn();
  async function mountLocal(accts: Account[]): Promise<HTMLElement> {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    setCurrentMachine(LOCAL_ORIGIN);
    writeText.mockReset().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    fetchLocalAccountsMock.mockResolvedValue(localState({ accounts: accts, defaultName: "z" }));
    return mount();
  }
  async function submit(el: HTMLElement, name: string, key?: string): Promise<void> {
    const form = el.querySelector<HTMLElement>(".accounts-local .accounts-new")!;
    const nameIn = form.querySelector<HTMLInputElement>("input.accounts-maint-name")!;
    nameIn.value = name;
    nameIn.dispatchEvent(new Event("input"));
    if (key !== undefined) {
      const r = form.querySelector<HTMLInputElement>('input[type=radio][value="apikey"]')!;
      r.checked = true;
      r.dispatchEvent(new Event("change"));
      const keyIn = form.querySelector<HTMLInputElement>(".accounts-new-key input[type=password]")!;
      keyIn.value = key;
      keyIn.dispatchEvent(new Event("input"));
    }
    await tick(); // 等表单问完本机后端预演
    await tick();
    [...form.querySelectorAll("button")].find((b) => b.textContent === NEW_ACCOUNT_COPY.create)!.click();
    for (let i = 0; i < 6; i++) await tick();
  }

  it("★ 本机有号 ⇒ 本机那一块里有新建表单；提示是本机的话；「只读」那句撤了", async () => {
    wire("ok");
    const el = await mountLocal([acct({ name: "z" })]);
    const form = el.querySelector(".accounts-local .accounts-new");
    expect(form, "本机页没有新建账号表单").not.toBeNull();
    expect(form!.querySelector(".accounts-new-hint:not(:empty)")?.textContent).toBe(
      copyText("accountsLocal.new.subscriptionHint"),
    );
    expect(el.textContent ?? "").not.toContain(NEW_ACCOUNT_COPY.subscriptionHint);
    expect(el.textContent ?? "").not.toContain(accounts.LOCAL_ACCOUNTS_COPY.scopeHint);
    expect(el.querySelector(".accounts-local-hint")?.textContent).toBe(copyText("accountsLocal.list.scope"));
  });

  it("阴性对照：远端页上那张表单的提示一个字没变（换提示只发生在本机那一页）", async () => {
    wire("ok");
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    const form = el.querySelector(".accounts-new")!;
    expect(form.querySelector(".accounts-new-hint:not(:empty)")?.textContent).toBe(
      NEW_ACCOUNT_COPY.subscriptionHint,
    );
  });

  it("★ 订阅 ⇒ 问的是**本机**那台后端 accounts-add；登录那一行交给本机开终端那一步", async () => {
    const calls = wire("ok");
    const el = await mountLocal([acct({ name: "z" })]);
    await submit(el, "b");
    expect(new Set(accountOps(calls).map(([, o]) => o))).toEqual(new Set([LOCAL_ORIGIN]));
    const launches = calls.filter(([c]) => c === "launch_remote_terminal");
    // 〔FIX4 · ⑬〕开终端那一口（`openTerminal`）恒交 `rbindToken`（登录那一行不需要 ⇒ `null`）。
    expect(launches.map(([, a]) => a)).toEqual([{ origin: LOCAL_ORIGIN, remoteCmd: fakeLoginCmd("b"), rbindToken: null }]);
  });

  it("★ Linux（后端说「刻意不开窗口」）⇒ 登录那一行复制给用户，提示是 info", async () => {
    wire("no-window");
    const toast = vi.mocked(showActionFailureToast);
    toast.mockClear();
    const el = await mountLocal([acct({ name: "z" })]);
    await submit(el, "b");
    expect(writeText).toHaveBeenCalledWith(fakeLoginCmd("b"));
    const c = toast.mock.calls.at(-1)!;
    expect(`${c[0]}|${(c[2] as { level: string }).level}`).toBe(`${copyText("accountsLocal.new.noWindowCopied")}|info`);
    expect(String(c[1])).toContain(fakeLoginCmd("b"));
  });

  it("★ 真失败（不是那句既定设计）⇒ 登录那一行照样复制，但提示是 error、标题不同", async () => {
    wire("fail");
    const toast = vi.mocked(showActionFailureToast);
    toast.mockClear();
    const el = await mountLocal([acct({ name: "z" })]);
    await submit(el, "b");
    expect(writeText).toHaveBeenCalledWith(fakeLoginCmd("b"));
    const c = toast.mock.calls.at(-1)!;
    expect(`${c[0]}|${(c[2] as { level: string }).level}`).toBe(`${copyText("accountsLocal.new.failedCopied")}|error`);
  });

  it("★ API 号 ⇒ key 交给本机后端那一发，本机不开终端、屏上没有明文", async () => {
    const calls = wire("ok");
    const el = await mountLocal([acct({ name: "z" })]);
    await submit(el, "b", "sk-ant-FOR-B");
    expect(accountOps(calls).filter(([, , a]) => a.dryRun !== true)).toEqual([
      ["accounts-add", LOCAL_ORIGIN, { name: "b", kind: "api-key", key: "sk-ant-FOR-B" }],
    ]);
    expect(calls.some(([c]) => c === "launch_remote_terminal")).toBe(false);
    expect(el.textContent).not.toContain("sk-ant-FOR-B");
  });
});

// ST1「切机器 pending」（`设计/70 §6` #5）：切到另一台 = 这一块重读一趟（远端是一次 SSH 往返），
// 这段时间这一块原先是**空的** —— 与「这台没有账号」分不开。
describe("ST1 切机器 pending：账号那一块", () => {
  it("切到 aya、读还在路上：挂一行 aria-busy 的「正在读 aya 的账号」；回来就撤", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host(), host({ label: "gpd" })] });
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const el = await mount();
    expect(el.querySelector("[data-pending=accounts]"), "前提：读完了不该还挂着").toBeNull();
    let release!: (v: AccountsState) => void;
    fetchAccountsMock.mockReturnValue(new Promise((r) => (release = r)));
    setCurrentMachine("gpd");
    await new Promise((r) => setTimeout(r, 0));
    const busy = el.querySelector<HTMLElement>("[data-pending=accounts]");
    expect(busy, "读在路上时这一块是空的").toBeTruthy();
    expect(busy!.getAttribute("aria-busy")).toBe("true");
    expect(busy!.textContent).toContain("gpd");
    release(state({ origin: "gpd", accounts: [acct({ name: "g1" })], defaultName: "g1" }));
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(el.querySelector("[data-pending=accounts]")).toBeNull();
    expect(el.querySelector(".accounts-table")?.textContent).toContain("g1");
  });
});

/** ST1「延后加载」：分节构造期不再发 I/O，由宿主在机器子页第一次可见时调 `loadNow()`。
 *  本文件量的是分节**加载之后**的行为 ⇒ 构造完就当宿主那样叫醒它。 */
function loaded<T extends { loadNow(): void }>(s: T): T {
  s.loadNow();
  return s;
}

// 〔RESYNC · 主会话 09-27 裁「本机点刷新不清账号缓存：缺陷」〕刷新清的是**这一页那台**的缓存，本机远端同一条（与账号 chip 同）。
describe("〔RESYNC〕[刷新] 清这一页那台的账号缓存", () => {
  it("本机页、远端页各点一次 ⇒ 各清各的那台", async () => {
    const inval = vi.spyOn(accountReads, "invalidateAccountsCache").mockImplementation(() => {});
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [{ label: "aya", host: "aya" }] });
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "z" })], defaultName: "z" }));
    const got: unknown[] = [];
    for (const origin of [LOCAL_ORIGIN, "aya"]) {
      setCurrentMachine(origin);
      const el = await mount();
      inval.mockClear();
      el.querySelector<HTMLButtonElement>("button.accounts-refresh")!.click();
      got.push(inval.mock.calls.map((c) => c[0]));
    }
    expect(got).toEqual([[LOCAL_ORIGIN], ["aya"]]);
  });
});
