/**
 * `src/frontend/ui/resume-menu.ts` 的判据：「恢复」那一组选项（标签页「恢复 ▸」与历史页「恢复 ▾」同一个组件）。
 *
 * 1. 账号组：那台可选的号 ＋ 基座；上次的号标「上次」，跟随时勾在它上面；每个号后面 5h 那一格读额度账的显示态。
 * 2. 这一家没有账号这一维 ⇒ 账号组灰着一行；那台没开多账号 ⇒ 账号组不出。
 * 3. 点单选只改勾着的那一组（菜单开关归 kit）；有 `run` ⇒ 最上面一行「恢复」按勾着的那一组起。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../src/frontend/ui/account-reads", () => ({ fetchAccounts: vi.fn() }));

import { defaultPick, pickAsk, resumeAccounts, resumeHint, resumeMenuItems, type ResumeAccounts, type ResumePick } from "../../../src/frontend/ui/resume-menu";
import { fetchAccounts } from "../../../src/frontend/ui/account-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";

/** 那台 `accounts-list` 的一行：号名（`label`）与额度格（`slots`）都是那台写好的。 */
const acct = (name: string, selectable = true, over: Record<string, unknown> = {}): Record<string, unknown> => ({
  name,
  label: name,
  email: `${name}@x`,
  configDir: `/h/${name}`,
  isDefault: false,
  mode: selectable ? "isolated" : "shared",
  exists: true,
  loggedIn: true,
  authKind: "subscription",
  authReady: true,
  selectable,
  slots: [],
  ...over,
});

beforeEach(() => {
  vi.mocked(fetchAccounts).mockReset();
});

describe("账号组的事实", () => {
  it("★ 那台可选的号 ＋ 基座；号名与号旁那一格照那一行（label · 第一格）；上次的号标出来；基座不带格", async () => {
    vi.mocked(fetchAccounts).mockResolvedValue({
      available: true,
      accounts: [acct("work", true, { label: "后端·work", slots: [{ slot: "5h", text: "后端·5h 41% ↻13:00", tone: "plain" }, { slot: "7d", text: "后端·7d", tone: "plain" }] }), acct("home"), acct("old", false)],
    } as never);
    const got = await resumeAccounts("devbox", "claude", { hasAccounts: true, agentName: "claude", last: "work" });
    expect(got).toEqual({
      kind: "list",
      items: [
        { name: "work", label: "后端·work", quota: "后端·5h 41% ↻13:00", last: true },
        { name: "home", label: "home", quota: null, last: false },
        { name: null, label: copyText("resumeMenu.account.base"), quota: null, last: false },
      ],
      origin: "devbox",
    });
  });

  it("只有一个可选号也照出（不整组藏）", async () => {
    vi.mocked(fetchAccounts).mockResolvedValue({ available: true, accounts: [acct("solo", true, { slots: [{ slot: "5h", text: "后端·5h 7%", tone: "plain" }] })] } as never);
    const got = await resumeAccounts("dev", "claude", { hasAccounts: true, agentName: "claude" });
    expect(got.kind === "list" && got.items.map((a) => [a.name, a.quota])).toEqual([
      ["solo", "后端·5h 7%"],
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
  it("账号 · 运行于两组单选 ＋ 在此目录新建会话；跟随时勾在上次的号上，运行于勾在那台答的默认上", () => {
    const items = resumeMenuItems({ accounts: LIST, pick: defaultPick(), machineRun: "direct", newInDir: () => {} });
    expect(shape(items)).toEqual([
      `[${copyText("beProfile.slot.account")}]`,
      `✓work|${copyText("resumeMenu.account.quotaLast", { quota: "5h 41%" })}`,
      "home",
      copyText("resumeMenu.account.base"),
      "—",
      `[${copyText("resumeMenu.group.run")}]`,
      copyText("resumeMenu.run.managed"),
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
    items[7].onClick?.();
    expect(pick).toEqual({ run: "tmux", account: "home", useBase: false });
    items[3].onClick?.();
    expect(pick).toEqual({ run: "tmux", account: undefined, useBase: true });
    expect(seen.length).toBe(3);
    expect(resumeHint(LIST, pick)).toBe(
      copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.pick.account", { account: copyText("resumeMenu.account.base"), run: copyText("resumeMenu.hint.tmux") }) }),
    );
  });

  it("有 run ⇒ 最上面一行「恢复」，右侧灰字是勾着的那一组，点了按它起", () => {
    const pick: ResumePick = { run: "tmux", account: "home", useBase: false };
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
    const off = resumeMenuItems({ accounts: { kind: "off" }, pick: defaultPick(), machineRun: "tmux" });
    expect(shape(off)).toEqual([`[${copyText("resumeMenu.group.run")}]`, copyText("resumeMenu.run.managed"), `✓${copyText("resumeMenu.run.tmux")}`, copyText("resumeMenu.run.direct")]);
    expect(resumeHint({ kind: "off" }, defaultPick(), "direct")).toBe(copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.run.direct") }));
  });

  it("★ 运行于没动过、也没问到那台 ⇒ 两项都不勾、悬停写照机器设置；请求里不带号也不带放在哪", () => {
    const pick = defaultPick();
    const items = resumeMenuItems({ accounts: { kind: "off" }, pick });
    expect(shape(items)).toEqual([`[${copyText("resumeMenu.group.run")}]`, copyText("resumeMenu.run.managed"), copyText("resumeMenu.run.tmux"), copyText("resumeMenu.run.direct")]);
    expect(resumeHint({ kind: "off" }, pick)).toBe(copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.run.machine") }));
    expect(pickAsk(pick)).toEqual({});
    items[2].onClick?.();
    expect(pickAsk(pick)).toEqual({ run: "tmux" });
    items[1].onClick?.();
    expect(pickAsk(pick)).toEqual({ run: "managed" });
    // 那台答缺省放在托管 ⇒ 勾托管那一项，悬停照它说。
    const hosted = resumeMenuItems({ accounts: { kind: "off" }, pick: defaultPick(), machineRun: "managed" });
    expect(shape(hosted)[1]).toBe(`✓${copyText("resumeMenu.run.managed")}`);
    expect(resumeHint({ kind: "off" }, defaultPick(), "managed")).toBe(copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.hint.managed") }));
    expect(pickAsk({ run: null, account: "w", useBase: false })).toEqual({ account: { kind: "named", name: "w" } });
    expect(pickAsk({ run: "direct", account: undefined, useBase: true })).toEqual({ account: { kind: "base" }, run: "direct" });
  });
});
