/**
 * 机器页「账号」栏的判据：表（一号一行 · 默认 · 5h / 7d · 按量）· 切机器只认最后一趟 · 删号确认框（删默认号时「之后新会话默认 X」
 * 是那台答的、带 force）· 各态（没连上过 · 离线画上次的 · 这台不支持多账号 · 没启用）· 打开时核一次（有对不上才出警告条）·
 * 表下没有指路框 · 开不了终端窗口 ⇒「在 tmux 里登录」· 命令名取自别名清单的分组 · key 的纪律（源码扫描）。
 *
 * 那几条读口 / 写口整块换成假的（`vi.mock`）：这里只量「拿到这些事实，画成什么样、交出去什么」。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../../test-support/repo-root";
import type { Account, AccountsState } from "../../../../src/frontend/ui/accounts";

const fetchAccounts = vi.fn();
const readQuota = vi.fn();
const accountsVerify = vi.fn();
const accountsRemove = vi.fn();
const accountsSetDefault = vi.fn();
const readProfiles = vi.fn();
const readMachines = vi.fn();
const emit = vi.fn();
const setModelForAccount = vi.fn();
const accountsInit = vi.fn();
const accountsRepair = vi.fn();
const accountsAdd = vi.fn();
const writeApikeyKey = vi.fn();
const openLoginDrawer = vi.fn();

vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: (...a: unknown[]) => emit(...a), listen: vi.fn() }));
vi.mock("../../../../src/frontend/ui/account-reads", () => ({
  fetchAccounts: (...a: unknown[]) => fetchAccounts(...a),
  launchAgentId: () => "claude",
  accountsAgentProfile: () => ({ displayName: "Claude Code", models: ["sonnet", "opus"] }),
}));
vi.mock("../../../../src/frontend/ui/quota-reads", () => ({ readQuota: (...a: unknown[]) => readQuota(...a) }));
vi.mock("../../../../src/frontend/ui/profiles-reads", () => ({ readProfiles: (...a: unknown[]) => readProfiles(...a) }));
// 各台此刻的成品（本机后端那张表里的每台）：判据照它摆「哪几台在表里」。
vi.mock("../../../../src/frontend/ui/machine-feed", async (orig) => ({
  ...(await orig<object>()),
  readMachines: () => readMachines(),
  readMachineNames: async () => Object.keys((await readMachines()) as Record<string, unknown>),
}));
vi.mock("../../../../src/frontend/ui/events", () => ({ subscribeTopics: () => Promise.resolve(() => {}) }));
const getModelForAccount = vi.fn();
// 每号模型住那台的偏好（`prefs-read` / `prefs-set`）：桩成「读那一号的那一格 · 写那一号的那一格」两件，判据照旧按号说。
vi.mock("../../../../src/frontend/ui/machine-prefs", () => ({
  readPrefs: async (origin: string) => {
    // 判据按号给（`getModelForAccount` 的答法与号名无关）⇒ 每个号读到的都是那一个值。
    const v = (await getModelForAccount(origin)) as string | undefined;
    const models = new Proxy({}, { get: () => v });
    return { state: "ok", prefs: { launchers: {}, resumeWhere: "managed", contextLimits: {}, models, permissionModes: {} } };
  },
  setPrefs: async (origin: string, patch: { models?: Record<string, string | null> }) => {
    for (const [name, m] of Object.entries(patch.models ?? {})) await setModelForAccount(origin, name, m);
    return { state: "ok", prefs: { launchers: {}, resumeWhere: "managed", contextLimits: {}, models: {}, permissionModes: {} } };
  },
}));
vi.mock("../../../../src/frontend/ui/account-ops", async (orig) => ({
  ...(await orig<object>()),
  accountsVerify: (...a: unknown[]) => accountsVerify(...a),
  accountsRemove: (...a: unknown[]) => accountsRemove(...a),
  accountsSetDefault: (...a: unknown[]) => accountsSetDefault(...a),
  accountsInit: (...a: unknown[]) => accountsInit(...a),
  accountsRepair: (...a: unknown[]) => accountsRepair(...a),
  accountsAdd: (...a: unknown[]) => accountsAdd(...a),
}));
vi.mock("../../../../src/frontend/ui/login-drawer", () => ({ openLoginDrawer: (...a: unknown[]) => openLoginDrawer(...a) }));
vi.mock("../../../../src/frontend/ui/apikey-reads", () => ({ writeApikeyKey: (...a: unknown[]) => writeApikeyKey(...a) }));

import { AccountsSection } from "../../../../src/frontend/ui/settings/accounts-section";
import { SETTINGS_GO_EVENT } from "../../../../src/frontend/ui/settings/events";
import { __resetMachineContextForTests, setCurrentMachine } from "../../../../src/frontend/ui/machine-context";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import type { ConfirmSpec } from "../../../../src/frontend/ui/kit/dialog";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { toast } from "../../../../src/frontend/ui/kit/toast";
import { nonEmpty } from "../../../test-support/non-empty";
import { ChanError } from "../../../../src/comms/inward/chan";

/** 那台后端拒了一发（对端 `refused` 那一层 ＋ 码 ＋ 那一句）。 */
function refusal(code: string, message: string): ChanError {
  return new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code, said: message })) }, message);
}

