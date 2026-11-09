/**
 * 起新会话之后的占位标签页（`launch-slot.ts`）：报到前「正在启动」· 问那台等报到（`session-arrive`，期限 20 s）·
 * 报到了原位换成真的（还看着它 ⇒ 切过去；标签页晚建 ⇒ 建了再切）· 没见到 ⇒「未报到」与那张报错卡（那台那一句原样 · 终端还在才有那三颗）。
 * 期望值是手写字面量；那台的回话与动手那几件换成假的。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), failToast: vi.fn(), undoToast: vi.fn(), recentToasts: vi.fn(() => []) }));

import { failToast, toast } from "../../../src/frontend/ui/kit/toast";
import { LaunchSlots, SLOT_MISS_MS, type SlotActs, type SlotSpec } from "../../../src/frontend/ui/launch-slot";
import type { ArriveSaid } from "../../../src/frontend/ui/session-open";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { endConfirm } from "../../../src/frontend/ui/end-confirm";
import type { TerminalsEndReply } from "../../../src/frontend/ui/generated/TerminalsEndReply";

let tail: HTMLElement;
let root: HTMLElement;
let tabs: Set<string>;
let switched: string[];
let shownLog: boolean[];
let viewed: number;
/** 那台那一问的回话：判据手里放行。 */
let answer: (end: ArriveSaid) => Promise<void>;
let acts: { [K in keyof SlotActs]: ReturnType<typeof vi.fn> };

/** 本机后端合好的一项那一批（`terminals-end`）：那一项的结局 ＋ 那一句。 */
function ended(outcome: "done" | "failed", said: string): TerminalsEndReply {
  const failed = outcome === "failed";
  return {
    results: [{ machine: "devbox", terminal: "%7", outcome, why: failed ? "wrong_owner" : null, said: failed ? "S-wrong_owner" : null, copyDetail: failed ? "d" : "", title: "billing-2" }],
    said,
    failed,
    lines: [],
    copyDetail: failed ? "S-wrong_owner\nd" : "",
  };
}

function make(): LaunchSlots {
  return new LaunchSlots(
    {
      tail,
      root,
      hasTab: (sid) => tabs.has(sid),
      switchTo: (sid) => switched.push(sid),
      shown: (on) => shownLog.push(on),
      viewTerminal: () => viewed++,
    },
    acts as unknown as SlotActs,
  );
}

const ASK = { by: { kind: "cwd", cwd: "/home/u/srv/billing" }, since: 3, session: "billing-2", host: "tmux" } as const;
const SPEC: SlotSpec = { origin: "devbox", cwd: "/home/u/srv/billing", tmuxName: "billing-2", arrival: ASK };
const TITLE = copyText("beSessionArrive.missed.title");
const SAID = `${TITLE}\n已等 18s 未报到 · tmux 会话「billing-2」末几行\nclaude: error: unknown option '--modle'\n(Did you mean --model?)`;

const rows = (): HTMLElement[] => [...tail.querySelectorAll<HTMLElement>("[data-slot]")];
const panelText = (): string => (root.firstElementChild as HTMLElement).textContent ?? "";
const actsOnCard = (): string[] => [...root.querySelectorAll<HTMLElement>("[data-act]")].map((b) => `${b.dataset.act}:${b.textContent}`);
const panelHidden = (): boolean => (root.firstElementChild as HTMLElement).hidden;
const tick = async (): Promise<void> => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

async function missWith(said: string, terminal: string | null): Promise<void> {
  await answer({ kind: "missed", said, terminal });
}

beforeEach(() => {
  document.body.innerHTML = "";
  tail = document.createElement("div");
  root = document.createElement("div");
  document.body.append(tail, root);
  tabs = new Set();
  switched = [];
  shownLog = [];
  viewed = 0;
  const pending: ((e: ArriveSaid) => void)[] = [];
  answer = async (end) => {
    pending.shift()?.(end);
    await tick();
  };
  acts = {
    arrive: vi.fn(() => new Promise<ArriveSaid>((r) => pending.push(r))),
    attach: vi.fn(),
    end: vi.fn(async () => ended("done", "S-done")),
    confirm: vi.fn(async () => true),
  };
  vi.mocked(toast).mockClear();
  vi.mocked(failToast).mockClear();
});

