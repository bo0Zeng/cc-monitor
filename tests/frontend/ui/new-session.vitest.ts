/**
 * 起新会话框（`new-session.ts`）：那台说什么就画什么 —— 预填 · 几行出不出 · 点［新建］交的那一份 · 某一格不行落在那一格下。
 * 替身：通道（几问，等报到那一问 `session-arrive` 也在里面；机器清单是本机后端的 `machine-read`）· 账号 · 额度 · 开终端。夹具只造结构。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const sent: { op: string; body: Record<string, unknown> }[] = [];
const replies = new Map<string, unknown>();
let refusal: { op: string; code: string; said: string; data: unknown; detail?: string } | null = null;
/** 接下来几次 `session-open` 没等到回话（期限到）。 */
let hang = 0;
/** 期限到之后那一次再核：`true` ⇒ 一直不回（停在「正在核实」那一刻）。 */
let stall = false;
let boom: Error | null = null;

vi.mock("../../../src/comms/inward/chan", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../../src/comms/inward/chan")>();
  return {
    ...real,
    chan: {
      call: async (_origin: string, op: string, body: Uint8Array) => {
        sent.push({ op, body: JSON.parse(new TextDecoder().decode(body)) });
        if (hang > 0 && op === "session-open") {
          hang--;
          throw new real.ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "壳·无应答", "码：Overrun");
        }
        if (stall && op === "session-open") await new Promise(() => {});
        if (refusal && refusal.op === op) {
          const r = refusal;
          refusal = null;
          throw new real.ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code: r.code, said: r.said, data: r.data })) }, r.said, r.detail ?? "");
        }
        if (boom && op === "session-open") throw boom;
        return new TextEncoder().encode(JSON.stringify(replies.get(op)));
      },
    },
  };
});
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    backend_machines: vi.fn(async () => ["<local>", "devbox"]),
    backend_status: vi.fn(async () => ({ channel: true })),
    backend_start: vi.fn(async () => ""),
  },
}));
vi.mock("../../../src/frontend/ui/account-reads", async () => {
  const { copyText } = await import("../../../src/frontend/ui/copy-table");
  return {
    fetchAccounts: vi.fn(async () => ({
      available: true,
      meta: { effectiveDefault: "work" },
      accounts: [
        { name: "personal", email: "", configDir: "/h/.cc/personal", isDefault: false, mode: "isolated", exists: true, loggedIn: false, authKind: "subscription", authReady: false, selectable: false, unselectable: copyText("acct.tag.login"), login: true, newSession: true },
        { name: "work", email: "", configDir: "/h/.cc/work", isDefault: true, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true, selectable: true, newSession: true },
        // 那台说不进选号表的号（有目录、隔离也不列）：列不列只照 `newSession`，界面不另判。
        { name: "elsewhere", email: "", configDir: "/h/.cc/elsewhere", isDefault: false, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true, selectable: true, newSession: false },
      ],
    })),
  };
});
vi.mock("../../../src/frontend/ui/acct-center", () => ({ refreshQuota: vi.fn(async () => {}) }));
vi.mock("../../../src/frontend/ui/behavior", () => ({ getBehavior: vi.fn(async () => ({ resumeCommand: "" })) }));
vi.mock("../../../src/frontend/ui/machine-prefs", () => ({ readPrefs: vi.fn(async () => ({ state: "absent", prefs: { launchers: {}, resumeWhere: "managed", models: {}, permissionModes: {}, contextLimits: {} } })) }));
const term = vi.hoisted(() => ({ openTerminal: vi.fn(async () => {}), openLocalTerminal: vi.fn(async () => {}) }));
vi.mock("../../../src/frontend/ui/terminal-open", async (importOriginal) => ({ ...(await importOriginal<object>()), ...term }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => {}), listen: vi.fn(async () => () => {}) }));
const login = vi.hoisted(() => ({ openLoginDrawer: vi.fn() }));
vi.mock("../../../src/frontend/ui/login-drawer", () => login);
const settings = vi.hoisted(() => ({ openSettingsWindow: vi.fn(async () => {}) }));
vi.mock("../../../src/frontend/ui/open-settings", async (importOriginal) => ({ ...(await importOriginal<object>()), ...settings }));