function acct(p: Partial<Account>): Account {
  return { name: "z", email: "z@x", configDir: "/h/.cc-monitor/accounts/z", configDirShown: "~/.cc-monitor/accounts/z", isDefault: false, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true, selectable: true, badge: { text: copyText("accounts.badge.signedIn"), warn: false }, ...p };
}
const META = { enabled: true, acctsDir: "/a", manifestPath: "/a/accounts.json", updatedAt: null, sharedStore: null, count: 3, error: null, unsupported: null, nextDefault: "personal", effectiveDefault: "work" };
function state(p: Partial<AccountsState> = {}): AccountsState {
  return {
    origin: "devbox",
    available: true,
    oldBackend: false,
    error: null,
    notice: null,
    meta: META,
    accounts: [
      acct({ name: "work", label: "后端·work", email: "work@example.com", isDefault: true, kindLine: "line-work", slots: [cell("后端·5h ✕ ↻13:00", "fail"), cell("后端·7d 78%")] }),
      acct({ name: "personal", email: "me@example.com", kindLine: "line-personal", slots: [cell("后端·5h 63% ↻13:00"), cell("后端·7d 41%")] }),
      acct({ name: "api", authKind: "api-key", email: "", keyMasked: "••••••••a1b2", endpointHost: "HOST-BY-BACKEND", kindLine: "line-api", slots: [cell("后端·按量")] }),
    ],
    ...p,
  };
}
/** 那一行里写好的一格额度（字与语气都是那台后端写的，界面照摆）。 */
function cell(text: string, tone: "plain" | "warn" | "fail" = "plain"): NonNullable<Account["slots"]>[number] {
  return { slot: "5h", text, tone };
}

let confirmed: ConfirmSpec[] = [];
let answer = true;
const confirm = (spec: ConfirmSpec): Promise<boolean> => {
  confirmed.push(spec);
  return Promise.resolve(answer);
};
const settle = async (): Promise<void> => {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
};
async function mount(): Promise<HTMLElement> {
  const s = new AccountsSection({ confirm });
  document.body.replaceChildren(s.element);
  s.loadNow();
  await settle();
  return s.element;
}
const rowOf = (el: HTMLElement, name: string): HTMLElement => el.querySelector<HTMLElement>(`[data-account="${name}"]`)!;
const buttonNamed = (root: HTMLElement, text: string): HTMLButtonElement =>
  [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === text || b.getAttribute("aria-label") === text)!;

beforeEach(() => {
  __resetMachineContextForTests();
  setCurrentMachine("devbox");
  confirmed = [];
  answer = true;
  for (const m of [fetchAccounts, readQuota, accountsVerify, accountsRemove, accountsSetDefault, readProfiles, readMachines, emit, setModelForAccount, accountsInit, accountsRepair, accountsAdd, writeApikeyKey, openLoginDrawer]) m.mockReset();
  const change = (p: object = {}) => ({ applied: true, steps: ["一步"], notes: [], backup: null, account: null, aliasNames: ["teamcc", "teamcct"], keyMasked: null, keyProblem: null, aliases: [], ...p });
  accountsInit.mockResolvedValue(change());
  accountsRepair.mockResolvedValue(change());
  accountsAdd.mockResolvedValue(change());
  readMachines.mockResolvedValue({ "<local>": { state: "up" }, devbox: { state: "up" }, "gpu-01": { state: "up" } });
  fetchAccounts.mockResolvedValue(state());
  accountsVerify.mockResolvedValue({ pass: true, fails: 0, checks: [] });
  readProfiles.mockResolvedValue({
    profiles: [
      { name: "apicc", accountShape: { account: "api", form: "plain" } },
      { name: "apicct", accountShape: { account: "api", form: "tmux" } },
      { name: "apicch", accountShape: { account: "api", form: "hosted" } },
      { name: "x", accountShape: null },
    ],
  });
  accountsRemove.mockResolvedValue({ applied: true, steps: [], notes: [] });
  accountsSetDefault.mockResolvedValue({ applied: true, steps: [], notes: [] });
  setModelForAccount.mockResolvedValue(undefined);
  getModelForAccount.mockReset();
  getModelForAccount.mockResolvedValue(undefined);
});

