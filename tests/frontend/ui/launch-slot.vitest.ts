/**
 * 起新会话之后的占位标签页（`launch-slot.ts`）：报到前「正在启动」· 问那台等报到（`session-arrive`，期限 20 s）·
 * 报到了原位换成真的（还看着它 ⇒ 切过去；标签页晚建 ⇒ 建了再切）· 没见到 ⇒「未报到」与那张报错卡（那台那一句原样 · 终端还在才有那三颗）。
 * 期望值是手写字面量；那台的回话与动手那几件换成假的。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), failToast: vi.fn() }));

import { failToast, toast } from "../../../src/frontend/ui/kit/toast";
import { LaunchSlots, SLOT_MISS_MS, type SlotActs, type SlotSpec } from "../../../src/frontend/ui/launch-slot";
import type { ArriveSaid } from "../../../src/frontend/ui/session-open";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";

let tail: HTMLElement;
let root: HTMLElement;
let tabs: Set<string>;
let switched: string[];
let shownLog: boolean[];
let viewed: number;
/** 那台那一问的回话：判据手里放行。 */
let answer: (end: ArriveSaid) => Promise<void>;
let acts: { [K in keyof SlotActs]: ReturnType<typeof vi.fn> };

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

const ASK = { by: { kind: "cwd", cwd: "/home/u/srv/billing" }, since: 3, session: "billing-2" } as const;
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
    kill: vi.fn(async () => {}),
    confirm: vi.fn(async () => true),
  };
  vi.mocked(toast).mockClear();
  vi.mocked(failToast).mockClear();
});

describe("报到之前", () => {
  it("★ 起好了 ⇒ 栏末一行「正在启动」（机器 ＋ 项目名、转圈），主区换成它那一页；问那台等报到（那一问原样 · 20 s）", () => {
    make().add(SPEC);
    expect(rows().map((r) => r.textContent)).toEqual([`devboxbilling${copyText("launch.placeholder.title")}`]);
    expect(rows()[0].dataset.state).toBe("starting");
    expect(rows()[0].classList.contains("active")).toBe(true);
    expect(panelHidden()).toBe(false);
    expect(panelText()).toBe(`${copyText("launch.placeholder.title")}devbox · /home/u/srv/billing`);
    expect(shownLog).toEqual([true]);
    expect(acts.arrive.mock.calls).toEqual([["devbox", ASK, SLOT_MISS_MS]]);
  });

  it("本机起的不出机器那一枚", () => {
    make().add({ ...SPEC, origin: LOCAL_ORIGIN });
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
    expect(rows()[0].textContent).toBe(`devboxbilling${copyText("launch.slot.missedTitle")}`);
    expect(panelText()).toBe(
      `${copyText("launch.slot.missedTitle")}devbox · /home/u/srv/billing` +
        TITLE +
        "已等 18s 未报到 · tmux 会话「billing-2」末几行" +
        "claude: error: unknown option '--modle'\n(Did you mean --model?)" +
        `${copyText("launch.slot.screen")}${copyText("sessionHead.act.openTerm")}${copyText("sessionState.killIdle.action")}${copyText("launch.slot.close")}`,
    );
    expect(root.querySelector("pre")?.textContent).toBe("claude: error: unknown option '--modle'\n(Did you mean --model?)");
    expect(actsOnCard()).toEqual([`screen:${copyText("launch.slot.screen")}`, `attach:${copyText("sessionHead.act.openTerm")}`, `kill:${copyText("sessionState.killIdle.action")}`, `close:${copyText("launch.slot.close")}`]);
    expect(vi.mocked(toast), "不再另弹一句").not.toHaveBeenCalled();
  });

  it("本机起的：没有［在终端里打开］（本机没有接回终端那条路）", async () => {
    make().add({ ...SPEC, origin: LOCAL_ORIGIN });
    await missWith(SAID, "%7");
    expect(actsOnCard()).toEqual([`screen:${copyText("launch.slot.screen")}`, `kill:${copyText("sessionState.killIdle.action")}`, `close:${copyText("launch.slot.close")}`]);
  });

  it("★ 那个终端已不在 / 读不到 / 开窗起的（那台不给句柄）⇒ 只有那一句 ＋［关闭标签页］、不出画面那一块", async () => {
    make().add(SPEC);
    await missWith(`${TITLE}\n已等 18s 未报到 · tmux 会话「billing-2」已不在`, null);
    expect(panelText()).toContain("已等 18s 未报到 · tmux 会话「billing-2」已不在");
    expect(root.querySelector("pre")).toBeNull();
    expect(actsOnCard()).toEqual([`close:${copyText("launch.slot.close")}`]);
  });

  it("问不到那台 ⇒ 那一句当报错卡那一行，只有［关闭标签页］", async () => {
    make().add(SPEC);
    await answer({ kind: "unknown", said: "devbox 无应答", detail: "d" });
    expect(rows()[0].dataset.state).toBe("missed");
    expect(panelText()).toContain("devbox 无应答");
    expect(actsOnCard()).toEqual([`close:${copyText("launch.slot.close")}`]);
  });
});

describe("报错卡上的几颗", () => {
  const click = (act: string): void => root.querySelector<HTMLElement>(`[data-act="${act}"]`)!.click();

  it("★［终端画面］开抽屉的终端页、看的就是那台给的那个终端 ·［在终端里打开］接回那个 tmux 会话", async () => {
    const slots = make();
    slots.add(SPEC);
    expect(slots.watched(), "没报到之前不知道那个终端").toBeNull();
    await missWith(SAID, "%7");
    expect(slots.watched()).toEqual({ origin: "devbox", terminal: "%7", name: "billing-2", title: "billing" });
    click("screen");
    expect(viewed).toBe(1);
    click("attach");
    expect(acts.attach).toHaveBeenCalledWith("devbox", "billing-2");
    slots.hide();
    expect(slots.watched(), "收起了 ⇒ 抽屉回到当前标签页").toBeNull();
  });

  it("★［结束 tmux 会话］先问会打断什么，结束了 ⇒ 这一行关掉", async () => {
    make().add(SPEC);
    await missWith(SAID, "%7");
    click("kill");
    await tick();
    expect(acts.confirm).toHaveBeenCalledWith({
      title: copyText("sessionState.killIdle.title", { name: "billing-2" }),
      action: copyText("sessionState.killIdle.action"),
      danger: true,
      rows: [{ label: copyText("kit.interrupts.cut"), items: [copyText("launch.slot.killCuts", { machine: "devbox", name: "billing-2" })] }],
    });
    expect(acts.kill).toHaveBeenCalledWith("devbox", "billing-2");
    expect(rows()).toEqual([]);
  });

  it("不确认 ⇒ 不动；结束失败 ⇒ 说一句、这一行留着", async () => {
    make().add(SPEC);
    await missWith(SAID, "%7");
    acts.confirm.mockResolvedValueOnce(false);
    click("kill");
    await tick();
    expect(acts.kill).not.toHaveBeenCalled();
    acts.kill.mockRejectedValueOnce(new Error("不在名单"));
    click("kill");
    await tick();
    expect(vi.mocked(failToast).mock.calls.map((c) => [c[0], String(c[1])])).toEqual([[copyText("tabSessionActions.kill.failed", { title: "billing-2" }), "Error: 不在名单"]]);
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
