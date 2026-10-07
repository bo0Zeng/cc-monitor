/**
 * 起新会话框（`new-session.ts`）：那台说什么就画什么 —— 预填 · 几行出不出 · 点［新建］交的那一份 · 某一格不行落在那一格下。
 * 替身：通道（三问）· 机器清单 · 账号 · 额度 · 等报到 · 开窗。夹具只造结构。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const sent: { op: string; body: Record<string, unknown> }[] = [];
const replies = new Map<string, unknown>();
let refusal: { op: string; code: string; message: string; data: unknown } | null = null;
let hang = false;

vi.mock("../../../src/comms/inward/chan", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../../src/comms/inward/chan")>();
  return {
    ...real,
    chan: {
      call: async (_origin: string, op: string, body: Uint8Array) => {
        sent.push({ op, body: JSON.parse(new TextDecoder().decode(body)) });
        if (refusal && refusal.op === op) {
          const r = refusal;
          refusal = null;
          throw new real.ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code: r.code, message: r.message, data: r.data })) });
        }
        if (hang && op === "session-new") {
          throw new real.ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" });
        }
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
vi.mock("../../../src/frontend/ui/account-reads", () => ({
  fetchAccounts: vi.fn(async () => ({
    available: true,
    accounts: [
      { name: "personal", email: "", configDir: "/h/.cc/personal", isDefault: false, mode: "isolated", exists: true, loggedIn: false, authKind: "subscription", authReady: false },
      { name: "work", email: "", configDir: "/h/.cc/work", isDefault: true, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true },
    ],
  })),
}));
vi.mock("../../../src/frontend/ui/acct-center", () => ({ refreshQuota: vi.fn(async () => {}) }));
vi.mock("../../../src/frontend/ui/account-prefs", () => ({ machineModels: vi.fn(async () => ({})) }));
vi.mock("../../../src/frontend/ui/behavior", () => ({ getBehavior: vi.fn(async () => ({ resumeCommandLocal: "", resumeCommandRemote: "" })) }));
vi.mock("../../../src/frontend/ui/remote-config", () => ({ resolveResumeCommand: vi.fn(async () => "") }));
const arrival = vi.hoisted(() => ({ awaitArrival: vi.fn(async () => "new-sid") }));
vi.mock("../../../src/frontend/ui/launch-arrival", () => arrival);
const win = vi.hoisted(() => ({ openWindow: vi.fn(async () => null) }));
vi.mock("../../../src/frontend/ui/tab-batch-run", () => win);
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => {}), listen: vi.fn(async () => () => {}) }));

import { openNewSession, setNewSessionFocus } from "../../../src/frontend/ui/new-session";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};
const dialog = (): HTMLElement => document.querySelector<HTMLElement>('[role="dialog"]')!;
const sel = (label: string): HTMLSelectElement => dialog().querySelector<HTMLSelectElement>(`select[aria-label="${label}"]`)!;
const input = (label: string): HTMLInputElement => dialog().querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;
const rowOf = (el: HTMLElement): HTMLElement => el.closest<HTMLElement>("[class*=nsRow]")!;
const createBtn = (): HTMLButtonElement => [...dialog().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "新建")!;
const newRequests = (): Record<string, unknown>[] => sent.filter((s) => s.op === "session-new").map((s) => s.body);

const FACTS = { recent: [{ cwd: "/home/u/srv/orders", lastMs: 2 }, { cwd: "/home/u/srv/billing", lastMs: 1 }], tmux: true, agents: ["claude"], fork: null };
const OK = { outcome: "started", session: "orders-cc", sid: null, cmd: null, account: { name: "work", configDir: "/h/.cc/work", model: null }, agent: "claude", cwd: "/home/u/srv/orders" };

beforeEach(() => {
  document.body.replaceChildren();
  sent.length = 0;
  replies.clear();
  refusal = null;
  hang = false;
  replies.set("session-new-facts", FACTS);
  replies.set("session-new-dir", { exists: true, tmuxName: "orders-cc" });
  replies.set("session-new", OK);
  arrival.awaitArrival.mockClear();
  win.openWindow.mockClear();
});

describe("起新会话框：预填与几行出不出（都照那台说的）", () => {
  it("★ 目录预填那台最近用过的第一个；只有一家能起 ⇒ 没有 agent 那一行；账号默认号在前、需登录的标出来", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(input("工作目录").value).toBe("/home/u/srv/orders");
    expect(rowOf(sel("agent")).hidden).toBe(true);
    const acct = sel("账号");
    expect(acct.value).toBe("work");
    expect([...acct.options].map((o) => o.textContent)).toEqual(["work 默认", "personal · 需登录"]);
    expect(input("tmux 会话名").placeholder).toBe("orders-cc");
    expect(sent.find((s) => s.op === "session-new-facts")!.body).toEqual({});
  });

  it("那台能起两家 ⇒ 出 agent 那一行；那台没 tmux ⇒ 只剩终端窗口那一张、灰字说为什么", async () => {
    replies.set("session-new-facts", { ...FACTS, tmux: false, agents: ["claude", "codex"] });
    void openNewSession({ origin: "devbox" });
    await flush();
    expect(rowOf(sel("agent")).hidden).toBe(false);
    const radios = [...dialog().querySelectorAll<HTMLInputElement>('input[type="radio"]')];
    expect(radios.find((r) => r.value === "tmux")!.closest("label")!.hidden).toBe(true);
    expect(radios.find((r) => r.value === "window")!.checked).toBe(true);
    expect(dialog().textContent).toContain("无 tmux · 仅终端窗口");
  });

  it("选了需登录的号 ⇒ 那一格下说「需登录」＋［登录…］，［新建］灰着", async () => {
    void openNewSession({ origin: "devbox" });
    await flush();
    const acct = sel("账号");
    acct.value = "personal";
    acct.dispatchEvent(new Event("change"));
    expect(rowOf(acct).textContent).toContain("personal 需登录");
    expect(rowOf(acct).textContent).toContain("登录…");
    expect(createBtn().getAttribute("aria-disabled")).toBe("true");
  });
});

describe("点［新建］：交那台的那一份", () => {
  it("★ 一个请求：哪一家 · 目录 · 点名的号 · 放在哪；没改终端名就不带（那台铸）；起好了切到报到的那个会话", async () => {
    const focus = vi.fn();
    setNewSessionFocus(focus);
    const done = openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    await done;
    expect(newRequests()).toEqual([
      { agent: "claude", cwd: "/home/u/srv/orders", place: "tmux", local: false, models: {}, account: { kind: "named", name: "work" } },
    ]);
    expect(document.querySelector('[role="dialog"]'), "起了框就关").toBeNull();
    await flush();
    expect(arrival.awaitArrival).toHaveBeenCalledWith(expect.objectContaining({ origin: "devbox", match: { cwd: "/home/u/srv/orders" }, tmuxName: "orders-cc" }));
    expect(focus).toHaveBeenCalledWith("devbox", "new-sid");
    expect(sent.some((s) => s.op === "terminal-name-mint" || s.op.startsWith("launch-render")), "界面不再自己拼那一串").toBe(false);
  });

  it("开窗那一形：那台给那一行 ⇒ 开窗跑它，按展开过的目录认报到的会话", async () => {
    replies.set("session-new", { ...OK, outcome: "open", session: null, cmd: "ccm -- new", cwd: "/home/u/x" });
    void openNewSession({ origin: "devbox" });
    await flush();
    dialog().querySelector<HTMLInputElement>('input[value="window"]')!.click();
    createBtn().click();
    await flush();
    expect(newRequests()[0].place).toBe("window");
    expect(win.openWindow).toHaveBeenCalledWith("devbox", "ccm -- new", "/home/u/x");
    expect(arrival.awaitArrival).toHaveBeenCalledWith(expect.objectContaining({ match: { cwd: "/home/u/x" }, tmuxName: null }));
  });

  it("★ 那台说目录那一格不行 ⇒ 错误落在目录那一格下、框不关、什么都没起", async () => {
    refusal = { op: "session-new", code: "no_dir", message: "目录不存在：/nope，没有起。", data: { field: "cwd", unavailable: null } };
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    expect(dialog(), "框不关").toBeTruthy();
    expect(rowOf(input("工作目录")).textContent).toContain("目录不存在 · devbox");
    expect(arrival.awaitArrival).not.toHaveBeenCalled();
  });

  it("★ 号选不了 ⇒ 不悄悄换号：那一格下给［改用 {替代}］，点了以那个号再交一次", async () => {
    refusal = {
      op: "session-new",
      code: "account_unavailable",
      message: "x",
      data: { field: "account", unavailable: { requested: "work", pinned: false, listKnown: true, alternative: "personal" } },
    };
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    const acctRow = rowOf(sel("账号"));
    expect(acctRow.textContent).toContain("work 不可用");
    const use = [...acctRow.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "改用 personal")!;
    expect(use).toBeTruthy();
    expect(newRequests()).toHaveLength(1);
    use.click();
    await flush();
    expect(newRequests()[1].account).toEqual({ kind: "named", name: "personal" });
  });

  it("期限到 ⇒ 框顶「启动无应答 · {机器}」＋［重试］，不说失败", async () => {
    hang = true;
    void openNewSession({ origin: "devbox" });
    await flush();
    createBtn().click();
    await flush();
    expect(dialog().textContent).toContain("启动无应答 · devbox");
    expect([...dialog().querySelectorAll("button")].some((b) => b.textContent === "重试")).toBe(true);
  });
});

describe("分叉那一形", () => {
  it("★ 顶上说分叉自哪一轮；没有终端名那一格；交的那一份带 forkFrom（只这两个 id）、号跟随源会话", async () => {
    replies.set("session-new-facts", {
      ...FACTS,
      fork: {
        agent: "claude",
        launch: { cwd: { kind: "known", value: "/home/u/work/orders", from: "record" }, account: { kind: "unknown", why: "exited" }, terminal: { kind: "unknown", why: "exited" } },
        turn: 9,
        start: "2026-10-06T02:05:00Z",
      },
    });
    void openNewSession({ origin: "devbox", fork: { sid: "src-1", uuid: "msg-9", title: "给订单服务加重试" } });
    await flush();
    expect(sent.find((s) => s.op === "session-new-facts")!.body).toEqual({ forkOf: "src-1", at: "msg-9" });
    expect(dialog().textContent).toMatch(/分叉自「给订单服务加重试」· 第 9 轮 \d\d:\d\d/);
    expect(dialog().querySelector('input[aria-label="tmux 会话名"]'), "分叉不给终端名那一格").toBeNull();
    expect(input("工作目录").value).toBe("/home/u/work/orders");
    expect(sel("机器").disabled).toBe(true);
    createBtn().click();
    await flush();
    const req = newRequests()[0];
    expect(req.forkFrom).toEqual({ sid: "src-1", uuid: "msg-9" });
    expect(req.account, "源会话的号说不出 ⇒ 跟随（那台按源会话上次的号判），不拿当前号顶替").toBeUndefined();
    expect(req.tmuxName).toBeUndefined();
  });
});