describe("账号表", () => {
  it("表头说是哪台、新会话默认谁；一号一行，号名照那一行的 label，默认号带「默认」，用量两列照那一行写好的格", async () => {
    const el = await mount();
    expect(el.querySelector(".acct-head-title")!.textContent).toBe(copyText("acctPage.head.title", { machine: "devbox" }));
    expect(el.querySelector(".acct-head-sub")!.textContent).toBe(copyText("acctPage.head.default", { name: "work" }));
    expect([...el.querySelectorAll("[data-account]")].map((r) => (r as HTMLElement).dataset.account)).toEqual(["work", "personal", "api"]);
    const nameLine = (n: string) => rowOf(el, n).querySelector(".acct-row-name")!.textContent;
    expect(nameLine("work")).toBe(`后端·work${copyText("resumeSelect.option.defaultBare")}`);
    expect(nameLine("personal")).toBe("personal");
    const slots = (name: string) => [...rowOf(el, name).querySelectorAll<HTMLElement>(".acct-row-slot")];
    expect(slots("work").map((c) => c.textContent)).toEqual(["后端·5h ✕ ↻13:00", "后端·7d 78%"]);
    expect(slots("personal").map((c) => c.textContent)).toEqual(["后端·5h 63% ↻13:00", "后端·7d 41%"]);
    expect(slots("api").map((c) => c.textContent)).toEqual(["后端·按量", ""]);
    expect(rowOf(el, "work").querySelector(".acct-row-kind")!.textContent).toBe("line-work");
    expect(readQuota, "额度格在那一行里，不另问额度账").not.toHaveBeenCalled();
  });

  it("★ 每格的语气照那一行写好的 tone：fail ⇒ 红 · warn ⇒ 黄 · plain ⇒ 不标", async () => {
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "work", slots: [cell("T5", "warn"), cell("T7", "fail")] }), acct({ name: "team", slots: [cell("P5")] })] }));
    const el = await mount();
    const [c5, c7] = rowOf(el, "work").querySelectorAll<HTMLElement>(".acct-row-slot");
    expect([c5.textContent, c5.dataset.shade]).toEqual(["T5", "warn"]);
    expect([c7.textContent, c7.dataset.shade]).toEqual(["T7", "refused"]);
    expect(rowOf(el, "team").querySelector<HTMLElement>(".acct-row-slot")!.dataset.shade).toBeUndefined();
  });

  it("★ 选不了的那一句（那台写好的 unselectable）挂在号名后面", async () => {
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "api", authKind: "api-key", selectable: false, unselectable: "后端·没配 key" })] }));
    const el = await mount();
    expect(rowOf(el, "api").querySelector(".acct-row-name")!.textContent).toBe("api后端·没配 key");
  });

  it("非默认号那一行有［设为默认］，点了问那台 accounts-set-default 并说「新会话默认 X」", async () => {
    const el = await mount();
    expect(buttonNamed(rowOf(el, "work"), copyText("acctPage.row.setDefault"))).toBeUndefined();
    buttonNamed(rowOf(el, "personal"), copyText("acctPage.row.setDefault")).click();
    await settle();
    expect(accountsSetDefault).toHaveBeenCalledWith("devbox", "personal");
  });

  it("点行展开详情：命令名取自别名清单里分给这个号的那几条（不自己拼）", async () => {
    const el = await mount();
    (rowOf(el, "api").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    const detail = rowOf(el, "api").querySelector(".acct-detail")!;
    expect(detail.querySelector(".acct-detail-mono")!.textContent).toBe("apicc · apicct · apicch");
    expect(detail.textContent).toContain(copyText("acctPage.detail.modelScope", { machine: "devbox", name: "api" }));
  });

  it("API key 号：第二行「API key · 地址」，详情「掩码 地址［更换…］」（掩码是那台遮好的）；没存上 ⇒ 那一句", async () => {
    const el = await mount();
    (rowOf(el, "api").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    const key = rowOf(el, "api").querySelector(".acct-key")!;
    expect(key.querySelector(".acct-detail-mono")!.textContent).toBe("••••••••a1b2");
    // 主机那一格照那台写好的 `endpointHost` 原样摆，界面不解析地址。
    expect(key.querySelector(".acct-key-host")!.textContent).toBe("HOST-BY-BACKEND");
    expect(buttonNamed(rowOf(el, "api"), copyText("acctPage.detail.keyChange"))).toBeDefined();
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "work", isDefault: true }), acct({ name: "api", authKind: "api-key", email: "", keyMasked: null, endpointHost: null })] }));
    const el2 = await mount();
    (rowOf(el2, "api").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    expect(rowOf(el2, "api").querySelector(".acct-key .acct-detail-mono")!.textContent).toBe(copyText("acctPage.row.keyNotSaved"));
  });

  it("账号目录照抄那台写好的显示形（`configDirShown`），界面不自己缩", async () => {
    const el = await mount();
    (rowOf(el, "work").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    const monos = () => [...rowOf(el, "work").querySelectorAll(".acct-detail-mono")].map((e) => e.textContent);
    expect(monos()).toContain("~/.cc-monitor/accounts/z");
    // 那台家目录是 /home/x、目录 /home/x/a ⇒ 那台写 `~/a`；界面照抄（换一个界面推不出的字也照抄）。
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "work", configDir: "/home/x/a", configDirShown: "~/a" })] }));
    const el2 = await mount();
    (rowOf(el2, "work").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    expect([...rowOf(el2, "work").querySelectorAll(".acct-detail-mono")].map((e) => e.textContent)).toContain("~/a");
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "work", configDir: "/h/.cc-monitor/accounts/z", configDirShown: "SHOWN-BY-BACKEND" })] }));
    const el3 = await mount();
    (rowOf(el3, "work").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    expect([...rowOf(el3, "work").querySelectorAll(".acct-detail-mono")].map((e) => e.textContent)).toContain("SHOWN-BY-BACKEND");
  });

  it("默认模型是下拉：第一项「{家} 默认」＋ 那一家认得的模型；盘上存的不在里面 ⇒ 多列它一项；选中即存（键是这台 ＋ 这个号）", async () => {
    getModelForAccount.mockResolvedValue("claude-opus-4-1");
    const el = await mount();
    (rowOf(el, "api").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    const sel = rowOf(el, "api").querySelector<HTMLSelectElement>("select.acct-detail-model")!;
    expect([...sel.options].map((o) => [o.value, o.textContent])).toEqual([
      ["", copyText("acctPage.detail.modelDefault", { agent: "Claude Code" })],
      ["sonnet", "sonnet"],
      ["opus", "opus"],
      ["claude-opus-4-1", "claude-opus-4-1"],
    ]);
    expect(sel.value).toBe("claude-opus-4-1");
    sel.value = "sonnet";
    sel.dispatchEvent(new Event("change"));
    await settle();
    expect(setModelForAccount).toHaveBeenLastCalledWith("devbox", "api", "sonnet");
    sel.value = "";
    sel.dispatchEvent(new Event("change"));
    await settle();
    expect(setModelForAccount, "选回「默认」⇒ 清掉那一格").toHaveBeenLastCalledWith("devbox", "api", null);
  });

  it("表下一行「共用 MCP：别名与配置文件」：点了冒泡一个目的地（同一台 · 别名与配置文件栏）", async () => {
    const el = await mount();
    const line = el.querySelector<HTMLElement>(".acct-mcp-line")!;
    expect(line.textContent).toBe(`${copyText("acctPage.mcp.where")}${copyText("machinePage.tab.config")}`);
    const got: unknown[] = [];
    document.body.addEventListener(SETTINGS_GO_EVENT, (ev) => got.push((ev as CustomEvent).detail));
    line.querySelector<HTMLButtonElement>("button")!.click();
    expect(got).toEqual([{ machine: "devbox", tab: "config", anchor: "shared-mcp" }]);
  });

  it("切机器：标题当场换；上一台晚到的回答作废", async () => {
    let release!: (v: AccountsState) => void;
    fetchAccounts.mockImplementationOnce(() => new Promise((r) => (release = r)));
    const s = new AccountsSection({ confirm });
    document.body.replaceChildren(s.element);
    s.loadNow();
    await settle();
    fetchAccounts.mockResolvedValue(state({ origin: "gpu-01", meta: { ...META, effectiveDefault: "team" }, accounts: [acct({ name: "team", isDefault: true })] }));
    setCurrentMachine("gpu-01");
    expect(s.element.querySelector(".acct-head-title")!.textContent).toBe(copyText("acctPage.head.title", { machine: "gpu-01" }));
    await settle();
    release(state());
    await settle();
    expect([...s.element.querySelectorAll("[data-account]")].map((r) => (r as HTMLElement).dataset.account)).toEqual(["team"]);
  });
});