import { openNewSession } from "../../../src/frontend/ui/new-session";
import { setLaunchSlots } from "../../../src/frontend/ui/session-open";
import { commands } from "../../../src/frontend/ui/ipc/commands";
import { fetchAccounts } from "../../../src/frontend/ui/account-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};
const dialog = (): HTMLElement => document.querySelector<HTMLElement>('[role="dialog"]')!;
const sel = (label: string): HTMLButtonElement => dialog().querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
/** 下拉点开后面板里那几项（字 ＋ 灰字）；点开再关上。 */
const optionsOf = (b: HTMLButtonElement): string[] => {
  b.click();
  const items = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].map((i) => {
    const c = i.cloneNode(true) as HTMLElement;
    c.querySelector(".acct-avatar")?.remove();
    return `${c.querySelector(".acct-avatar") ? "" : i.querySelector(".acct-avatar") ? "▣" : ""}${c.textContent ?? ""}`;
  });
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  b.click();
  return items;
};
/** 下拉里点选一项（按值）。 */
const choose = (b: HTMLButtonElement, label: string): void => {
  b.click();
  [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((i) => i.textContent?.includes(label))!.click();
};
const input = (label: string): HTMLInputElement => dialog().querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;
const rowOf = (el: HTMLElement): HTMLElement => el.closest<HTMLElement>("[class*=nsRow]")!;
const createBtn = (): HTMLButtonElement => [...dialog().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("mobTerms.new.detached"))!;
const newRequests = (): Record<string, unknown>[] => sent.filter((s) => s.op === "session-open").map((s) => s.body);
/** 请求里「起什么」那一份。 */
const whatOf = (r: Record<string, unknown>): Record<string, unknown> => r.what as Record<string, unknown>;

const agentOf = (kind: string) => ({ kind, text: kind === "claude" ? "Claude Code" : "Codex", models: [], modelDefault: null, modelNote: null, permissionModes: [] });
const FACTS = { recent: [{ cwd: "/home/u/srv/orders", shown: "~/srv/orders", lastMs: 2, lastText: "00:00" }, { cwd: "/home/u/srv/billing", shown: "~/srv/billing", lastMs: 1, lastText: "00:00" }], hosts: ["tmux", "direct"], run: "tmux", agents: [agentOf("claude")], fork: null };
const OK = {
  landed: "started",
  session: "orders-cc",
  line: "tmux attach -t orders-cc",
  sid: null,
  agent: "claude",
  cwd: "/home/u/srv/orders",
  account: { name: "work", configDir: "/h/.cc/work", model: null },
  note: null,
  prompt: null,
  arrival: { by: { kind: "cwd", cwd: "/home/u/srv/orders" }, since: 5, session: "orders-cc" },
};
/** 一台的状态成品（结构夹具：只填界面读的那几格）。 */
const machineOf = (name: string, state: string, offlineSaid = `${name} 离线`) => ({
  name, state, offline: state !== "up", reason: null, stage: null, version: null, versionRelation: null, os: null,
  word: "", said: "", offlineSaid, fixes: [], seenHostKey: null, expectedHostKey: null, detail: null,
});
/** `machine-read` 的回包：本机与 devbox 都连着。 */
const MACHINES = { machines: { "<local>": machineOf("workstation", "up"), devbox: machineOf("devbox", "up") } };
/** 等报到那几问（`session-arrive` 的入参）。 */
const arriveAsks = (): Record<string, unknown>[] => sent.filter((s) => s.op === "session-arrive").map((s) => s.body);

beforeEach(() => {
  document.body.replaceChildren();
  sent.length = 0;
  replies.clear();
  refusal = null;
  hang = 0;
  stall = false;
  boom = null;
  vi.mocked(commands.backend_status).mockClear();
  vi.mocked(commands.backend_machines).mockClear();
  vi.mocked(commands.backend_start).mockClear();
  replies.set("machine-read", MACHINES);
  replies.set("session-open-facts", FACTS);
  replies.set("session-open-dir", { exists: true, tmuxName: "orders-cc" });
  replies.set("session-open", OK);
  replies.set("session-arrive", { kind: "arrived", sid: "new-sid" });
  setLaunchSlots(null);
  term.openTerminal.mockClear();
});

describe("起新会话框：预填与几行出不出（都照那台说的）", () => {
  it("★ 目录预填那台最近用过的第一个；只有一家能起 ⇒ 没有 agent 那一行；账号默认号在前、需登录的标出来", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(input(copyText("newSession.label.cwd")).value).toBe("/home/u/srv/orders");
    expect(rowOf(sel(copyText("beProfile.slot.agent"))).hidden).toBe(true);
    const acct = sel(copyText("beProfile.slot.account"));
    expect(acct.dataset.value).toBe("work");
    expect(acct.querySelector(".acct-avatar"), "框上带头像").not.toBeNull();
    expect(optionsOf(acct), "每项头像在前（▣）· 名字 · 灰字").toEqual([`▣work${copyText("resumeSelect.option.defaultBare")}`, `▣personal${copyText("acct.tag.login")}`]);
    expect(input(copyText("newSession.label.tmuxName")).placeholder).toBe("orders-cc");
    expect(sent.find((s) => s.op === "session-open-facts")!.body).toEqual({});
  });

  it("那台能起两家 ⇒ 出 agent 那一行；那台没 tmux ⇒ 只剩终端窗口那一张、灰字说为什么", async () => {
    replies.set("session-open-facts", { ...FACTS, hosts: ["direct"], run: "direct", agents: [agentOf("claude"), agentOf("codex")] });
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(rowOf(sel(copyText("beProfile.slot.agent"))).hidden).toBe(false);
    const radios = [...dialog().querySelectorAll<HTMLInputElement>('input[type="radio"]')];
    expect(radios.find((r) => r.value === "tmux")!.closest("label")!.hidden).toBe(true);
    expect(radios.find((r) => r.value === "window")!.checked).toBe(true);
    expect(dialog().textContent).toContain(copyText("newSession.place.noTmux"));
  });

  it("★ 那台能托管 ⇒ 「放在哪」多一张托管终端、缺省勾它，交的请求 `run: managed`、第一句照交；那台不能托管 ⇒ 不出那一张", async () => {
    replies.set("session-open-facts", { ...FACTS, hosts: ["managed", "tmux", "direct"], run: "managed" });
    void openNewSession({ origin: "devbox", prompt: "看一下" });
    await flush();
    const radios = (): HTMLInputElement[] => [...dialog().querySelectorAll<HTMLInputElement>('input[type="radio"]')];
    const managed = radios().find((r) => r.value === "managed")!;
    expect(managed.closest("label")!.hidden).toBe(false);
    expect(managed.checked).toBe(true);
    expect(managed.closest("label")!.textContent).toContain(copyText("newSession.place.managed"));
    expect(rowOf(input(copyText("newSession.label.tmuxName"))).hidden).toBe(true);
    createBtn().click();
    await flush();
    expect(newRequests()[0]).toMatchObject({ run: "managed", what: { prompt: "看一下" } });
    document.body.replaceChildren();
    replies.set("session-open-facts", FACTS);
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(radios().find((r) => r.value === "managed")!.closest("label")!.hidden).toBe(true);
    expect(radios().find((r) => r.value === "tmux")!.checked).toBe(true);
  });

  it("选了需登录的号 ⇒ 那一格下照那台那一句说「需登录」＋［登录…］（那台说给才给），［新建］灰着", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    const acct = sel(copyText("beProfile.slot.account"));
    choose(acct, "personal");
    expect(rowOf(acct).textContent).toContain(copyText("acct.tag.login"));
    expect(rowOf(acct).textContent).toContain(copyText("newSession.account.login"));
    expect(createBtn().getAttribute("aria-disabled")).toBe("true");
  });

  it("★ 点［登录…］⇒ 框不关、开登录抽屉（那台 · 那一家 · 那个号），不开设置窗", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    const acct = sel(copyText("beProfile.slot.account"));
    choose(acct, "personal");
    [...rowOf(acct).querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("newSession.account.login"))!.click();
    await flush();
    expect(login.openLoginDrawer).toHaveBeenCalledWith(expect.objectContaining({ origin: "devbox", target: { kind: "account", agent: "claude", account: "personal" } }));
    expect(settings.openSettingsWindow).not.toHaveBeenCalled();
    expect(document.querySelector('[role="dialog"]'), "框还开着").not.toBeNull();
  });

  it("★ 那台说登不了（`login: false`）⇒ 只写那一句、不给［登录…］", async () => {
    const st = await fetchAccounts("devbox");
    vi.mocked(fetchAccounts).mockResolvedValueOnce({ ...st, accounts: st.accounts.map((a) => (a.name === "personal" ? { ...a, login: false, unselectable: copyText("accounts.row.loginElsewhere") } : a)) });
    void openNewSession({ origin: "devbox" });
    await flush();
    const acct = sel(copyText("beProfile.slot.account"));
    choose(acct, "personal");
    expect(rowOf(acct).textContent).toContain(copyText("accounts.row.loginElsewhere"));
    expect(rowOf(acct).textContent).not.toContain(copyText("newSession.account.login"));
  });
});

