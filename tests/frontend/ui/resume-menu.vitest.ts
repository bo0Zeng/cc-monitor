/**
 * `src/frontend/ui/resume-menu.ts` 的判据：「恢复」那一组选项（标签页「恢复 ▸」与历史页「恢复 ▾」同一个组件）。
 *
 * 1. 账号组：那台可选的号 ＋ 基座；上次的号标「上次」，跟随时勾在它上面；每个号后面 5h 那一格读额度账的显示态。
 * 2. 这一家没有账号这一维 ⇒ 账号组灰着一行；那台没开多账号 ⇒ 账号组不出。
 * 3. 点单选只改勾着的那一组（菜单开关归 kit）；有 `run` ⇒ 最上面一行「恢复」按勾着的那一组起。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../src/frontend/ui/account-reads", () => ({ fetchAccounts: vi.fn() }));
vi.mock("../../../src/frontend/ui/acct-center", () => ({ refreshQuota: vi.fn() }));

import { defaultPick, resumeAccounts, resumeHint, resumeMenuItems, type ResumeAccounts, type ResumePick } from "../../../src/frontend/ui/resume-menu";
import { fetchAccounts } from "../../../src/frontend/ui/account-reads";
import { refreshQuota } from "../../../src/frontend/ui/acct-center";
import { appStore, putIn } from "../../../src/frontend/ui/app-store";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { QuotaRead } from "../../../src/frontend/ui/acct-words";

const acct = (name: string, selectable = true): Record<string, unknown> => ({
  name,
  email: `${name}@x`,
  configDir: `/h/${name}`,
  isDefault: false,
  mode: selectable ? "isolated" : "shared",
  exists: true,
  loggedIn: true,
  authKind: "subscription",
  authReady: true,
  selectable,
});

const quota = (accounts: { account: string; pct?: number; state?: string; kind?: string }[]): QuotaRead =>
  ({
    state: "present",
    reason: null,
    path: null,
    now: 1_800_000_000,
    accounts: accounts.map((a) => ({
      agent: "claude-code",
      account: a.account,
      seenAt: 1_800_000_000,
      kind: a.kind ?? "sub",
      state: a.state ?? "ok",
      stale: false,
      limiting: "5h",
      // 那一格的字是核心写好的（被拒没标用满 ⇒「{pct}% · 被拒」），界面照抄。
      slots: a.pct === undefined ? [] : [{ slot: "5h", pct: a.pct, text: a.state === "refused" ? copyText("acct.val.refusedPct", { pct: a.pct }) : `${a.pct}%`, tone: "plain" }],
      login: "ok",
    })),
    unseen: [],
    usableNow: [],
    earliestReturn: null,
  }) as unknown as QuotaRead;

beforeEach(() => {
  vi.mocked(fetchAccounts).mockReset();
  vi.mocked(refreshQuota).mockReset();
  appStore.quota.set(new Map());
});

describe("账号组的事实", () => {
  it("那台可选的号 ＋ 基座；上次的号标出来；5h 读额度账（被拒照显示态换字；没出过数 ⇒ 不出）", async () => {
    vi.mocked(fetchAccounts).mockResolvedValue({ available: true, accounts: [acct("work"), acct("home"), acct("old", false)] } as never);
    putIn(appStore.quota, "devbox", quota([{ account: "work", pct: 41 }, { account: "_", state: "refused", pct: 100 }]));
    const got = await resumeAccounts("devbox", "claude", { hasAccounts: true, agentName: "claude", last: "work" });
    expect(got).toEqual({
      kind: "list",
      items: [
        { name: "work", label: "work", quota: "5h 41%", last: true },
        { name: "home", label: "home", quota: null, last: false },
        { name: null, label: copyText("resumeMenu.account.base"), quota: `5h ${copyText("acct.val.refusedPct", { pct: 100 })}`, last: false },
      ],
    });
    expect(refreshQuota, "那台的额度账已在 ⇒ 不再问").not.toHaveBeenCalled();
  });

  it("那台额度账还没读过 ⇒ 先读一次；只有一个可选号也照出（不整组藏）", async () => {
    vi.mocked(fetchAccounts).mockResolvedValue({ available: true, accounts: [acct("solo")] } as never);
    vi.mocked(refreshQuota).mockImplementation(async (o) => putIn(appStore.quota, o, quota([{ account: "solo", pct: 7 }])));
    const got = await resumeAccounts("dev", "claude", { hasAccounts: true, agentName: "claude" });
    expect(refreshQuota).toHaveBeenCalledWith("dev");
    expect(got.kind === "list" && got.items.map((a) => [a.name, a.quota])).toEqual([
      ["solo", "5h 7%"],
      [null, null],
    ]);
  });

  it("这一家没有账号这一维 ⇒ none（不问那台）；没开多账号 / 一个可选号也没有 / 问不到 ⇒ off", async () => {
    expect(await resumeAccounts("devbox", "codex", { hasAccounts: false, agentName: "Codex" })).toEqual({ kind: "none", agentName: "Codex" });
    expect(fetchAccounts).not.toHaveBeenCalled();
    vi.mocked(fetchAccounts).mockResolvedValueOnce({ available: false, accounts: [] } as never);
    expect((await resumeAccounts("devbox", "claude", { hasAccounts: true, agentName: "claude" })).kind).toBe("off");
    vi.mocked(fetchAccounts).mockResolvedValueOnce({ available: true, accounts: [acct("x", false)] } as never);
    expect((await resumeAccounts("devbox", "claude", { hasAccounts: true, agentName: "claude" })).kind).toBe("off");
    vi.mocked(fetchAccounts).mockRejectedValueOnce(new Error("断了"));
    expect((await resumeAccounts("devbox", "claude", { hasAccounts: true, agentName: "claude" })).kind).toBe("off");
  });
});

const LIST: ResumeAccounts = {
  kind: "list",
  items: [
    { name: "work", label: "work", quota: "5h 41%", last: true },
    { name: "home", label: "home", quota: null, last: false },
    { name: null, label: copyText("resumeMenu.account.base"), quota: null, last: false },
  ],
};
const shape = (items: ReturnType<typeof resumeMenuItems>): string[] =>
  items.map((i) => (i.divider ? "—" : i.heading ? `[${i.label}]` : `${i.checked ? "✓" : ""}${i.label}${i.detail ? `|${i.detail}` : ""}`));

describe("菜单怎么摆", () => {
  it("账号 · 运行于两组单选 ＋ 在此目录新建会话；跟随时勾在上次的号上，不用 tmux 勾着", () => {
    const items = resumeMenuItems({ accounts: LIST, pick: defaultPick(), newInDir: () => {} });
    expect(shape(items)).toEqual([
      `[${copyText("resumeMenu.group.account")}]`,
      `✓work|${copyText("resumeMenu.account.quotaLast", { quota: "5h 41%" })}`,
      "home",
      copyText("resumeMenu.account.base"),
      "—",
      `[${copyText("resumeMenu.group.run")}]`,
      copyText("resumeMenu.run.tmux"),
      `✓${copyText("resumeMenu.run.direct")}`,
      "—",
      copyText("history.menu.newInDir"),
    ]);
    expect(items.filter((i) => i.radio === "account").length).toBe(3);
  });

  it("点单选只改勾着的那一组、报一声；基座交 useBase", () => {
    const pick = defaultPick();
    const seen: ResumePick[] = [];
    const items = resumeMenuItems({ accounts: LIST, pick, onChange: (p) => seen.push({ ...p }) });
    items[2].onClick?.();
    items[6].onClick?.();
    expect(pick).toEqual({ tmux: true, account: "home", useBase: false });
    items[3].onClick?.();
    expect(pick).toEqual({ tmux: true, account: undefined, useBase: true });
    expect(seen.length).toBe(3);
    expect(resumeHint(LIST, pick)).toBe(
      copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.pick.account", { account: copyText("resumeMenu.account.base"), run: copyText("resumeMenu.hint.tmux") }) }),
    );
  });

  it("有 run ⇒ 最上面一行「恢复」，右侧灰字是勾着的那一组，点了按它起", () => {
    const pick: ResumePick = { tmux: true, account: "home", useBase: false };
    const run = vi.fn();
    const items = resumeMenuItems({ accounts: LIST, pick, run });
    expect(items[0].label).toBe(copyText("history.row.resume"));
    expect(items[0].detailOf?.()).toBe(copyText("resumeMenu.pick.account", { account: "home", run: copyText("resumeMenu.hint.tmux") }));
    expect(items[1].divider).toBe(true);
    items[0].onClick?.();
    expect(run).toHaveBeenCalledWith(pick);
  });

  it("没有账号这一维 ⇒ 账号组灰一行「Codex · 无账号维」；没开多账号 ⇒ 账号组不出、悬停只说方式", () => {
    const none = resumeMenuItems({ accounts: { kind: "none", agentName: "Codex" }, pick: defaultPick() });
    expect(none[1]).toMatchObject({ label: copyText("history.resume.noAccounts", { agent: "Codex" }), enabled: false });
    const off = resumeMenuItems({ accounts: { kind: "off" }, pick: defaultPick() });
    expect(shape(off)).toEqual([`[${copyText("resumeMenu.group.run")}]`, copyText("resumeMenu.run.tmux"), `✓${copyText("resumeMenu.run.direct")}`]);
    expect(resumeHint({ kind: "off" }, defaultPick())).toBe(copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.run.direct") }));
  });
});