describe("删号", () => {
  it("删默认号：确认框危险、列出删 / 留，多一句「之后新会话默认 X」（那台答的），确认后带 force，默认模型那一格一起删", async () => {
    const el = await mount();
    (rowOf(el, "work").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    buttonNamed(rowOf(el, "work"), copyText("acctPage.menu.remove", { name: "work" })).click();
    await settle();
    expect(confirmed).toHaveLength(1);
    const spec = confirmed[0];
    expect(spec.title).toBe(copyText("acctPage.remove.title", { name: "work", machine: "devbox" }));
    expect(spec.action).toBe(copyText("acctPage.remove.action", { name: "work" }));
    expect(spec.danger).toBe(true);
    expect(spec.body).toBe(copyText("acctPage.remove.next", { name: "personal" }));
    expect(spec.rows?.map((r) => r.label)).toEqual([copyText("rsFilewinWriteops.label.delete"), copyText("kit.interrupts.keep")]);
    expect(spec.rows?.[1].items).toEqual([copyText("acctPage.remove.sessions")]);
    expect(accountsRemove).toHaveBeenCalledWith("devbox", { name: "work", force: true });
    expect(setModelForAccount).toHaveBeenCalledWith("devbox", "work", null);
  });

  it("删非默认号：没有那一句、不带 force；点了取消 ⇒ 什么都不交", async () => {
    answer = false;
    const el = await mount();
    (rowOf(el, "api").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    buttonNamed(rowOf(el, "api"), copyText("acctPage.menu.remove", { name: "api" })).click();
    await settle();
    expect(confirmed[0].body).toBeUndefined();
    expect(confirmed[0].rows?.[0].items).toContain(copyText("acctPage.remove.commands", { names: "apicc apicct apicch" }));
    expect(accountsRemove).not.toHaveBeenCalled();
  });
});

describe("各态", () => {
  it("机器表里没有这台 ⇒「X 未连接过」，新建账号禁用，不去问那台", async () => {
    setCurrentMachine("build-02");
    const el = await mount();
    expect(el.textContent).toContain(copyText("acctPage.never.title", { machine: "build-02" }));
    expect(el.textContent).toContain(copyText("acctPage.never.hint"));
    expect(buttonNamed(el, copyText("acctPage.head.new")).getAttribute("aria-disabled")).toBe("true");
    expect(fetchAccounts).not.toHaveBeenCalled();
  });

  it("★ 机器表每一趟都问（界面不另存一份）：开着的时候那台从表里删了 ⇒ 刷新一下就是「未连接过」", async () => {
    const el = await mount();
    expect(rowOf(el, "work")).toBeTruthy();
    readMachines.mockResolvedValue({ "<local>": { state: "up" }, "gpu-01": { state: "up" } });
    fetchAccounts.mockClear();
    buttonNamed(el, copyText("accountChip.menu.refresh")).click();
    await settle();
    expect(el.textContent).toContain(copyText("acctPage.never.title", { machine: "devbox" }));
    expect(fetchAccounts).not.toHaveBeenCalled();
  });

  it("这一次没问到、有上次的 ⇒ 警告条照后端写好的上次读数那一句「离线 · 采样于 … · 只读」＋［重试］，画上次的表、只读", async () => {
    const seen = { atMs: 1, said: "采样于 夹具那一刻" };
    fetchAccounts.mockResolvedValue(state({ available: false, error: "超时", last: { meta: META, accounts: [acct({ name: "work", isDefault: true })], seen } }));
    const el = await mount();
    expect(el.textContent).toContain(copyText("acctPage.offline.bar", { machine: "devbox", seen: seen.said }));
    expect(buttonNamed(el, copyText("front.act.retry"))).toBeDefined();
    expect(el.querySelector(".acct-table")!.getAttribute("data-readonly")).toBe("true");
    expect(rowOf(el, "work")).not.toBeNull();
  });

  it("这台做不了多账号（后端说的）⇒「Windows · 不支持多账号」卡，不摆启用表单", async () => {
    fetchAccounts.mockResolvedValue(state({ meta: { ...META, enabled: false, unsupported: "这台不支持" }, accounts: [] }));
    const el = await mount();
    expect(el.textContent).toContain(copyText("acctPage.unsupported.title"));
    expect(el.textContent).toContain(copyText("acctPage.unsupported.hint"));
    expect(el.querySelector(".acct-enable")).toBeNull();
  });

  it("没启用多账号 ⇒ 启用卡（名字框 ＋［启用］）", async () => {
    fetchAccounts.mockResolvedValue(state({ meta: { ...META, enabled: false }, accounts: [] }));
    const el = await mount();
    expect(el.querySelector(".acct-enable")!.textContent).toContain(copyText("accountChip.label.disabled"));
    expect(buttonNamed(el, copyText("acctPage.notEnabled.enable"))).toBeDefined();
  });

  it("打开时核一次：回包里有写好的那一句才出警告条（原样摆那一句 ＋［修复…］）；没有那一句 ⇒ 不占地方", async () => {
    let el = await mount();
    expect(el.textContent).not.toContain(copyText("acctPage.verify.fix"));
    // 三条对不上（两条带号名）：那一句由后端写好；前端不数、不挑、不拼 —— 逐条明细与写好的那一句故意不一样。
    const checks = [
      { level: "fail", account: "personal", text: "登录信息缺失" },
      { level: "fail", account: null, text: "共享库里留着身份" },
      { level: "fail", account: "work", text: "权限不对" },
    ];
    accountsVerify.mockResolvedValue({ pass: false, fails: 3, checks, headline: "后端写好的那一句-xyz" });
    el = await mount();
    expect(buttonNamed(el, copyText("acctPage.verify.fix"))).toBeDefined();
    expect(el.textContent).toContain("后端写好的那一句-xyz");
    expect(el.textContent).not.toContain("personal：登录信息缺失");
    expect(el.textContent).not.toContain(copyText("kit.dialog.more", { n: 2 }));
    accountsVerify.mockResolvedValue({ pass: false, fails: 3, checks });
    el = await mount();
    expect(el.textContent, "没有那一句 ⇒ 前端不自己拼一条").not.toContain(copyText("acctPage.verify.fix"));
  });
});

describe("登录与指路", () => {
  it("★ 需登录那一行［登录…］·「⋯ 重新登录…」都开登录抽屉（目标照这一行拼：那一家 ＋ 号名）；「⋯」里没有在终端里登录（只在抽屉里）", async () => {
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "work", isDefault: true }), acct({ name: "team", loggedIn: false, authReady: false, selectable: false })] }));
    const el = await mount();
    const go = rowOf(el, "team").querySelector<HTMLButtonElement>('[data-action="acct-login"]')!;
    expect(go.textContent).toBe(copyText("newSession.account.login"));
    expect(rowOf(el, "work").querySelector('[data-action="acct-login"]'), "已登录的那一行没有").toBeNull();
    go.click();
    expect(openLoginDrawer).toHaveBeenLastCalledWith(expect.objectContaining({ origin: "devbox", target: { kind: "account", agent: "claude", account: "team" } }));
    buttonNamed(rowOf(el, "work"), copyText("acctPage.row.more", { name: "work" })).click();
    const items = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")];
    expect(items.some((m) => m.textContent?.includes(copyText("login.act.terminal"))), "终端那条路只在抽屉里").toBe(false);
    items.find((m) => m.textContent?.includes(copyText("acctPage.menu.relogin")))!.click();
    expect(openLoginDrawer).toHaveBeenLastCalledWith(expect.objectContaining({ origin: "devbox", target: { kind: "account", agent: "claude", account: "work" } }));
  });

  it("表下没有指路框（默认轮换与规则在同一台的「轮换」栏里管），只剩「共用 MCP」那一行", async () => {
    const el = await mount();
    expect(el.querySelector(".acct-pointer")).toBeNull();
    expect([...el.querySelectorAll(".acct-pointer-link")].map((b) => b.textContent)).toEqual([copyText("machinePage.tab.config")]);
    expect(emit).not.toHaveBeenCalledWith("open-account-panel", expect.anything());
  });

  it("本机那一页：标题「本机 上的账号」，读的是本机", async () => {
    setCurrentMachine(LOCAL);
    fetchAccounts.mockResolvedValue(state({ origin: LOCAL }));
    const el = await mount();
    expect(el.querySelector(".acct-head-title")!.textContent).toBe(copyText("acctPage.head.title", { machine: copyText("control.machine.unsure") }));
    expect(fetchAccounts.mock.calls[0][0]).toBe(LOCAL);
  });
});