describe("单个 · 一组：页签与别名那一格", () => {
  const BOOK = { rev: "r", sets: [], profiles: [{ name: "look", usable: true }, { name: "bad", usable: false }] };

  it("★ 单个页多一格「别名」（那台能用的几条 ＋ 不用别名）；挑了 ⇒ 请求带 alias", async () => {
    replies.set("profiles-read", BOOK);
    const done = openNewSession({ origin: "devbox" });
    await flush();
    const alias = sel(copyText("newSession.label.alias"));
    expect(rowOf(alias).hidden).toBe(false);
    expect(optionsOf(alias)).toEqual([copyText("newSession.alias.none"), "look"]);
    choose(alias, "look");
    createBtn().click();
    await flush();
    await done;
    expect(whatOf(newRequests()[0]).alias).toBe("look");
  });

  /** look 合下来的那几格（那台 `profiles-resolve` 的合并表，结构夹具）。 */
  const RESOLVED = {
    chain: ["look"],
    rows: [
      { key: "account", slot: "account", label: "acct-label", vals: ["lab"], said: "lab", from: "look", fromSaid: "FROM-look", overriddenBy: null },
      { key: "ccm-tmux", slot: "tmux", label: "tmux-label", vals: [], said: "tmux-said", from: "look", fromSaid: "FROM-look", overriddenBy: null },
      { key: "say", slot: "say", label: "say-label", vals: ["读 pb 图（夹具）"], said: "读 pb 图（夹具）", from: "look", fromSaid: "FROM-look", overriddenBy: null },
    ],
    line: null,
    lineShort: null,
    lineError: null,
    problem: null,
  };
  /** 那台多一个能起的号 lab（别名 look 用它）；回放回原样的那一手。 */
  const withLab = async (): Promise<() => void> => {
    const orig = vi.mocked(fetchAccounts).getMockImplementation()!;
    const st = await fetchAccounts("devbox");
    vi.mocked(fetchAccounts).mockImplementation(async () => ({ ...st, accounts: [...st.accounts, { ...st.accounts[1], name: "lab", configDir: "/h/.cc/lab", isDefault: false }] }));
    return () => void vi.mocked(fetchAccounts).mockImplementation(orig);
  };

  it("★ 选了别名 ⇒ 账号 · 运行于 · 开口那句写合下来的值 ＋「来自 look」；没改过的格不另交（照别名起）", async () => {
    const restore = await withLab();
    replies.set("profiles-read", BOOK);
    replies.set("profiles-resolve", RESOLVED);
    const done = openNewSession({ origin: "devbox" });
    await flush();
    choose(sel(copyText("newSession.label.alias")), "look");
    await flush();
    expect(sent.find((s) => s.op === "profiles-resolve")?.body).toMatchObject({ name: "look", edit: null });
    const acct = sel(copyText("beProfile.slot.account"));
    const from = "FROM-look";
    expect(acct.textContent, "账号格是别名合下来的号").toContain("lab");
    expect(rowOf(acct).textContent).toContain(from);
    const prompt = dialog().querySelector<HTMLTextAreaElement>(`textarea[aria-label="${copyText("beProfile.slot.say")}"]`)!;
    expect(prompt.value).toBe("读 pb 图（夹具）");
    expect(rowOf(prompt).textContent).toContain(from);
    createBtn().click();
    await flush();
    await done;
    const req = newRequests()[0];
    expect(whatOf(req).alias).toBe("look");
    expect(req.account, "账号照别名，不另交").toBeUndefined();
    expect(whatOf(req).prompt, "开口那句照别名，不另交").toBeUndefined();
    restore();
  });

  it("改一格 ⇒ 盖过别名（灰字消失、照交）；清空开口那句 ⇒ 回到别名；问不到 ⇒ 各格照缺省、别名格下写读不出", async () => {
    const restore = await withLab();
    replies.set("profiles-read", BOOK);
    replies.set("profiles-resolve", RESOLVED);
    const done = openNewSession({ origin: "devbox" });
    await flush();
    choose(sel(copyText("newSession.label.alias")), "look");
    await flush();
    const acct = sel(copyText("beProfile.slot.account"));
    choose(acct, "work");
    const from = "FROM-look";
    expect(rowOf(acct).textContent).not.toContain(from);
    const prompt = dialog().querySelector<HTMLTextAreaElement>(`textarea[aria-label="${copyText("beProfile.slot.say")}"]`)!;
    prompt.value = "";
    prompt.dispatchEvent(new Event("input"));
    prompt.dispatchEvent(new Event("blur"));
    expect(prompt.value, "清空 ⇒ 回到别名那一句").toBe("读 pb 图（夹具）");
    createBtn().click();
    await flush();
    await done;
    expect(newRequests()[0].account).toEqual({ kind: "named", name: "work" });
    document.body.replaceChildren();
    refusal = { op: "profiles-resolve", code: "refused", said: "devbox 离线", data: null };
    void openNewSession({ origin: "devbox" });
    await flush();
    const alias = sel(copyText("newSession.label.alias"));
    choose(alias, "look");
    await flush();
    expect(rowOf(alias).textContent).toContain("devbox 离线");
    restore();
  });

  it("页签「单个 · 一组」：换到一组页，主按钮字跟着换；分叉打开时没有页签", async () => {
    replies.set("profiles-read", BOOK);
    void openNewSession({ origin: "devbox" });
    await flush();
    const tab = (p: string): HTMLButtonElement => dialog().querySelector<HTMLButtonElement>(`[role="tab"][data-page="${p}"]`)!;
    expect(tab("single").getAttribute("aria-selected")).toBe("true");
    tab("set").click();
    await flush();
    expect(dialog().querySelector<HTMLElement>('[data-role="set-page"]')!.hidden).toBe(false);
    expect(createBtn()).toBeUndefined();
    document.body.replaceChildren();
    void openNewSession({ origin: "devbox", fork: { sid: "s", uuid: "u", title: "源" } });
    await flush();
    expect(dialog().querySelector('[role="tab"]')).toBeNull();
  });
});

