// 栏顶组菜单 · 命令面板「起一组…」：项照 set-plan 成品摆（再起 · 已齐 · 写错 · 答不了，几句都是那台写好的），
// 点一项发 set-launch，起成的交等报到，起完照那台那一句说。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const h = vi.hoisted(() => ({
  readProfiles: vi.fn(),
  planSet: vi.fn(),
  launchSet: vi.fn(),
  awaitArrival: vi.fn(),
  toast: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/profiles-reads", () => ({ readProfiles: h.readProfiles }));
vi.mock("../../../src/frontend/ui/set-reads", () => ({ planSet: h.planSet, launchSet: h.launchSet }));
vi.mock("../../../src/frontend/ui/session-open", () => ({ awaitArrival: h.awaitArrival }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: h.toast }));

import { copyText } from "../../../src/frontend/ui/copy-table";
import { newSessionSplit, setItem, startSetCommand, type SetMenuHost } from "../../../src/frontend/ui/set-menu";
import { startSet } from "../../../src/frontend/ui/set-start";
import type { SetOut } from "../../../src/frontend/ui/generated/SetOut";
import type { SetPlan } from "../../../src/frontend/ui/generated/SetPlan";
import type { SetLaunched } from "../../../src/frontend/ui/generated/SetLaunched";
import type { Opened } from "../../../src/frontend/ui/generated/Opened";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 16; i += 1) await Promise.resolve();
};

const set = (name: string, problems: SetOut["problems"] = []): SetOut => ({ name, rows: [], problems });

/** 那台写好的几句用占位字：界面照摆，不拼。 */
const plan = (more: number): SetPlan => ({
  rows: [
    { cwd: "~/work/cc-bus", alias: "look", want: 1, have: 0, more, group: "看图", blocked: null, said: "ROW-1", haveSaid: "HAVE", moreSaid: "RMORE", canMore: true, moreWhy: null },
    { cwd: "~/work/notes", alias: "look", want: 1, have: 1, more: 0, group: "看图", blocked: null, said: "ROW-2", haveSaid: "HAVE", moreSaid: "RMORE", canMore: true, moreWhy: null },
  ],
  more,
  short: "SHORT",
  moreSaid: "MORE-SAID",
  totalSaid: "TOTAL",
  startSaid: "START",
});

const opened = (cwd: string): Opened => ({
  landed: "started",
  session: "t",
  line: null,
  sid: "s1",
  agent: null,
  cwd,
  account: null,
  note: null,
  prompt: null,
  arrival: { by: { kind: "sid", sid: "s1" }, since: 1, session: "t" } as Opened["arrival"],
});

const host = (): SetMenuHost => ({ newSession: vi.fn(), editSets: vi.fn() });

beforeEach(() => {
  for (const f of Object.values(h)) f.mockReset();
});

afterEach(() => {
  document.body.replaceChildren();
  delete document.body.dataset.tabBar;
});

describe("组菜单那一项", () => {
  it("★ 能起的：右侧照那台那一句、第二行恒写那台的短句（不按行铺）；逐行在悬停侧卡里；点了起", () => {
    const start = vi.fn();
    const it = setItem(set("工具链"), { kind: "plan", plan: plan(1) }, start);
    expect([it.label, it.detail, it.body, it.enabled]).toEqual(["工具链", "MORE-SAID", "SHORT", undefined]);
    const card = it.peek!();
    expect(Array.from(card.querySelectorAll('[data-role="set-peek-row"]'), (e) => e.textContent)).toEqual(["ROW-1", "ROW-2"]);
    it.onClick!();
    expect(start).toHaveBeenCalledTimes(1);
  });

  it("已齐的灰着、右侧照那一句；写错的 · 答不了的灰着、写为什么；还在问的转圈", () => {
    const full = setItem(set("周报"), { kind: "plan", plan: plan(0) }, vi.fn());
    expect([full.enabled, full.detail, full.why, full.onClick]).toEqual([false, "MORE-SAID", "SHORT", undefined]);
    const broken = setItem(set("坏", [{ section: null, line: 3, message: "行 3 写错", detail: null } as SetOut["problems"][number]]), { kind: "plan", plan: plan(1) }, vi.fn());
    expect([broken.enabled, broken.why]).toEqual([false, "行 3 写错"]);
    const failed = setItem(set("远"), { kind: "failed", said: "devbox 离线" }, vi.fn());
    expect([failed.enabled, failed.why]).toEqual([false, "devbox 离线"]);
    expect(setItem(set("等"), { kind: "pending" }, vi.fn()).pending).toBe(true);
  });
});