describe("报到之前", () => {
  it("★ 起好了 ⇒ 栏末一行「正在启动」（机器 ＋ 打的那个目录原样、转圈：后端回话之前界面不切路径），主区换成它那一页；问那台等报到（那一问原样 · 20 s）", () => {
    make().add(SPEC);
    expect(rows().map((r) => r.textContent)).toEqual([`devbox/home/u/srv/billing${copyText("launch.placeholder.title")}`]);
    expect(rows()[0].dataset.state).toBe("starting");
    expect(rows()[0].classList.contains("active")).toBe(true);
    expect(panelHidden()).toBe(false);
    expect(panelText()).toBe(`${copyText("launch.placeholder.title")}devbox · /home/u/srv/billing`);
    expect(shownLog).toEqual([true]);
    expect(acts.arrive.mock.calls).toEqual([["devbox", ASK, SLOT_MISS_MS]]);
  });

  it("本机起的不出机器那一枚", () => {
    make().add({ ...SPEC, origin: LOCAL });
    expect(rows()[0].querySelector(".tab-machine")).toBeNull();
  });
});

describe("报到了：原位换成真的", () => {
  it("★ 那台答报到了 ⇒ 这一行摘掉、那一页收起、切到它", async () => {
    make().add(SPEC);
    tabs.add("new-sid");
    await answer({ kind: "arrived", sid: "new-sid" });
    expect(rows()).toEqual([]);
    expect(panelHidden()).toBe(true);
    expect(switched).toEqual(["new-sid"]);
    expect(shownLog).toEqual([true, false]);
  });

  it("★ 回话先到、标签页后建（会话流那一帧晚到）⇒ 建了那一刻切过去；别的标签页建了不切", async () => {
    const slots = make();
    slots.add(SPEC);
    await answer({ kind: "arrived", sid: "new-sid" });
    expect(switched).toEqual([]);
    slots.tabAppeared("other");
    expect(switched).toEqual([]);
    slots.tabAppeared("new-sid");
    slots.tabAppeared("new-sid");
    expect(switched).toEqual(["new-sid"]);
  });

  it("★ 已经切走去看别的标签页了 ⇒ 报到了只换掉那一行，不把人拽过去", async () => {
    const slots = make();
    slots.add(SPEC);
    slots.hide();
    tabs.add("new-sid");
    await answer({ kind: "arrived", sid: "new-sid" });
    slots.tabAppeared("new-sid");
    expect(rows()).toEqual([]);
    expect(switched).toEqual([]);
  });
});

describe("没见到", () => {
  it("★ 那台答没见到 ⇒ 红点「未报到」，那一页一张报错卡：那台那一句（标题 · 那个终端怎样 · 画面末几行）＋ 终端还在 ⇒ 四颗", async () => {
    make().add(SPEC);
    await missWith(SAID, "%7");
    expect(rows()[0].dataset.state).toBe("missed");
    expect(rows()[0].textContent).toBe(`devbox/home/u/srv/billing${copyText("launch.slot.missedTitle")}`);
    expect(panelText()).toBe(
      `${copyText("launch.slot.missedTitle")}devbox · /home/u/srv/billing` +
        TITLE +
        "已等 18s 未报到 · tmux 会话「billing-2」末几行" +
        "claude: error: unknown option '--modle'\n(Did you mean --model?)" +
        `${copyText("launch.slot.again")}${copyText("launch.slot.screen")}${copyText("mobDrawer.item.terminal")}${copyText("tabMenu.kill.label")}${copyText("launch.slot.close")}`,
    );
    expect(root.querySelector("pre")?.textContent).toBe("claude: error: unknown option '--modle'\n(Did you mean --model?)");
    expect(actsOnCard()).toEqual([`again:${copyText("launch.slot.again")}`, `screen:${copyText("launch.slot.screen")}`, `attach:${copyText("mobDrawer.item.terminal")}`, `end:${copyText("tabMenu.kill.label")}`, `close:${copyText("launch.slot.close")}`]);
    expect(vi.mocked(toast), "不再另弹一句").not.toHaveBeenCalled();
  });

  it("本机起的：没有［在终端里打开］（本机没有接回终端那条路）", async () => {
    make().add({ ...SPEC, origin: LOCAL });
    await missWith(SAID, "%7");
    expect(actsOnCard()).toEqual([`again:${copyText("launch.slot.again")}`, `screen:${copyText("launch.slot.screen")}`, `end:${copyText("tabMenu.kill.label")}`, `close:${copyText("launch.slot.close")}`]);
  });

  it("★ 那个终端已不在 / 读不到 / 开窗起的（那台不给句柄）⇒ 只有那一句 ＋［再查一次］［关闭标签页］、不出画面那一块", async () => {
    make().add(SPEC);
    await missWith(`${TITLE}\n已等 18s 未报到 · tmux 会话「billing-2」已不在`, null);
    expect(panelText()).toContain("已等 18s 未报到 · tmux 会话「billing-2」已不在");
    expect(root.querySelector("pre")).toBeNull();
    expect(actsOnCard()).toEqual([`again:${copyText("launch.slot.again")}`, `close:${copyText("launch.slot.close")}`]);
  });

  it("★ 问不到那台（中途断了）⇒ 那一行是「状态不明」不是「未报到」，报错卡那一句 ＋［再查一次］［关闭标签页］（A2-17-8-2）", async () => {
    make().add(SPEC);
    await answer({ kind: "unknown", said: "devbox 无应答", detail: "d" });
    expect(rows()[0].dataset.state).toBe("unknown");
    expect(rows()[0].textContent).toBe(`devbox/home/u/srv/billing${copyText("runs.state.unknown")}`);
    expect(panelText()).toContain("devbox 无应答");
    expect(actsOnCard()).toEqual([`again:${copyText("launch.slot.again")}`, `close:${copyText("launch.slot.close")}`]);
  });

  it("★［再查一次］⇒ 同一问再等一回（那一行回到「正在启动」）；这回报到了 ⇒ 原位换成真的", async () => {
    make().add(SPEC);
    await missWith(SAID, "%7");
    root.querySelector<HTMLButtonElement>('[data-act="again"]')!.click();
    await tick();
    expect(acts.arrive.mock.calls).toEqual([["devbox", ASK, SLOT_MISS_MS], ["devbox", ASK, SLOT_MISS_MS]]);
    expect(rows()[0].dataset.state).toBe("starting");
    tabs.add("s-9");
    await answer({ kind: "arrived", sid: "s-9" });
    expect(rows()).toEqual([]);
    expect(switched).toEqual(["s-9"]);
  });
});