describe("点［新建］：交那台的那一份", () => {
  it("★ 一问 `session-open`：哪一家 · 目录 · 点名的号 · 放在哪；没改终端名就不带（那台铸）；主窗口里起好了 ⇒ 交占位标签页（按展开过的目录认、带终端名），不接进终端、不再另等", async () => {
    const slot = vi.fn(() => true);
    setLaunchSlots(slot);
    const done = openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    await done;
    expect(newRequests()).toEqual([
      { what: { kind: "new", agent: "claude", cwd: "/home/u/srv/orders" }, run: "tmux", local: false, account: { kind: "named", name: "work" }, ticket: expect.any(String) },
    ]);
    expect(sent.map((x) => x.op).filter((op) => op !== "session-open-facts" && op !== "session-open-dir" && op !== "rotation-strategies" && op !== "machine-read" && op !== "profiles-read"), "只这一问").toEqual(["session-open"]);
    expect(document.querySelector('[role="dialog"]'), "起了框就关").toBeNull();
    await flush();
    expect(slot.mock.calls).toEqual([[{ origin: "devbox", cwd: "/home/u/srv/orders", tmuxName: "orders-cc", arrival: OK.arrival }]]);
    expect(arriveAsks(), "主窗口里起的由占位标签页去问，这里不问").toEqual([]);
    expect(term.openTerminal, "后台起的不接进终端").not.toHaveBeenCalled();
  });

  it("★ 轮换：缺省新会话用的那份（不写进请求）；选「不轮换」⇒ 请求带 `rotation: {strategy: null}`", async () => {
    replies.set("rotation-strategies", {
      strategies: [{ id: "按权重轮换", description: "第一行说明\n第二行", rev: "h:1", inUse: [], forNew: true, wordsBad: false, tzUnknown: false }],
      forNew: "按权重轮换",
      oldScripts: 0,
    });
    const done = openNewSession({ origin: "devbox" });
    await flush();
    const sel = document.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("machinePage.tab.rot")}"]`)!;
    expect(sel.textContent).toContain("按权重轮换");
    sel.click();
    [...document.querySelectorAll<HTMLElement>('[role="menuitem"], [role="menuitemradio"], [role="option"]')].find((e) => e.textContent?.includes(copyText("rot.none.label")))!.click();
    createBtn().click();
    await flush();
    await done;
    expect(whatOf(newRequests()[0]).rotation).toEqual({ strategy: null });
    replies.delete("rotation-strategies");
  });

  it("开窗那一形：那台给那一行 ⇒ 开终端跑它，按展开过的目录认报到的会话", async () => {
    const arrival = { by: { kind: "cwd", cwd: "/home/u/x" }, since: 5, session: null };
    replies.set("session-open", { ...OK, landed: "window", session: null, line: "ccm -- new", cwd: "/home/u/x", arrival });
    void openNewSession({ origin: "devbox" });
    await flush();
    dialog().querySelector<HTMLInputElement>('input[value="window"]')!.click();
    createBtn().click();
    await flush();
    expect(newRequests()[0].run).toBe("direct");
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", "ccm -- new", null);
    expect(arriveAsks()).toEqual([arrival]);
  });

  it("★ 在别的窗口（设置 · 查看）里起的：没有占位标签页 ⇒ 等那台报到，报到了说「已启动」＋［切过去］", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    await flush();
    expect(arriveAsks(), "等报到只问一次、回包那一问原样").toEqual([OK.arrival]);
    expect(document.body.textContent).toContain(copyText("launch.fromSettings.done", { name: "orders-cc", machine: "devbox" }));
  });

  it("★ 那台说目录那一格不行 ⇒ 错误落在目录那一格下、框不关、什么都没起", async () => {
    refusal = { op: "session-open", code: "no_dir", said: copyText("beSessionNew.cwd.missing", { cwd: "/nope" }), data: { field: "cwd", unavailable: null } };
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    expect(dialog(), "框不关").toBeTruthy();
    expect(rowOf(input(copyText("newSession.label.cwd"))).textContent).toContain(copyText("launch.dir.missing", { machine: "devbox" }));
    expect(arriveAsks()).toEqual([]);
  });

  it("★ 号选不了 ⇒ 不悄悄换号：那一格下给［改用 {替代}］，点了以那个号再交一次", async () => {
    refusal = {
      op: "session-open",
      code: "account_unavailable",
      said: "那台说 work 选不了",
      data: { field: "account", unavailable: { requested: "work", pinned: false, listKnown: true, alternative: "personal" } },
    };
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    const acctRow = rowOf(sel(copyText("beProfile.slot.account")));
    expect(acctRow.textContent, "那一句是那台写的，原样上屏").toContain("那台说 work 选不了");
    const use = [...acctRow.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("newSession.account.useAlt", { alt: "personal" }))!;
    expect(use).toBeTruthy();
    expect(newRequests()).toHaveLength(1);
    use.click();
    await flush();
    expect(newRequests()[1].account).toEqual({ kind: "named", name: "personal" });
  });

  it("★ 期限到 ⇒ 带同一张票自己再核一次；那台说起好了 ⇒ 落到那个会话、框关上（不起第二个）", async () => {
    hang = 1;
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    const [a, b] = newRequests();
    expect(typeof a.ticket).toBe("string");
    expect(b.ticket, "再核那一次带的是同一张票").toBe(a.ticket);
    expect(newRequests()).toHaveLength(2);
    expect(document.querySelector('[role="dialog"]'), "起好了 ⇒ 框关上").toBeNull();
    expect(arriveAsks()).toEqual([OK.arrival]);
  });

  it("期限到、正在再核那一拍 ⇒ 框顶那一条是中性的（没出错：不用错误色、不当警报念）", async () => {
    hang = 1;
    void openNewSession({ origin: "devbox" });
    await flush();
    stall = true;
    createBtn().click();
    await flush();
    const top = dialog().querySelector<HTMLElement>("[class*=nsTop]")!;
    expect(top.textContent).toContain(copyText("newSession.timeout.checking", { machine: "devbox" }));
    const bar = top.firstElementChild as HTMLElement;
    expect([bar.dataset.intent, bar.getAttribute("role"), bar.querySelector("svg")?.dataset.icon]).toEqual(["info", "status", "info"]);
  });

  it("★ 再核也无应答 ⇒ 框顶「起没起未知」＋［再核一次］＋［复制详情］、没有［重试］；［再核一次］带同一张票，那台说起好了就落过去", async () => {
    hang = 2;
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    const top = dialog().querySelector<HTMLElement>("[class*=nsTop]")!;
    expect(top.textContent).toContain(copyText("newSession.timeout.unknown", { machine: "devbox" }));
    const labels = [...top.querySelectorAll("button")].map((x) => x.textContent);
    expect(labels, "无应答只给［再核一次］（同一张票，不会起第二个），不给［重试］").toEqual([copyText("newSession.recheck.action"), copyText("detail.act.copy")]);
    [...top.querySelectorAll<HTMLButtonElement>("button")].find((x) => x.textContent === copyText("newSession.recheck.action"))!.click();
    await flush();
    const tickets = newRequests().map((r) => r.ticket);
    expect(tickets).toHaveLength(3);
    expect(new Set(tickets).size, "三次都是同一张票").toBe(1);
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(arriveAsks()).toEqual([OK.arrival]);
  });

  it("★ 起没起未知时：表单一格没改就点［新建］⇒ 还是那张票（等于再核一次，不起第二个）；改过任一格再点 ⇒ 换一张新票", async () => {
    hang = 2;
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    hang = 2;
    createBtn().click();
    await flush();
    const t = newRequests().map((r) => r.ticket);
    expect(t).toHaveLength(4);
    expect(new Set(t).size, "没改表单 ⇒ 同一张票").toBe(1);
    const cwd = input(copyText("newSession.label.cwd"));
    cwd.value = "/home/u/srv/billing";
    cwd.dispatchEvent(new Event("input", { bubbles: true }));
    createBtn().click();
    await flush();
    const last = newRequests().at(-1)!;
    expect(whatOf(last).cwd).toBe("/home/u/srv/billing");
    expect(last.ticket, "改过一格 ⇒ 新票").not.toBe(t[0]);
  });

  it("那台说这一趟还在起 ⇒ 框顶那一句 ＋［再核一次］＋［复制详情］，不重起", async () => {
    hang = 1;
    refusal = { op: "session-open", code: "launch_pending", said: "还在起-甲", data: { field: null, unavailable: null }, detail: "码：launch_pending" };
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    const top = dialog().querySelector<HTMLElement>("[class*=nsTop]")!;
    expect(top.textContent).toContain("还在起-甲");
    const labels = [...top.querySelectorAll("button")].map((x) => x.textContent);
    expect(labels).toContain(copyText("newSession.recheck.action"));
    expect(labels).toContain(copyText("detail.act.copy"));
    expect(newRequests()).toHaveLength(2);
  });
});