describe("动作", () => {
  it("没启用 ⇒ 填名字点［启用］：先预演、确认框列出那几步，确认后才真做", async () => {
    fetchAccounts.mockResolvedValue(state({ meta: { ...META, enabled: false }, accounts: [] }));
    const el = await mount();
    const input = el.querySelector<HTMLInputElement>(".acct-enable input")!;
    input.value = "main";
    buttonNamed(el, copyText("acctPage.notEnabled.enable")).click();
    await settle();
    expect(accountsInit.mock.calls[0]).toEqual(["devbox", { name: "main", dryRun: true }]);
    expect(confirmed[0].list).toEqual(["一步"]);
    expect(accountsInit.mock.calls[1]).toEqual(["devbox", { name: "main" }]);
    // 确认框点了取消 ⇒ 只预演、不建库。
    answer = false;
    accountsInit.mockClear();
    el.querySelector<HTMLInputElement>(".acct-enable input")!.value = "main";
    buttonNamed(el, copyText("acctPage.notEnabled.enable")).click();
    await settle();
    expect(accountsInit.mock.calls).toEqual([["devbox", { name: "main", dryRun: true }]]);
  });

  it("没启用 ⇒ 名字界面不判：原样交那台预演，名字格上画它拒的那一句", async () => {
    fetchAccounts.mockResolvedValue(state({ meta: { ...META, enabled: false }, accounts: [] }));
    const said = copyText("beAcctPlan.name.shape", { name: '" B!"', max: 32 });
    accountsInit.mockRejectedValue(refusal("refused", said));
    const el = await mount();
    el.querySelector<HTMLInputElement>(".acct-enable input")!.value = " B!";
    buttonNamed(el, copyText("acctPage.notEnabled.enable")).click();
    await settle();
    expect(accountsInit.mock.calls).toEqual([["devbox", { name: " B!", dryRun: true }]]);
    expect(el.querySelector(".acct-enable [data-error]")?.textContent).toContain(said);
    expect(confirmed).toEqual([]);
  });

  it("警告条［修复…］：预演出几步 ⇒ 确认框列出来，确认后才修；修过之后条上 ⋯ 里才有「恢复到修复之前」", async () => {
    accountsVerify.mockResolvedValue({ pass: false, fails: 1, checks:[{ level: "fail", account: "personal", text: "登录信息缺失" }], headline: "personal：登录信息缺失" });
    const el = await mount();
    expect(buttonNamed(el, copyText("history.row.more"))).toBeUndefined();
    buttonNamed(el, copyText("acctPage.verify.fix")).click();
    await settle();
    expect(confirmed[0].title).toBe(copyText("acctPage.verify.repairTitle", { machine: "devbox" }));
    expect(accountsRepair.mock.calls.map((c) => c[1])).toEqual([{ dryRun: true }, {}]);
    expect(buttonNamed(el, copyText("history.row.more"))).toBeDefined();
  });

  it("★ 新建订阅号：交给那台 accounts-add，建好就开登录抽屉（不开终端）", async () => {
    const el = await mount();
    buttonNamed(el, copyText("acctPage.head.new")).click();
    const name = el.querySelector<HTMLInputElement>(".acct-new input")!;
    name.value = "team";
    name.dispatchEvent(new Event("input"));
    await settle();
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "work", isDefault: true }), acct({ name: "team", loggedIn: false, email: "" })] }));
    buttonNamed(el, copyText("acctNew.form.createLogin")).click();
    await settle();
    expect(accountsAdd).toHaveBeenLastCalledWith("devbox", { name: "team", kind: "subscription" });
    expect(openLoginDrawer).toHaveBeenLastCalledWith(expect.objectContaining({ origin: "devbox", target: { kind: "account", agent: "claude", account: "team" } }));
    expect(el.querySelector(".acct-new")).toBeNull();
  });

  it("★ 建好那一句的下一行照后端回的提示（如「已开的终端要重读别名」），界面不另拼", async () => {
    accountsAdd.mockImplementation((_o: string, a: { dryRun?: boolean }) =>
      Promise.resolve({ applied: true, steps: [], notes: a.dryRun ? [] : ["已开的终端要重读别名-xyz"], backup: null, account: null, aliasNames: ["teamcc", "teamcct"], keyMasked: null, keyProblem: null, aliases: [] }),
    );
    const el = await mount();
    buttonNamed(el, copyText("acctPage.head.new")).click();
    const name = el.querySelector<HTMLInputElement>(".acct-new input")!;
    name.value = "team";
    name.dispatchEvent(new Event("input"));
    await settle();
    buttonNamed(el, copyText("acctNew.form.createLogin")).click();
    await settle();
    expect(vi.mocked(toast).mock.calls.some((c) => String(c[1]).includes("已开的终端要重读别名-xyz")), "后端回的提示没上屏").toBe(true);
  });

  it("API key 号详情［更换…］：地址 ＋ key 交给写 key 那一条（带这个号的目录），写完重读那台、照新那一行摆掩码与主机、框清空", async () => {
    writeApikeyKey.mockResolvedValue(undefined);
    const el = await mount();
    fetchAccounts.mockResolvedValue(state({ accounts: [acct({ name: "work", isDefault: true }), acct({ name: "api", authKind: "api-key", email: "", keyMasked: "••••••••c3d4", endpointHost: "GW-BY-BACKEND" })] }));
    (rowOf(el, "api").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    const row = rowOf(el, "api");
    buttonNamed(row, copyText("acctPage.detail.keyChange")).click();
    const key = row.querySelector<HTMLInputElement>(".acct-key-form input[type=password]")!;
    key.value = "sk-ant-NEW";
    buttonNamed(row, copyText("rsFilewinShell.editor.save")).click();
    await settle();
    expect(writeApikeyKey).toHaveBeenCalledWith("devbox", "/h/.cc-monitor/accounts/z", "sk-ant-NEW", undefined);
    expect(rowOf(el, "api").querySelector(".acct-key")!.textContent).toContain("••••••••c3d4GW-BY-BACKEND");
    expect(rowOf(el, "api").querySelector<HTMLElement>(".acct-key-form")!.hidden, "写完框收起").toBe(true);
    expect(key.value).toBe("");
  });

  it("API key 号详情［更换…］：地址界面不判，原样交写 key 那一条；它以 bad_args 拒 ⇒ 那一句画在地址格", async () => {
    const said = copyText("beUpstreamTable.whyPlaintextOffLoopback.say");
    writeApikeyKey.mockRejectedValue(refusal("bad_args", said));
    const el = await mount();
    (rowOf(el, "api").querySelector(".acct-row") as HTMLElement).click();
    await settle();
    const row = rowOf(el, "api");
    buttonNamed(row, copyText("acctPage.detail.keyChange")).click();
    const inputs = row.querySelectorAll<HTMLInputElement>(".acct-key-form input");
    const base = [...inputs].find((i) => i.type !== "password")!;
    base.value = " http://gw.example.com ";
    row.querySelector<HTMLInputElement>(".acct-key-form input[type=password]")!.value = "sk-ant-NEW";
    buttonNamed(row, copyText("rsFilewinShell.editor.save")).click();
    await settle();
    expect(writeApikeyKey).toHaveBeenCalledWith("devbox", "/h/.cc-monitor/accounts/z", "sk-ant-NEW", " http://gw.example.com ");
    expect(base.closest("[data-error]")?.textContent).toContain(said);
  });
});

