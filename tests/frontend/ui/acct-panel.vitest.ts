/**
 * 「账号」面板：头下那一条（中转没见过这个会话 · 额度账读不出）＋ 底栏（新会话默认 · 管理账号…）。
 * 轮换那一块此刻空着：这里也验它不画任何第三版的东西。数据只从 `appStore` 来。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { never } = vi.hoisted(() => ({ never: () => new Promise(() => {}) }));
vi.mock("../../../src/frontend/ui/rotation-view-reads", () => ({ readSessionView: vi.fn(never) }));
vi.mock("../../../src/frontend/ui/quota-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/quota-reads")>()),
  readQuota: vi.fn(never),
}));

import { openAccountPanel, toggleAccountPanel, type AcctPanelHost } from "../../../src/frontend/ui/acct-panel.ts";
import { appStore } from "../../../src/frontend/ui/app-store.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const host: AcctPanelHost = {
  sessionTitle: () => "orders",
  openSettings: vi.fn(),
  openDefaultMenu: vi.fn(),
  defaultOf: () => "work",
};

const view = { now: 1, nowText: "00:00", title: null, strategy: null, settings: {}, board: { all: [], rows: [] }, dirty: false, start: "work", current: "work", accounts: [], plates: {}, listed: 0, decision: null, timeline: null, past: null, baseRev: "", log: [], hints: [] } satisfies RotationViewReply;

const panel = (): HTMLElement => document.querySelector<HTMLElement>("aside[role=dialog]")!;
const btn = (text: string): HTMLButtonElement | undefined => [...panel().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === text);

beforeEach(() => {
  if (document.querySelector("aside[role=dialog]")) toggleAccountPanel("s1", "<local>", host);
  document.body.replaceChildren();
  appStore.quota.set(new Map());
  appStore.sessionRotation.set(new Map());
});

describe("账号面板", () => {
  it("★ 中转没见过这个会话 ⇒ 头下一条 未实时显示 ＋ 接入…；底栏 新会话默认 work · 管理账号…", () => {
    appStore.sessionRotation.set(new Map([["s1", { origin: "<local>", read: null }]]));
    openAccountPanel("s1", "<local>", host);
    expect(panel().textContent).toContain(copyText("acct.now.noRelay"));
    btn(copyText("acct.now.connect"))!.click();
    expect(host.openSettings).toHaveBeenCalledWith("<local>");
    expect(panel().textContent).toContain(copyText("acct.foot.default"));
    expect(btn(copyText("acct.foot.manage"))).toBeDefined();
  });

  it("★ 额度账读不出 ⇒ 一条红的 ＋ 重试；读到了、会话也见过 ⇒ 头下不出提示，也不画任何轮换的东西", () => {
    appStore.sessionRotation.set(new Map([["s1", { origin: "<local>", read: view }]]));
    appStore.quota.set(new Map([["<local>", null]]));
    openAccountPanel("s1", "<local>", host);
    expect(btn(copyText("acct.now.retry"))).toBeDefined();
    appStore.quota.set(new Map([["<local>", { state: "present", reason: null, detail: null, path: null, now: 1, accounts: [], unseen: [], usableNow: [], candidates: [], earliestReturn: null, text: null, fiveHour: null, names: { accounts: {}, slots: {} } }]]));
    expect(btn(copyText("acct.now.retry"))).toBeUndefined();
    expect(panel().querySelector("[data-rot-cfg], [data-rc-row], [data-rot-switch]")).toBeNull();
  });
});