describe("分叉那一形", () => {
  it("★ 顶上说分叉自哪一轮；没有终端名那一格；交的那一份带 fork（只这两个 id）、号跟随源会话", async () => {
    replies.set("session-open-facts", {
      ...FACTS,
      fork: {
        agent: "claude",
        launch: { cwd: { kind: "known", value: "/home/u/work/orders", from: "record" }, account: { kind: "unknown", why: "exited" }, terminal: { kind: "unknown", why: "exited" } },
        turn: 9,
        startText: "02:05",
      },
    });
    void openNewSession({ origin: "devbox", fork: { sid: "src-1", uuid: "msg-9", title: "给订单服务加重试" } });
    await flush();
    expect(sent.find((s) => s.op === "session-open-facts")!.body).toEqual({ forkOf: "src-1", at: "msg-9" });
    expect(dialog().textContent, "那一轮的钟面照抄后端写好的字").toContain(copyText("newSession.fork.from", { title: "给订单服务加重试", n: 9, time: "02:05" }));
    expect(dialog().querySelector(`input[aria-label="${copyText("newSession.label.tmuxName")}"]`), "分叉不给终端名那一格").toBeNull();
    expect(input(copyText("newSession.label.cwd")).value).toBe("/home/u/work/orders");
    expect(sel(copyText("detail.label.machine")).disabled).toBe(true);
    createBtn().click();
    await flush();
    const req = newRequests()[0];
    expect(whatOf(req).fork).toEqual({ sid: "src-1", uuid: "msg-9" });
    expect(req.account, "源会话的号说不出 ⇒ 跟随（那台按源会话上次的号判），不拿当前号顶替").toBeUndefined();
    expect(whatOf(req).tmuxName).toBeUndefined();
  });
});

