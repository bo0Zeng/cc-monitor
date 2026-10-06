/**
 * 「账号」面板的行为：勾号 · 无号可换两态 · 热切换 · 热切换不成立 · 跟随默认时只读。写都经 `quota-reads.ts`（这里桩住），
 * 数据只从 `appStore` 来（那几格由推送填）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const writeSessionRotation = vi.fn();
const switchHot = vi.fn();
const switchRestart = vi.fn();
const runRemoteAttach = vi.fn();
const openLog = vi.fn();
vi.mock("../../../src/frontend/ui/quota-reads", () => ({
  writeSessionRotation: (...a: unknown[]) => writeSessionRotation(...a),
  switchHot: (...a: unknown[]) => switchHot(...a),
  switchRestart: (...a: unknown[]) => switchRestart(...a),
  readQuota: vi.fn(() => new Promise(() => {})),
  readRotation: vi.fn(() => new Promise(() => {})),
  readSessionRotation: vi.fn(() => new Promise(() => {})),
}));

vi.mock("../../../src/frontend/ui/sessions-where", () => ({
  standingOf: vi.fn(async () => ({ kind: "running", names: ["proj-cc"], terminals: ["t1"] })),
}));
vi.mock("../../../src/frontend/ui/interrupt-reads", () => ({ askSessionInterrupts: vi.fn(async () => ({ families: [] })) }));
vi.mock("../../../src/frontend/ui/tab-batch-run", () => ({ startSettings: vi.fn(async () => ({})) }));
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({ runRemoteAttach: (...a: unknown[]) => runRemoteAttach(...a) }));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({ commands: { open_log_file: () => openLog() } }));

import { openAccountPanel, toggleAccountPanel, type AcctPanelHost } from "../../../src/frontend/ui/acct-panel.ts";
import { appStore } from "../../../src/frontend/ui/app-store.ts";
import type { QuotaRead } from "../../../src/frontend/ui/quota-lines.ts";
import type { Rotation } from "../../../src/frontend/ui/generated/Rotation.ts";
import type { SessionRotation } from "../../../src/frontend/ui/generated/SessionRotation.ts";

const NOW = 1_791_189_600;
const host: AcctPanelHost = {
  sessionTitle: () => "orders",
  cwdOf: () => "/w",
  openSettings: vi.fn(),
  openDefaultMenu: vi.fn(),
  defaultOf: () => "work",
  openResume: vi.fn(),
};

const LEDGER: QuotaRead = {
  state: "present",
  reason: null,
  path: null,
  now: NOW,
  accounts: ["work", "team", "api"].map((account) => ({
    agent: "claude-code",
    account,
    seenAt: NOW - 60,
    reading: {},
    kind: account === "api" ? ("api" as const) : ("sub" as const),
    state: "ok" as const,
    stale: false,
    limiting: account === "api" ? undefined : "5h",
    slots: account === "api" ? [] : [{ slot: "5h", pct: 30, resetsAt: NOW + 3600 }],
    login: "ok" as const,
  })),
  unseen: [],
  usableNow: ["work", "team", "api"],
  earliestReturn: null,
};

function seed(p: Partial<SessionRotation>, custom: Rotation | undefined): void {
  appStore.quota.set(new Map([["<local>", LEDGER]]));
  appStore.rotationDefault.set(new Map([["<local>", { state: "present", rotation: { order: [{ start: true }], enabled: [], when: "full", atLimit: "continue" }, followers: 1 }]]));
  appStore.sessionRotation.set(
    new Map([
      [
        "s1",
        {
          origin: "<local>",
          now: NOW,
          read: {
            state: "present",
            agent: "claude-code",
            follow: custom === undefined,
            custom,
            account: { start: "work", current: "work", since: NOW - 600, history: [], inPlace: "ok" },
            next: "team",
            atLimit: "continue",
            quota: { ...LEDGER.accounts[0] },
            ...p,
          },
        },
      ],
    ]),
  );
}

const panel = (): HTMLElement => document.querySelector<HTMLElement>('[role="dialog"]')!;
const flush = async (): Promise<void> => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  writeSessionRotation.mockReset().mockResolvedValue({ s1: { state: "done" } });
  switchHot.mockReset().mockResolvedValue({ s1: { state: "done" } });
  switchRestart.mockReset();
  runRemoteAttach.mockReset().mockResolvedValue(undefined);
  openLog.mockReset().mockResolvedValue(undefined);
  vi.mocked(host.openResume).mockReset();
  if (document.querySelector('[role="dialog"]')) toggleAccountPanel("s1", "<local>", host);
  document.body.replaceChildren();
});

describe("账号面板", () => {
  it("本会话：勾上一个号 ⇒ 整份写回（顺序里没有的排末尾，挪按量号到末尾是后端的事）", async () => {
    seed({}, { order: [{ start: true }, "team"], enabled: ["team"], when: "full", atLimit: "continue" });
    openAccountPanel("s1", "<local>", host);
    const boxes = [...panel().querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
    expect(boxes.map((b) => b.getAttribute("aria-label"))).toEqual(["加入轮换 work", "加入轮换 team", "加入轮换 api"]);
    expect(boxes[0].disabled, "起始 · 在用那一行锁着").toBe(true);
    boxes[2].click();
    await flush();
    expect(writeSessionRotation).toHaveBeenCalledWith("<local>", ["s1"], { custom: { order: [{ start: true }, "team", "api"], enabled: ["team", "api"], when: "full", atLimit: "continue" } });
  });

  it("无号可换：「满」触发时两态灰着；≥N% 时点「停」⇒ 写 atLimit stop", async () => {
    seed({}, { order: [{ start: true }], enabled: [], when: "full", atLimit: "continue" });
    openAccountPanel("s1", "<local>", host);
    const seg = (): HTMLButtonElement[] => [...panel().querySelectorAll<HTMLButtonElement>('[aria-label="无号可换时"] button')];
    expect(seg().map((b) => [b.textContent, b.disabled])).toEqual([["继续跑", true], ["停", true]]);
    toggleAccountPanel("s1", "<local>", host);
    seed({}, { order: [{ start: true }], enabled: [], when: { threshold: { n: 90 } }, atLimit: "continue" });
    openAccountPanel("s1", "<local>", host);
    seg()[1].click();
    await flush();
    expect(writeSessionRotation).toHaveBeenCalledWith("<local>", ["s1"], { custom: { order: [{ start: true }], enabled: [], when: { threshold: { n: 90 } }, atLimit: "stop" } });
  });

  it("切换：缺省选后端给的下一个；热切换 ⇒ rotation-switch hot", async () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", host);
    const go = [...panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "切换")!;
    go.click();
    await flush();
    expect(switchHot).toHaveBeenCalledWith("<local>", ["s1"], "team");
  });

  it("热切换不成立（未实时显示）⇒ 热切换灰、写原因、主按钮变「重启切换」", () => {
    seed({ account: { start: "work", current: "work", since: NOW, history: [], inPlace: "noRelay" } }, undefined);
    openAccountPanel("s1", "<local>", host);
    const radios = [...panel().querySelectorAll<HTMLInputElement>('input[type="radio"][name^="acct-mode"]')];
    expect(radios.map((r) => [r.disabled, r.checked])).toEqual([[true, false], [false, true]]);
    expect(panel().textContent).toContain("未实时显示");
    expect([...panel().querySelectorAll("button")].some((b) => b.textContent === "重启切换")).toBe(true);
  });

  it("跟随默认：列表只读（无把手、勾都灰），上方写「本机默认」那一句", () => {
    seed({}, undefined);
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelectorAll('[aria-label^="拖动排序"]').length).toBe(0);
    expect([...panel().querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].every((b) => b.disabled)).toBe(true);
    expect(panel().textContent).toContain("跟随本机默认");
  });
});

describe("账号面板 · 重启切换", () => {
  /** 热切换不成立 ⇒ 主按钮是「重启切换」；点它，等那一趟回来。 */
  async function restart(reply: unknown): Promise<void> {
    switchRestart.mockResolvedValue({ s1: reply });
    seed({ account: { start: "work", current: "work", since: NOW, history: [], inPlace: "noRelay" } }, undefined);
    openAccountPanel("s1", "<local>", host);
    [...panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "重启切换")!.click();
    await flush();
  }
  const toasts = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('[role="alert"], [role="status"]')];
  const toastButtons = (t: HTMLElement): string[] => [...t.querySelectorAll("button")].map((b) => b.textContent ?? "").filter((x) => x !== "");

  it("成了 ⇒ 接回后端说的那个终端、说一句成了", async () => {
    await restart({ state: "done", terminal: "proj-cc-2" });
    expect(switchRestart).toHaveBeenCalledWith("<local>", [expect.objectContaining({ sid: "s1", compact_first: false })], "team", expect.any(Number));
    expect(runRemoteAttach).toHaveBeenCalledWith("<local>", expect.any(String), "proj-cc-2", { quiet: true });
    expect(toasts().map((t) => t.textContent)).toEqual([expect.stringContaining("team · 重启切换 · orders")]);
  });

  it("没成、旧会话还在 ⇒ 「原会话保留 · 原因」，只带［日志］", async () => {
    await restart({ state: "failed", code: "stop_failed", old: "kept" });
    const [t] = toasts();
    expect(t.dataset.level).toBe("error");
    expect(t.textContent).toContain("重启切换失败 · 原会话保留 · 停不下旧进程");
    expect(toastButtons(t)).toEqual(["日志"]);
    [...t.querySelectorAll("button")].find((b) => b.textContent === "日志")!.click();
    expect(openLog).toHaveBeenCalledTimes(1);
    expect(runRemoteAttach).not.toHaveBeenCalled();
  });

  it("没成、旧会话已停 ⇒ 「原会话已结束 · 原因」带［恢复…］［日志］，点了各做各的", async () => {
    await restart({ state: "failed", code: "start_failed", old: "ended" });
    const [t] = toasts();
    expect(t.dataset.level).toBe("error");
    expect(t.textContent).toContain("重启切换失败 · 原会话已结束 · 新会话未起");
    expect(toastButtons(t)).toEqual(["恢复…", "日志"]);
    [...t.querySelectorAll("button")].find((b) => b.textContent === "恢复…")!.click();
    expect(host.openResume).toHaveBeenCalledWith("s1");
    await restart({ state: "failed", code: "notArrived", old: "ended" });
    const last = toasts().at(-1)!;
    expect(last.textContent).toContain("原会话已结束 · 未报到");
    [...last.querySelectorAll("button")].find((b) => b.textContent === "日志")!.click();
    expect(openLog).toHaveBeenCalledTimes(1);
  });

  it("那台给的码逐个说成人话，不落「原因不明」", async () => {
    const said: Record<string, string> = {
      account_unavailable: "team 不可用",
      ambiguous: "在多个终端里",
      not_in_terminal: "不在 tmux 里",
      session_already_live: "另有进程在写",
      shutting_down: "后端正在退出",
    };
    for (const [code, words] of Object.entries(said)) {
      document.body.replaceChildren();
      if (document.querySelector('[role="dialog"]')) toggleAccountPanel("s1", "<local>", host);
      await restart({ state: "failed", code, old: "kept" });
      expect(toasts().at(-1)!.textContent, code).toContain(`原会话保留 · ${words}`);
    }
  });
});
