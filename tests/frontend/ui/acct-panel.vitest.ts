/**
 * 「账号」面板：头下那一条（中转没见过这个会话 · 额度账读不出）＋ 底栏（新会话默认 · 管理账号…）。
 * 轮换那一块此刻空着：这里也验它不画任何第三版的东西。数据只从 `appStore` 来。期望手写。
 * 末尾一节：策略下拉 · 底栏号名长了在框内截断的那几条样式。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import postcss, { type Rule } from "postcss";

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

const view = { now: 1, nowText: "00:00", title: null, strategy: null, board: { all: [], rows: [] }, start: "work", current: "work", lineup: [], manual: false, off: [], accounts: [], listed: 0, decision: null, log: [], hints: [] } satisfies RotationViewReply;

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

  it("★ quota-read 整问失败 ⇒ 一条红的写出错那一层那一句 ＋ 重试；读到了、会话也见过 ⇒ 头下不出提示，也不画任何轮换的东西", () => {
    appStore.sessionRotation.set(new Map([["s1", { origin: "<local>", read: view }]]));
    appStore.quota.set(new Map([["<local>", { state: "failed", said: "连接层那一句", detail: "D-detail" }]]));
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelector("[role=alert]")?.textContent).toContain("连接层那一句");
    expect(btn(copyText("acct.now.retry"))).toBeDefined();
    appStore.quota.set(new Map([["<local>", { state: "present", reason: null, detail: null, path: null, now: 1, accounts: [], unseen: [], candidates: [], text: null, fiveHour: null, names: { accounts: {}, slots: {} }, verdict: null }]]));
    expect(btn(copyText("acct.now.retry"))).toBeUndefined();
    expect(panel().querySelector("[data-rot-cfg], [data-rc-row], [data-rot-switch]")).toBeNull();
  });

  it("★ 那份账那台读不出 ⇒ 头下照抄后端写好的那一句（text），面板不自己补一句", () => {
    appStore.sessionRotation.set(new Map([["s1", { origin: "<local>", read: view }]]));
    const unreadable = { state: "unreadable" as const, reason: "原因一句", detail: "D-detail", path: null, now: 1, accounts: [], unseen: [], candidates: [], text: "后端那一句", fiveHour: null, names: { accounts: {}, slots: {} }, verdict: null };
    appStore.quota.set(new Map([["<local>", unreadable]]));
    openAccountPanel("s1", "<local>", host);
    expect(panel().textContent).toContain("后端那一句");
    expect(panel().textContent).not.toContain("原因一句");
    // 成品里没有那一句（违约）⇒ 不拿 reason 或自己的「读取失败」顶上。
    appStore.quota.set(new Map([["<local>", { ...unreadable, text: null }]]));
    expect(panel().textContent).not.toContain("原因一句");
    expect(panel().textContent).not.toContain(copyText("acct.text.readFail"));
  });

  it("★ 额度账还没问回来 ⇒ 不说读取失败、不出重试，头下说读取中", () => {
    appStore.sessionRotation.set(new Map([["s1", { origin: "<local>", read: view }]]));
    openAccountPanel("s1", "<local>", host);
    expect(panel().querySelector("[role=alert]")).toBeNull();
    expect(btn(copyText("acct.now.retry"))).toBeUndefined();
    expect(panel().textContent).toContain(copyText("kit.state.reading"));
  });
});

/**
 * 长名字不撑破（10-19 用户：账号面板里点下拉选策略，字超出去）。门禁里没有排版引擎，这里钉住让截断成立的那几条样式；
 * 浏览器里量外接框的那一半住截图工具。
 */
const UI = resolve(__dirname, "../../../src/frontend/ui");

/** 一份 CSS 里，选择器恰好是 `sel` 的规则里各声明的最后取值。 */
function declsOf(file: string, sel: string): Map<string, string> {
  const out = new Map<string, string>();
  postcss.parse(readFileSync(resolve(UI, file), "utf8")).walkRules((r: Rule) => {
    if (!r.selectors.map((x) => x.trim()).includes(sel)) return;
    r.walkDecls((d) => void out.set(d.prop, d.value));
  });
  return out;
}

/**
 * 能在框内截断：单行省略（那个类 composes 全局的 one-line，三件套只住那一处）· 能缩（flex 第二个数不是 0、不是 none）·
 * 能缩到比字窄（min-width 不是 auto）。`sel` 是排版那条规则；省略号挂在它最后那个类上。
 */
function clips(file: string, sel: string): void {
  const cls = sel.split(/\s|>/).filter(Boolean).at(-1) ?? sel;
  expect(declsOf(file, cls).get("composes"), `${cls} composes`).toBe("one-line from global");
  const d = declsOf(file, sel);
  const flex = d.get("flex") ?? "";
  expect(flex !== "none" && !/^\S+ 0( |$)/.test(flex), `${sel} 的 flex = ${flex}`).toBe(true);
  expect(d.get("min-width") ?? "auto", `${sel} min-width`).not.toBe("auto");
}

describe("长名字在框内截断", () => {
  it("★ 策略下拉面板的一项：名字 · 灰字 · 右侧摘要三段都能截断，项本身不让字伸出去", () => {
    expect(declsOf("kit/menu.module.css", ".menuItem").get("white-space")).toBe("nowrap");
    expect(declsOf("kit/menu.module.css", ".menuItem").get("overflow")).toBe("hidden");
    for (const sel of [".menuLabel", ".menuNote", ".menuDetail"]) clips("kit/menu.module.css", sel);
  });

  it("★ 策略下拉框上：名字与灰字都能截断", () => {
    for (const sel of [".selectText", ".selectNote"]) clips("kit/select.module.css", sel);
  });

  it("★ 底栏「新会话默认」那个号名能截断，框能缩、不把「管理账号…」挤出去", () => {
    clips("acct.module.css", ".acctPick > .acctPickName");
    expect(declsOf("acct.module.css", ".acctFoot .acctPick").get("min-width")).toBe("0");
  });
});