describe("换机器 · 换一家 · 那台问不到", () => {
  const top = (): string => dialog().querySelector<HTMLElement>("[class*=nsTop]")!.textContent ?? "";

  it("换一台 ⇒ 目录清空、照新那台重问、重新预填", async () => {
    replies.set("session-open-facts", { ...FACTS, recent: [{ cwd: "/srv/a", lastMs: 1, lastText: "00:00" }] });
    void openNewSession({ origin: "<local>" });
    await flush();
    const cwd = input(copyText("newSession.label.cwd"));
    cwd.value = "/typed";
    replies.set("session-open-facts", { ...FACTS, recent: [{ cwd: "/srv/b", lastMs: 1, lastText: "00:00" }] });
    const asked = sent.filter((x) => x.op === "session-open-facts").length;
    choose(sel(copyText("detail.label.machine")), "devbox");
    await flush();
    expect(sent.filter((x) => x.op === "session-open-facts").length).toBe(asked + 1);
    expect(cwd.value, "换机器没清掉手填的目录").toBe("/srv/b");
  });

  it("★ 机器清单只问本机后端一条 `machine-read`（各台的状态成品），一台都不问；关着的那台不列、断开的灰着", async () => {
    replies.set("machine-read", { machines: { ...MACHINES.machines, gpu: machineOf("gpu", "down"), old: machineOf("old", "disabled") } });
    void openNewSession({ origin: "<local>" });
    await flush();
    expect(sent.filter((x) => x.op === "machine-read").length).toBe(1);
    expect(commands.backend_status).not.toHaveBeenCalled();
    expect(commands.backend_machines).not.toHaveBeenCalled();
    const items = optionsOf(sel(copyText("detail.label.machine")));
    expect(items.some((t) => t.includes("old")), "关着的不列").toBe(false);
    expect(items.find((t) => t.includes("gpu")), "断开的那台标着").toContain(copyText("machinePage.state.down"));
    expect(items.find((t) => t.includes("devbox"))).not.toContain(copyText("machinePage.state.down"));
  });

  it("那台连不上、成品还没到 ⇒ 框顶不说离线（不知道 ≠ 离线），说状态不明", async () => {
    void openNewSession({ origin: "box-no-product" });
    await flush();
    expect(top()).toContain(copyText("settingsNav.dot.unknown", { machine: "box-no-product" }));
    expect(createBtn().title).toBe(copyText("settingsNav.dot.unknown", { machine: "box-no-product" }));
  });

  it("那台连不上 ⇒ 框顶照那台成品说离线 ＋［重连］（点了请后端起那台）、［新建］灰着说为什么；不问那台", async () => {
    replies.set("machine-read", { machines: { ...MACHINES.machines, devbox: machineOf("devbox", "down", "成品写的 devbox 离线那一句") } });
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(top()).toContain("成品写的 devbox 离线那一句");
    expect(createBtn().title).toBe("成品写的 devbox 离线那一句");
    expect(sent.filter((x) => x.op.startsWith("session-")), "断开还去问那台").toEqual([]);
    [...dialog().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("history.list.reconnect"))!.click();
    expect(commands.backend_start).toHaveBeenCalledWith({ origin: "devbox" });
  });

  it("★ ［重新连接］被那台拒了 ⇒ 框顶换成那一句（不吞）", async () => {
    replies.set("machine-read", { machines: { ...MACHINES.machines, devbox: machineOf("devbox", "down", "成品写的 devbox 离线那一句") } });
    vi.mocked(commands.backend_start).mockRejectedValueOnce(new Error("devbox 停用着-甲"));
    void openNewSession({ origin: "devbox" });
    await flush();
    [...dialog().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("history.list.reconnect"))!.click();
    await flush();
    expect(top()).toContain("devbox 停用着-甲");
  });

  it("那台问不到能起什么 ⇒ 框顶说原因、［新建］灰着说还在读", async () => {
    refusal = { op: "session-open-facts", code: "boom", said: "读不出", data: null };
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(top()).toContain(copyText("newSession.facts.failed", { machine: "devbox", why: "\u0000" }).split("\u0000")[0]);
    expect(createBtn().title).toBe(copyText("kit.state.reading"));
  });

  it("目录空着 ⇒ ［新建］灰着说要填；填了就亮", async () => {
    replies.set("session-open-facts", { ...FACTS, recent: [] });
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(createBtn().title).toBe(copyText("newSession.cwd.empty"));
    const cwd = input(copyText("newSession.label.cwd"));
    cwd.value = "/home/u/x";
    cwd.dispatchEvent(new Event("input"));
    expect(createBtn().getAttribute("aria-disabled")).toBeNull();
  });

  it("换一家 ⇒ 启动命令的占位跟着那一家、号那一行照那一家重列", async () => {
    replies.set("session-open-facts", { ...FACTS, agents: [agentOf("claude"), agentOf("codex")] });
    void openNewSession({ origin: "devbox" });
    await flush();
    const cmd = input(copyText("newSession.label.command"));
    const before = cmd.placeholder;
    // 菜单里写那台写好的名字（不是线上的 kind），选中交回的是 kind。
    choose(sel(copyText("beProfile.slot.agent")), "Codex");
    await flush();
    expect(sel(copyText("beProfile.slot.agent")).dataset.value).toBe("codex");
    expect(cmd.placeholder).not.toBe(before);
    createBtn().click();
    await flush();
    expect(whatOf(newRequests()[0]).agent).toBe("codex");
  });
});