describe("起一组", () => {
  const launched = (failed: number): SetLaunched => ({
    rows: [
      { cwd: "/w/a", alias: "dev", group: { id: "g", name: "开发" }, opened: [opened("/w/a")], failed: [], blocked: null },
      { cwd: "/w/b", alias: "look", group: null, opened: [], failed: failed ? [{ code: "account_unavailable", message: "b 需登录" }] : [], blocked: null },
    ],
    started: 1,
    failed,
    said: "SAID",
    detail: failed ? "DETAIL" : "",
  });

  it("发 set-launch {name}；起成的交等报到；全成照那台那一句说（标题是组名）", async () => {
    h.launchSet.mockResolvedValue(launched(0));
    await startSet("local", "工具链");
    expect(h.launchSet).toHaveBeenCalledWith("local", "工具链");
    expect(h.awaitArrival).toHaveBeenCalledTimes(1);
    expect(h.awaitArrival.mock.calls[0][2]).toBe("/w/a");
    expect(h.toast).toHaveBeenCalledWith("工具链", "SAID", { level: "info" });
  });

  it("有没成的：那一句 ＋ 逐条原因进明细 ＋［重试］（再点一次 ＝ 只起还缺的）", async () => {
    h.launchSet.mockResolvedValue(launched(1));
    await startSet("local", "工具链");
    const [title, body, opts] = h.toast.mock.calls[0];
    expect([title, body, opts.more]).toEqual(["工具链", "SAID", ["DETAIL"]]);
    expect(opts.action.label).toBe(copyText("setMenu.launched.retry"));
    h.launchSet.mockResolvedValue(launched(0));
    opts.action.run();
    await flush();
    expect(h.launchSet).toHaveBeenCalledTimes(2);
  });

  it("那台拒了（没 tmux · 没这一组）⇒ 那一句原样说", async () => {
    h.launchSet.mockRejectedValue(new Error("这台没有 tmux"));
    await startSet("local", "工具链");
    expect(h.toast.mock.calls[0][0]).toBe("这台没有 tmux");
  });
});

describe("栏顶拆分按钮 · 命令面板那一条", () => {
  it("「＋」开新建框；「⌄」开组菜单：新建会话 · 这台的几组（各问一次 set-plan，答回来就地换上）· 编辑组…", async () => {
    h.readProfiles.mockResolvedValue({ sets: [set("工具链")] });
    h.planSet.mockResolvedValue(plan(1));
    const hs = host();
    const el = newSessionSplit(() => "local", hs);
    document.body.appendChild(el);
    el.querySelector<HTMLButtonElement>('[data-role="new-session"]')!.click();
    expect(hs.newSession).toHaveBeenCalledTimes(1);
    el.querySelector<HTMLButtonElement>('[data-role="set-menu"]')!.click();
    await flush();
    expect(h.planSet).toHaveBeenCalledWith("local", { name: "工具链" });
    const text = document.body.textContent ?? "";
    expect(text).toContain(copyText("main.cmd.newSession"));
    expect(text).toContain("工具链");
    expect(text).toContain("MORE-SAID");
    expect(text).toContain(copyText("setMenu.list.edit"));
  });

  it("窄栏里「＋」也开新建框（组菜单只在「⌄」）", async () => {
    h.readProfiles.mockResolvedValue({ sets: [] });
    document.body.dataset.tabBar = "folded";
    const hs = host();
    const el = newSessionSplit(() => "local", hs);
    document.body.appendChild(el);
    el.querySelector<HTMLButtonElement>('[data-role="new-session"]')!.click();
    await flush();
    expect(hs.newSession).toHaveBeenCalledTimes(1);
    expect(document.body.querySelector("[role=menu]")).toBeNull();
    delete document.body.dataset.tabBar;
  });

  it("命令面板「起一组…」开的是同一张组菜单", async () => {
    h.readProfiles.mockResolvedValue({ sets: [set("工具链")] });
    h.planSet.mockResolvedValue(plan(0));
    const anchor = document.createElement("button");
    document.body.appendChild(anchor);
    const c = startSetCommand(() => anchor, () => "devbox", host());
    expect(c.title).toBe(copyText("main.cmd.startSet"));
    c.run();
    await flush();
    expect(h.readProfiles).toHaveBeenCalledWith("devbox");
    expect(document.body.textContent).toContain("MORE-SAID");
  });
});
