/**
 * 「账号」面板的行为：勾号 · 无号可换两态 · 热切换 · 热切换不成立 · 跟随默认时只读。写都经 `quota-reads.ts`（这里桩住），
 * 数据只从 `appStore` 来（那几格由推送填）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const writeSessionRotation = vi.fn();
const switchHot = vi.fn();
vi.mock("../../../src/frontend/ui/quota-reads", () => ({
  writeSessionRotation: (...a: unknown[]) => writeSessionRotation(...a),
  switchHot: (...a: unknown[]) => switchHot(...a),
  switchRestart: vi.fn(),
  readQuota: vi.fn(() => new Promise(() => {})),
  readRotation: vi.fn(() => new Promise(() => {})),
  readSessionRotation: vi.fn(() => new Promise(() => {})),
}));

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