describe("目录格 · 最近的 · 更多那几格", () => {
  it("离开目录格 ⇒ 问那台在不在：不在 ⇒ 那一格下黄字；终端名占位跟着那台铸的名字", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    replies.set("session-open-dir", { exists: false, tmuxName: "nope-cc" });
    const cwd = input(copyText("newSession.label.cwd"));
    cwd.value = "/nope";
    cwd.dispatchEvent(new Event("blur"));
    await flush();
    expect(sent.filter((x) => x.op === "session-open-dir").at(-1)!.body).toEqual({ cwd: "/nope" });
    expect(rowOf(cwd).textContent).toContain(copyText("launch.dir.missing", { machine: "devbox" }));
    expect(input(copyText("newSession.label.tmuxName")).placeholder).toBe("nope-cc");
  });

  it("［最近的］列那台用过的目录（当前那个打勾），点一项填进目录格；那台一个都没有 ⇒ 一项灰着的「没有」", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    const recent = dialog().querySelector<HTMLButtonElement>(`button[aria-label="${copyText("newSession.recent.label")}"]`)!;
    recent.click();
    const items = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')];
    // 每项画那台写好的显示形（照抄）；点了填进目录格的是原路径。
    expect(items.map((i) => i.textContent)).toEqual(["~/srv/orders", "~/srv/billing"]);
    expect(items.map((i) => i.getAttribute("aria-checked"))).toEqual(["true", "false"]);
    items[1].click();
    await flush();
    expect(input(copyText("newSession.label.cwd")).value).toBe("/home/u/srv/billing");
    document.body.replaceChildren();
    replies.set("session-open-facts", { ...FACTS, recent: [] });
    void openNewSession({ origin: "devbox" });
    await flush();
    dialog().querySelector<HTMLButtonElement>(`button[aria-label="${copyText("newSession.recent.label")}"]`)!.click();
    const none = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')];
    expect(none.map((i) => [i.textContent, (i.closest("button") ?? i.querySelector("button") ?? (i as HTMLButtonElement)).disabled])).toEqual([[copyText("newSession.recent.none"), true]]);
  });

  it("填了终端名与启动命令 ⇒ 请求里带上（tmux 那一形才带终端名）；在格里回车 = 点［新建］", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    input(copyText("newSession.label.tmuxName")).value = " mine ";
    const cmd = input(copyText("newSession.label.command"));
    cmd.value = " ccm -- x ";
    cmd.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    expect(newRequests()).toHaveLength(1);
    expect(newRequests()[0]).toMatchObject({ what: { tmuxName: "mine", command: "ccm -- x" }, run: "tmux" });
  });

  it("「更多」标题列的就是展开后在的那几格：那台没有 tmux ⇒ 不写 tmux 会话名；有 tmux 选终端窗口 ⇒ 也不写，切回 tmux ⇒ 写回，展开着的不收起", async () => {
    const moreHead = (): HTMLElement => dialog().querySelector<HTMLElement>("[data-fold-head]")!;
    replies.set("session-open-facts", { ...FACTS, hosts: ["direct"], run: "direct" });
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(moreHead().textContent).toBe(copyText("newSession.more.fork"));
    document.body.replaceChildren();
    replies.set("session-open-facts", FACTS);
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(moreHead().textContent).toBe(copyText("newSession.more.plain"));
    moreHead().click();
    dialog().querySelector<HTMLInputElement>('input[value="window"]')!.click();
    expect(moreHead().textContent).toBe(copyText("newSession.more.fork"));
    expect(moreHead().getAttribute("aria-expanded")).toBe("true");
    dialog().querySelector<HTMLInputElement>('input[value="tmux"]')!.click();
    expect(moreHead().textContent).toBe(copyText("newSession.more.plain"));
  });

  it("切到终端窗口 ⇒ 终端名那一格收起、请求里不带它", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    const tmuxName = input(copyText("newSession.label.tmuxName"));
    tmuxName.value = "mine";
    const w = dialog().querySelector<HTMLInputElement>('input[value="window"]')!;
    w.click();
    expect(rowOf(tmuxName).hidden).toBe(true);
    createBtn().click();
    await flush();
    expect(whatOf(newRequests()[0]).tmuxName).toBeUndefined();
  });
});

describe("锁住的目录 · 第一句", () => {
  const prompt = (): HTMLTextAreaElement => dialog().querySelector<HTMLTextAreaElement>(`textarea[aria-label="${copyText("beProfile.slot.say")}"]`)!;
  const recentBtn = (): HTMLButtonElement | null => dialog().querySelector<HTMLButtonElement>(`button[aria-label="${copyText("newSession.recent.label")}"]`);

  it("★ 入口锁了目录 ⇒ 目录格只读、不拿最近用过的顶替、［最近的］不出；交的就是那个目录", async () => {
    const done = openNewSession({ origin: "devbox", cwd: "/work/ledger", lockCwd: true });
    await flush();
    const cwd = input(copyText("newSession.label.cwd"));
    expect(cwd.value).toBe("/work/ledger");
    expect(cwd.readOnly).toBe(true);
    expect(recentBtn()?.hidden ?? true, "锁了目录就没有［最近的］").toBe(true);
    createBtn().click();
    await flush();
    await done;
    expect(whatOf(newRequests()[0]).cwd).toBe("/work/ledger");
  });

  it("没锁 ⇒ 目录格照常可改、［最近的］在", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(input(copyText("newSession.label.cwd")).readOnly).toBe(false);
    expect(recentBtn()!.hidden).toBe(false);
  });

  it("★ 入口给了开口那句 ⇒ 预填进「第一句」那一格，点［新建］交 `what.prompt`（照原样）", async () => {
    const said = "你负责 ledger 的 A2 导入 csv：先 pb claim A2 <认领码>，再 pb where";
    const done = openNewSession({ origin: "devbox", prompt: said });
    await flush();
    expect(prompt().value).toBe(said);
    createBtn().click();
    await flush();
    await done;
    expect(whatOf(newRequests()[0]).prompt).toBe(said);
  });

  it("「第一句」空着（或只有空白）⇒ 请求不带 prompt", async () => {
    const done = openNewSession({ origin: "devbox" });
    await flush();
    expect(prompt().value).toBe("");
    prompt().value = "  ";
    createBtn().click();
    await flush();
    await done;
    expect("prompt" in whatOf(newRequests()[0])).toBe(false);
  });

  it("切到终端窗口 ⇒ 「第一句」那一格灰着（字留着）、请求里不带它（第一句只跟 tmux）", async () => {
    void openNewSession({ origin: "devbox", prompt: "开口那句" });
    await flush();
    expect(prompt().disabled).toBe(false);
    dialog().querySelector<HTMLInputElement>('input[value="window"]')!.click();
    expect(prompt().disabled).toBe(true);
    expect(prompt().value).toBe("开口那句");
    createBtn().click();
    await flush();
    expect("prompt" in whatOf(newRequests()[0])).toBe(false);
  });
});

