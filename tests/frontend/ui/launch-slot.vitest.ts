/**
 * 起新会话之后的占位标签页（`launch-slot.ts`）：报到前「正在启动」· 报到了原位换成真的（还看着它 ⇒ 切过去）·
 * 20 s 没报到 ⇒「未报到」与那张报错卡（tmux 会话在不在 · 画面末几行 · 四颗 / 一颗按钮）· 到点不放弃。
 * 期望值是手写字面量；那台的两问与动手那几件换成假的。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), failToast: vi.fn() }));

import { failToast, toast } from "../../../src/frontend/ui/kit/toast";
import { __pendingCountForTests, __resetArrivalsForTests, noteLive } from "../../../src/frontend/ui/launch-arrival";
import { LaunchSlots, SLOT_MISS_MS, type SlotActs, type SlotScreen, type SlotSpec } from "../../../src/frontend/ui/launch-slot";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";

let tail: HTMLElement;
let root: HTMLElement;
let tabs: Set<string>;
let switched: string[];
let shownLog: boolean[];
let screen: SlotScreen | Error;
let acts: { [K in keyof SlotActs]: ReturnType<typeof vi.fn> };

function make(): LaunchSlots {
  return new LaunchSlots(
    {
      tail,
      root,
      hasTab: (sid) => tabs.has(sid),
      switchTo: (sid) => switched.push(sid),
      shown: (on) => shownLog.push(on),
    },
    acts as unknown as SlotActs,
  );
}

const SPEC: SlotSpec = { origin: "devbox", cwd: "/home/u/srv/billing", tmuxName: "billing-2", agent: "claude", match: { cwd: "/home/u/srv/billing" } };

const rows = (): HTMLElement[] => [...tail.querySelectorAll<HTMLElement>("[data-slot]")];
const panelText = (): string => (root.firstElementChild as HTMLElement).textContent ?? "";
const actsOnCard = (): string[] => [...root.querySelectorAll<HTMLElement>("[data-act]")].map((b) => `${b.dataset.act}:${b.textContent}`);
const panelHidden = (): boolean => (root.firstElementChild as HTMLElement).hidden;

async function missNow(): Promise<void> {
  await vi.advanceTimersByTimeAsync(SLOT_MISS_MS);
}

beforeEach(() => {
  vi.useFakeTimers();
  __resetArrivalsForTests();
  document.body.innerHTML = "";
  tail = document.createElement("div");
  root = document.createElement("div");
  document.body.append(tail, root);
  tabs = new Set();
  switched = [];
  shownLog = [];
  screen = { kind: "there", terminal: "%7", words: "claude: error: unknown option '--modle'\n(Did you mean --model?)\n$" };
  acts = {
    screen: vi.fn(async () => {
      if (screen instanceof Error) throw screen;
      return screen;
    }),
    preview: vi.fn(),
    attach: vi.fn(),
    kill: vi.fn(async () => {}),
    confirm: vi.fn(async () => true),
  };
  vi.mocked(toast).mockClear();
  vi.mocked(failToast).mockClear();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("报到之前", () => {
  it("★ 起好了 ⇒ 栏末一行「正在启动」（机器 ＋ 项目名、转圈），主区换成它那一页、会话头让开", () => {
    const slots = make();
    slots.add(SPEC);
    expect(rows().map((r) => r.textContent)).toEqual([`devboxbilling${copyText("launch.placeholder.title")}`]);
    expect(rows()[0].dataset.state).toBe("starting");
    expect(rows()[0].classList.contains("active")).toBe(true);
    expect(panelHidden()).toBe(false);
    expect(panelText()).toBe(`${copyText("launch.placeholder.title")}devbox · /home/u/srv/billing`);
    expect(shownLog).toEqual([true]);
  });

  it("本机起的不出机器那一枚", () => {
    make().add({ ...SPEC, origin: LOCAL_ORIGIN });
    expect(rows()[0].querySelector(".tab-machine")).toBeNull();
  });
});

describe("报到了：原位换成真的", () => {
  it("★ 那台报出同目录的新会话 ⇒ 这一行摘掉、那一页收起、切到它", () => {
    const slots = make();
    slots.add(SPEC);
    tabs.add("new-sid");
    noteLive("devbox", "new-sid", { cwd: "/home/u/srv/billing" });
    expect(rows()).toEqual([]);
    expect(panelHidden()).toBe(true);
    expect(switched).toEqual(["new-sid"]);
    expect(shownLog).toEqual([true, false]);
  });

  it("别的机器 / 别的目录报出的不算", () => {
    make().add(SPEC);
    noteLive("gpu-01", "x", { cwd: "/home/u/srv/billing" });
    noteLive("devbox", "y", { cwd: "/home/u/srv/orders" });
    expect(rows()).toHaveLength(1);
    expect(switched).toEqual([]);
  });

  it("远端先报到、标签页同一拍后建 ⇒ 等它在了再切", async () => {
    make().add(SPEC);
    noteLive("devbox", "new-sid", { cwd: "/home/u/srv/billing" });
    tabs.add("new-sid");
    await Promise.resolve();
    expect(switched).toEqual(["new-sid"]);
  });

  it("★ 已经切走去看别的标签页了 ⇒ 报到了只换掉那一行，不把人拽过去", () => {
    const slots = make();
    slots.add(SPEC);
    slots.hide();
    tabs.add("new-sid");
    noteLive("devbox", "new-sid", { cwd: "/home/u/srv/billing" });
    expect(rows()).toEqual([]);
    expect(switched).toEqual([]);
  });

  it("分叉那一形按 sid 认", () => {
    make().add({ ...SPEC, match: { sid: "fork-1" } });
    tabs.add("fork-1");
    noteLive("devbox", "other", { cwd: "/home/u/srv/billing" });
    expect(rows()).toHaveLength(1);
    noteLive("devbox", "fork-1", { cwd: "/elsewhere" });
    expect(switched).toEqual(["fork-1"]);
  });
});

describe("20 s 没报到", () => {
  it("★ 到点之前一直是「正在启动」；到点 ⇒ 红点「未报到」，那一页一张报错卡：tmux 会话在 ＋ 画面末几行 ＋ 四颗按钮", async () => {
    make().add(SPEC);
    await vi.advanceTimersByTimeAsync(SLOT_MISS_MS - 1);
    expect(rows()[0].dataset.state).toBe("starting");
    await vi.advanceTimersByTimeAsync(1);
    expect(acts.screen).toHaveBeenCalledWith("devbox", "billing-2");
    expect(rows()[0].dataset.state).toBe("missed");
    expect(rows()[0].textContent).toBe(`devboxbilling${copyText("launch.slot.missedTitle")}`);
    expect(panelText()).toBe(
      `${copyText("launch.slot.missedTitle")}devbox · /home/u/srv/billing${copyText("launch.slot.missedTitle")} · 20s` +
        copyText("launch.slot.missedCard", { secs: 20 }) + copyText("launch.slot.tmuxThere", { machine: "devbox", name: "billing-2" }) +
        "claude: error: unknown option '--modle'\n(Did you mean --model?)\n$" +
        `${copyText("launch.slot.screen")}${copyText("sessionHead.act.openTerm")}${copyText("sessionState.killIdle.action")}${copyText("launch.slot.close")}`,
    );
    expect(actsOnCard()).toEqual([`screen:${copyText("launch.slot.screen")}`, `attach:${copyText("sessionHead.act.openTerm")}`, `kill:${copyText("sessionState.killIdle.action")}`, `close:${copyText("launch.slot.close")}`]);
    expect(vi.mocked(toast), "不再另弹「没看到会话起来」").not.toHaveBeenCalled();
  });

  it("★ 到点不放弃：之后又报到了 ⇒ 照样换成真的，报错卡消失", async () => {
    make().add(SPEC);
    await missNow();
    tabs.add("late");
    noteLive("devbox", "late", { cwd: "/home/u/srv/billing" });
    expect(rows()).toEqual([]);
    expect(panelHidden()).toBe(true);
    expect(switched).toEqual(["late"]);
  });

  it("本机起的：没有［在终端里打开］（本机没有接回终端那条路）", async () => {
    make().add({ ...SPEC, origin: LOCAL_ORIGIN });
    await missNow();
    expect(actsOnCard()).toEqual([`screen:${copyText("launch.slot.screen")}`, `kill:${copyText("sessionState.killIdle.action")}`, `close:${copyText("launch.slot.close")}`]);
  });

  it("tmux 会话已经不在 ⇒ 说不在，只剩［关闭标签页］", async () => {
    screen = { kind: "gone" };
    make().add(SPEC);
    await missNow();
    expect(panelText()).toContain(copyText("launch.slot.tmuxGone", { machine: "devbox", name: "billing-2" }));
    expect(actsOnCard()).toEqual([`close:${copyText("launch.slot.close")}`]);
  });

  it("画面读不到 ⇒ 照说原因；画面是空的 ⇒ 说空、不出那一块", async () => {
    screen = new Error("那台无应答");
    make().add(SPEC);
    await missNow();
    expect(panelText()).toContain(copyText("launch.slot.noScreen", { machine: "devbox", name: "billing-2", why: "那台无应答" }));
    expect(actsOnCard()).toEqual([`close:${copyText("launch.slot.close")}`]);

    screen = { kind: "there", terminal: "%7", words: "" };
    make().add({ ...SPEC, cwd: "/home/u/b2" });
    await missNow();
    expect(root.lastElementChild?.textContent).toContain(copyText("launch.slot.tmuxEmpty", { machine: "devbox", name: "billing-2" }));
    expect(root.lastElementChild?.querySelector("pre")).toBeNull();
  });

  it("★ 开窗起的：只有那一句 ＋［关闭标签页］，不去问 tmux", async () => {
    make().add({ ...SPEC, tmuxName: null });
    await missNow();
    expect(acts.screen).not.toHaveBeenCalled();
    expect(panelText()).toBe(`${copyText("launch.slot.missedTitle")}devbox · /home/u/srv/billing${copyText("launch.slot.missedState", { secs: 20 })}${copyText("launch.slot.missedCard", { secs: 20 })}${copyText("launch.slot.close")}`);
    expect(actsOnCard()).toEqual([`close:${copyText("launch.slot.close")}`]);
  });
});

describe("报错卡上的几颗", () => {
  const click = (act: string): void => root.querySelector<HTMLElement>(`[data-act="${act}"]`)!.click();

  it("［终端画面］看那一屏 ·［在终端里打开］按那一家接上那个 tmux 会话", async () => {
    make().add(SPEC);
    await missNow();
    click("screen");
    expect(acts.preview).toHaveBeenCalledWith("devbox", "billing-2", "%7");
    click("attach");
    expect(acts.attach).toHaveBeenCalledWith("devbox", "claude", "billing-2");
  });

  it("★［结束 tmux 会话］先问会打断什么，结束了 ⇒ 这一行关掉、不再等", async () => {
    make().add(SPEC);
    await missNow();
    click("kill");
    await vi.advanceTimersByTimeAsync(0);
    expect(acts.confirm).toHaveBeenCalledWith({
      title: copyText("sessionState.killIdle.title", { name: "billing-2" }),
      action: copyText("sessionState.killIdle.action"),
      danger: true,
      rows: [{ label: copyText("kit.interrupts.cut"), items: [copyText("launch.slot.killCuts", { machine: "devbox", name: "billing-2" })] }],
    });
    expect(acts.kill).toHaveBeenCalledWith("devbox", "billing-2");
    expect(rows()).toEqual([]);
    tabs.add("late");
    noteLive("devbox", "late", { cwd: "/home/u/srv/billing" });
    expect(switched, "关掉之后不再等它").toEqual([]);
  });

  it("不确认 ⇒ 不动；结束失败 ⇒ 说一句、这一行留着", async () => {
    make().add(SPEC);
    await missNow();
    acts.confirm.mockResolvedValueOnce(false);
    click("kill");
    await vi.advanceTimersByTimeAsync(0);
    expect(acts.kill).not.toHaveBeenCalled();
    acts.kill.mockRejectedValueOnce(new Error("不在名单"));
    click("kill");
    await vi.advanceTimersByTimeAsync(0);
    // 那次失败交给失败 toast（带详情 ⇒ 换成那一句；这次没带 ⇒ 标题留界面那句、无按钮，原文进控制台）。
    expect(vi.mocked(failToast).mock.calls.map((c) => [c[0], String(c[1])])).toEqual([[copyText("tabSessionActions.kill.failed", { title: "billing-2" }), "Error: 不在名单"]]);
    expect(rows()).toHaveLength(1);
  });

  it("［关闭标签页］⇒ 摘掉、那一页收起、不再等", async () => {
    make().add(SPEC);
    await missNow();
    expect(__pendingCountForTests()).toBe(1);
    click("close");
    expect(rows()).toEqual([]);
    expect(panelHidden()).toBe(true);
    expect(__pendingCountForTests(), "那一件等也撤了").toBe(0);
    tabs.add("late");
    noteLive("devbox", "late", { cwd: "/home/u/srv/billing" });
    expect(switched).toEqual([]);
  });
});