describe("报错卡上的几颗", () => {
  const click = (act: string): void => root.querySelector<HTMLElement>(`[data-act="${act}"]`)!.click();

  it("★［终端画面］开抽屉的终端页、看的就是那台给的那个终端 ·［在终端里打开］接回那个 tmux 会话", async () => {
    const slots = make();
    slots.add(SPEC);
    expect(slots.watched(), "没报到之前不知道那个终端").toBeNull();
    await missWith(SAID, "%7");
    expect(slots.watched()).toEqual({ origin: "devbox", terminal: "%7", name: "billing-2", title: "/home/u/srv/billing" });
    click("screen");
    expect(viewed).toBe(1);
    click("attach");
    expect(acts.attach).toHaveBeenCalledWith("devbox", "billing-2");
    slots.hide();
    expect(slots.watched(), "收起了 ⇒ 抽屉回到当前标签页").toBeNull();
  });

  it("★［结束］开的是结束会话那一个确认框，交本机一项的那一批；回包那一项成了 ⇒ 照摆那一句、这一行关掉", async () => {
    make().add(SPEC);
    await missWith(SAID, "%7");
    click("end");
    await tick();
    expect(acts.confirm).toHaveBeenCalledWith(endConfirm(["billing-2"]));
    expect(acts.end, "按名单里的句柄结束，不按会话名").toHaveBeenCalledWith("devbox", "%7");
    expect(vi.mocked(toast).mock.calls.at(-1)?.[0], "那一句照回包，界面不自己合").toBe("S-done");
    expect(rows()).toEqual([]);
  });

  it("不确认 ⇒ 不动；那台说没成 ⇒ 照摆那一句、这一行留着；问不到本机后端 ⇒ 点名那一句、这一行留着", async () => {
    make().add(SPEC);
    await missWith(SAID, "%7");
    acts.confirm.mockResolvedValueOnce(false);
    click("end");
    await tick();
    expect(acts.end).not.toHaveBeenCalled();
    acts.end.mockResolvedValueOnce(ended("failed", "S-failed"));
    click("end");
    await tick();
    expect(vi.mocked(toast).mock.calls.at(-1)?.[0]).toBe("S-failed");
    expect(vi.mocked(toast).mock.calls.at(-1)?.[2]).toMatchObject({ level: "error", detail: "S-wrong_owner\nd" });
    expect(rows()).toHaveLength(1);
    const down = new Error("本机后端不在");
    acts.end.mockRejectedValueOnce(down);
    click("end");
    await tick();
    expect(vi.mocked(failToast).mock.calls.at(-1)).toEqual([copyText("tabMenu.kill.label"), down]);
    expect(rows()).toHaveLength(1);
  });

  it("［关闭标签页］⇒ 摘掉、那一页收起；关掉之后那台的回话来了也不动", async () => {
    const slots = make();
    const id = slots.add(SPEC);
    slots.close(id);
    expect(rows()).toEqual([]);
    expect(panelHidden()).toBe(true);
    tabs.add("late");
    await answer({ kind: "arrived", sid: "late" });
    expect(switched).toEqual([]);
  });
});