describe("key 的纪律（源码扫描：账号栏 ＋ 新建表单）", () => {
  const code = ["src/frontend/ui/settings/accounts-section.ts", "src/frontend/ui/settings/account-new-form.ts"].map((f) => readFileSync(resolve(REPO_ROOT, f), "utf8")).join("\n");

  it("KS6：输入框的 `.value` 只许被赋成空串（从不预填 key）", () => {
    const assigns = [...code.matchAll(/(?:input|In)\.value\s*=\s*([^;]+);/g)].map((m) => m[1].trim());
    nonEmpty(assigns);
    for (const rhs of assigns) expect(rhs, `输入框被赋了不是空串的值：${rhs}`).toBe('""');
  });

  it("KS7：key 只流向写 key 那一条（恰好一处），不进配置写口；前端不认后端那份文件的字段名", () => {
    expect([...code.matchAll(/\bwriteApikeyKey\(/g)].length).toBe(1);
    expect(/\bpatchConfig\b|patch_config/.test(code)).toBe(false);
    expect(code.includes("api_key")).toBe(false);
  });

  it("端点的主机只由后端写（`endpointHost`）：账号栏不解析地址", () => {
    const section = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/settings/accounts-section.ts"), "utf8");
    expect(/\bhostOf\b/.test(section), "账号栏还在自己算主机").toBe(false);
    expect(/new URL\(/.test(section), "账号栏还在解析地址").toBe(false);
    expect(/\.baseUrl\b/.test(section), "账号栏还在读原地址").toBe(false);
  });

  it("KH2C1：前端不从 configDir 推账号 id（那条规则只在 Rust）", () => {
    for (const needle of ['split("/")', "split('/')", "basename(", 'lastIndexOf("/")']) expect(code.includes(needle), needle).toBe(false);
    expect(/writeApikeyKey\(origin,\s*dir,\s*k\b/.test(code), "写 key 那一条没把这个号的 configDir 一起交出去").toBe(true);
  });
});