describe("那台说不行的其余几形", () => {
  it("哪一格不行落在哪一格下（终端名 · 启动命令）；那一格收着 ⇒ 落到框顶", async () => {
    refusal = { op: "session-open", code: "tmux_taken", said: "原话", data: { field: "tmuxName", unavailable: null } };
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    expect(rowOf(input(copyText("newSession.label.tmuxName"))).textContent).toContain(copyText("launch.tmux.taken"));
    refusal = { op: "session-open", code: "bad_cmd", said: "命令不行", data: { field: "command", unavailable: null } };
    createBtn().click();
    await flush();
    expect(rowOf(input(copyText("newSession.label.command"))).textContent).toContain("命令不行");
    expect(rowOf(input(copyText("newSession.label.tmuxName"))).textContent, "再交一次先清掉上一次的话").not.toContain(copyText("launch.tmux.taken"));
    refusal = { op: "session-open", code: "bad_agent", said: "这一家不行", data: { field: "agent", unavailable: null }, detail: "码：bad_agent" };
    createBtn().click();
    await flush();
    // 那一格收着 ⇒ 整体那一形：按钮行上方一行红字 ＋［复制详情］（表单框的那一形）。
    const fail = dialog().querySelector<HTMLElement>("[class*=dlgFail]")!;
    expect(fail.hidden).toBe(false);
    expect(fail.textContent).toContain("这一家不行");
    expect([...fail.querySelectorAll("button")].map((x) => x.textContent)).toContain(copyText("detail.act.copy"));
  });

  it("通道不通 ⇒ 按钮行上方说那一句；号被钉住（没有替代）⇒ 那一格下是那台那一句、只给［登录…］", async () => {
    boom = new Error("线断了");
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    expect(dialog().querySelector<HTMLElement>("[class*=dlgFail]")!.textContent).toContain("线断了");
    boom = null;
    refusal = {
      op: "session-open",
      code: "account_unavailable",
      said: "work 被钉住了-甲",
      data: { field: "account", unavailable: { requested: "work", pinned: true, listKnown: true, alternative: null } },
    };
    createBtn().click();
    await flush();
    const note = rowOf(sel(copyText("beProfile.slot.account"))).querySelector<HTMLElement>("[class*=nsNote]")!;
    expect(note.textContent).toContain("work 被钉住了-甲");
    expect([...note.querySelectorAll("button")].map((b) => b.textContent)).toEqual([copyText("newSession.account.login")]);
  });
});

describe("分叉：那台说得出源会话的号与终端", () => {
  it("号说得出 ⇒ 预选那个号、没有「跟随」；那台说默认放在终端窗口 ⇒ 预选终端窗口；那一家照源会话", async () => {
    replies.set("session-open-facts", {
      ...FACTS,
      run: "direct",
      agents: [agentOf("codex"), agentOf("claude")],
      fork: {
        agent: "claude",
        launch: { cwd: { kind: "unknown", why: "exited" }, account: { kind: "known", value: "personal", from: "record" }, terminal: { kind: "known", value: { host: "none" }, from: "record" } },
        turn: null,
        startText: null,
      },
    });
    void openNewSession({ origin: "devbox", fork: { sid: "src-1", uuid: "msg-9", title: "t" } });
    await flush();
    expect(dialog().textContent).toContain(copyText("newSession.fork.fromBare", { title: "t" }));
    expect(input(copyText("newSession.label.cwd")).value, "源会话目录说不出 ⇒ 用最近的").toBe("/home/u/srv/orders");
    expect(dialog().querySelector<HTMLInputElement>('input[value="window"]')!.checked).toBe(true);
    expect(sel(copyText("beProfile.slot.agent")).dataset.value).toBe("claude");
    const acct = sel(copyText("beProfile.slot.account"));
    expect(acct.dataset.value).toBe("personal");
    expect(optionsOf(acct).some((o) => o.includes(copyText("newSession.account.follow")))).toBe(false);
  });
});

describe("问图框：三格 ＋ 那台写的起在哪", () => {
  const BOOK = { rev: "r", sets: [], profiles: [{ name: "look", usable: true }, { name: "dev", usable: true }] };
  const textarea = (): HTMLTextAreaElement => dialog().querySelector<HTMLTextAreaElement>(`textarea[aria-label="${copyText("beProfile.slot.say")}"]`)!;
  const askOf = (runs: string | null, relined = async (a: string) => ({ line: `L-${a}`, runs: `R-${a}` })) => ({
    origin: "devbox",
    lockMachine: true,
    cwd: "/work/ledger",
    lockCwd: true,
    prompt: "L-look",
    ask: { title: "问图 · 编码探测", alias: runs === null ? null : "look", runs, relined },
  });

  it("★ 框里没有机器那一格；目录锁着、格内标「工作区根」；开口那句下那一行是那台写的 runs", async () => {
    replies.set("profiles-read", BOOK);
    void openNewSession(askOf("tmux 里 · 号 lab（夹具）"));
    await flush();
    expect(rowOf(sel(copyText("detail.label.machine"))).hidden, "机器那一格不出").toBe(true);
    const cwd = input(copyText("newSession.label.cwd"));
    expect(cwd.readOnly).toBe(true);
    expect(rowOf(cwd).textContent).toContain(copyText("newSession.ask.cwdTag"));
    expect(rowOf(textarea()).textContent).toContain("tmux 里 · 号 lab（夹具）");
  });

  it("★ 没选别名 ⇒ 那一行写「先选别名」；改选别名 ⇒ 换成那台新写的那一句", async () => {
    replies.set("profiles-read", BOOK);
    void openNewSession(askOf(null));
    await flush();
    expect(rowOf(textarea()).textContent).toContain(copyText("newSession.ask.runsNone"));
    choose(sel(copyText("newSession.label.alias")), "dev");
    await flush();
    expect(rowOf(textarea()).textContent).toContain("R-dev");
    expect(rowOf(textarea()).textContent).not.toContain(copyText("newSession.ask.runsNone"));
  });
});
