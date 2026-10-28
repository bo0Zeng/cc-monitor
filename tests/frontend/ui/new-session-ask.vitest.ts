/**
 * 起新会话框的问图样子（`spec.ask`，计划页［问这一块…］开的）：框里只别名 · 目录 · 开口那句；号 · 哪一家 · 轮换 · 运行于随别名、不出；
 * 别名预选那台回的那条，没有 ⇒ 空着、［新建］灰到选了为止；改选 ⇒ 问那台新拼的那句，前一半换掉、人打的留着；起的那一份带别名与开口那句、放在 tmux。
 * 替身同 `new-session.vitest.ts`（通道几问 · 账号 · 偏好）。夹具只造结构。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const sent: { op: string; body: Record<string, unknown> }[] = [];
const replies = new Map<string, unknown>();

vi.mock("../../../src/comms/inward/chan", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../../src/comms/inward/chan")>();
  return {
    ...real,
    chan: {
      call: async (_origin: string, op: string, body: Uint8Array) => {
        sent.push({ op, body: JSON.parse(new TextDecoder().decode(body)) });
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
    meta: { effectiveDefault: "work" },
    accounts: [{ name: "work", email: "", configDir: "/h/.cc/work", isDefault: true, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true, selectable: true, newSession: true }],
  })),
}));
vi.mock("../../../src/frontend/ui/acct-center", () => ({ refreshQuota: vi.fn(async () => {}) }));
vi.mock("../../../src/frontend/ui/behavior", () => ({ getBehavior: vi.fn(async () => ({ resumeCommand: "" })) }));
vi.mock("../../../src/frontend/ui/machine-prefs", () => ({ readPrefs: vi.fn(async () => ({ state: "absent", prefs: { launchers: {}, resumeInTmux: false, models: {}, permissionModes: {}, contextLimits: {} } })) }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => {}), listen: vi.fn(async () => () => {}) }));

import { openNewSession, type NewSessionSpec } from "../../../src/frontend/ui/new-session";
import { setLaunchSlots } from "../../../src/frontend/ui/session-open";
import { copyText } from "../../../src/frontend/ui/copy-table";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};
const dialog = (): HTMLElement => document.querySelector<HTMLElement>('[role="dialog"]')!;
const sel = (label: string): HTMLButtonElement => dialog().querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
const choose = (b: HTMLButtonElement, label: string): void => {
  b.click();
  [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((i) => i.textContent?.includes(label))!.click();
};
const rowOf = (el: HTMLElement): HTMLElement => el.closest<HTMLElement>("[class*=nsRow]")!;
const prompt = (): HTMLTextAreaElement => dialog().querySelector<HTMLTextAreaElement>(`textarea[aria-label="${copyText("newSession.label.prompt")}"]`)!;
const createBtn = (): HTMLButtonElement => [...dialog().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("newSession.action.create"))!;
const opened = (): Record<string, unknown>[] => sent.filter((s) => s.op === "session-open").map((s) => s.body);

const FACTS = { recent: [], tmux: true, hosts: ["tmux"], run: "direct", agents: [{ kind: "claude", text: "Claude Code", models: [], modelDefault: null, modelNote: null, permissionModes: [] }], fork: null };
const machineOf = (name: string) => ({
  name, state: "up", offline: false, reason: null, stage: null, version: null, versionRelation: null, os: null,
  word: "", said: "", offlineSaid: `${name} 离线`, fixes: [], seenHostKey: null, expectedHostKey: null, detail: null,
});
const OK = {
  landed: "started", session: "ws-cc", line: "", sid: null, agent: "claude", cwd: "/work/ledger",
  account: null, note: null, prompt: null, arrival: { by: { kind: "cwd", cwd: "/work/ledger" }, since: 5, session: "ws-cc" },
};

const LINE = "读图。\n看 A2-1-3「编码探测」这一格：";
const spec = (alias: string | null, relined: (a: string) => Promise<string> = async (a) => `${a} 的那句\n看 A2-1-3「编码探测」这一格：`): NewSessionSpec => ({
  origin: "devbox",
  lockMachine: true,
  cwd: "/work/ledger",
  lockCwd: true,
  prompt: LINE,
  ask: { title: "问图 · 编码探测", alias, relined },
});

beforeEach(() => {
  document.body.replaceChildren();
  sent.length = 0;
  replies.clear();
  replies.set("machine-read", { machines: { "<local>": machineOf("workstation"), devbox: machineOf("devbox") } });
  replies.set("session-open-facts", FACTS);
  replies.set("session-open-dir", { exists: true, tmuxName: "ws-cc" });
  replies.set("session-open", OK);
  replies.set("session-arrive", { kind: "arrived", sid: "new-sid" });
  replies.set("profiles-read", { rev: "r", sets: [], profiles: [{ name: "look", usable: true }, { name: "dev", usable: true }] });
  setLaunchSlots(null);
});

describe("问图框", () => {
  it("★ 标题是问图那一句 · 别名预选 · 号 · 哪一家 · 运行于不出 · 开口那句是那台拼的；起的那一份带别名与那句、放在 tmux、不点名号", async () => {
    const done = openNewSession(spec("look"));
    await flush();
    expect(dialog().textContent).toContain("问图 · 编码探测");
    expect(sel(copyText("newSession.label.alias")).dataset.value).toBe("look");
    expect(rowOf(sel(copyText("newSession.label.account"))).hidden).toBe(true);
    expect(rowOf(sel(copyText("newSession.label.agent"))).hidden).toBe(true);
    expect(rowOf(dialog().querySelector<HTMLInputElement>('input[type="radio"]')!).hidden).toBe(true);
    expect(prompt().value).toBe(LINE);
    prompt().value = `${LINE}卡在哪`;
    createBtn().click();
    await flush();
    await done;
    const r = opened()[0];
    expect(r.what).toMatchObject({ kind: "new", cwd: "/work/ledger", alias: "look", prompt: `${LINE}卡在哪` });
    expect(r.run).toBe("tmux");
    expect(r.account).toBeUndefined();
  });

  it("没问过图（alias null）⇒ 别名空着、［新建］灰；选了 ⇒ 问那台新拼的那句，前一半换掉、人打的留着，才能起", async () => {
    const asked: string[] = [];
    void openNewSession(spec(null, async (a) => (asked.push(a), `${a} 的那句\n看 A2-1-3「编码探测」这一格：`)));
    await flush();
    expect(sel(copyText("newSession.label.alias")).dataset.value).toBe("");
    expect(createBtn().getAttribute("aria-disabled")).toBe("true");
    prompt().value = `${LINE}为什么还没签`;
    choose(sel(copyText("newSession.label.alias")), "dev");
    await flush();
    expect(asked).toEqual(["dev"]);
    expect(prompt().value).toBe("dev 的那句\n看 A2-1-3「编码探测」这一格：为什么还没签");
    expect(createBtn().getAttribute("aria-disabled")).toBeNull();
  });
});
