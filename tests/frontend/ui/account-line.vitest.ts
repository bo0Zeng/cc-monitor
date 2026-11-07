/**
 * 「号 ＋ 余量」小件：号牌 · 号名 · 那台写好的那一格余量；没有那一格只露号。余量取那台额度账里这个号那一格，
 * 账读不出 ⇒ 顶上那一格。字都是夹具里的「后端·…」样字，界面不拼。
 */
import { beforeEach, describe, expect, it } from "vitest";
import { accountLine, quotaCellOf } from "../../../src/frontend/ui/account-line";
import { appStore } from "../../../src/frontend/ui/app-store";

const read = (over: Record<string, unknown>) =>
  ({ state: "present", reason: null, detail: null, path: null, now: 1, accounts: [], unseen: [], candidates: [], text: null, fiveHour: null, names: { accounts: {}, slots: {} }, verdict: null, ...over }) as never;

beforeEach(() => appStore.quota.set(new Map()));

describe("号 ＋ 余量", () => {
  it("★ 号牌 · 号名 · 余量一行；没有余量那一格 ⇒ 只露号", () => {
    const plate = document.createElement("span");
    plate.textContent = "wk";
    const l = accountLine({ plate, name: "work", quota: "后端·5h 62%" });
    expect(l.textContent).toBe("wkwork后端·5h 62%");
    expect(l.querySelector('[data-part="quota"]')?.textContent).toBe("后端·5h 62%");
    const bare = accountLine({ plate: document.createElement("span"), name: "team", quota: null });
    expect(bare.querySelector('[data-part="quota"]')).toBeNull();
  });

  it("★ 余量取那台额度账里这个号那一格；账读不出 ⇒ 顶上那一格；还没问回来 ⇒ 没有", () => {
    expect(quotaCellOf("<local>", "work")).toBeNull();
    appStore.quota.set(new Map([["<local>", read({ accounts: [{ account: "work", fiveHour: "后端·work 那格" }] })]]));
    expect(quotaCellOf("<local>", "work")).toBe("后端·work 那格");
    expect(quotaCellOf("<local>", "team")).toBeNull();
    appStore.quota.set(new Map([["<local>", read({ state: "unreadable", fiveHour: "后端·5h 读不到", accounts: [{ account: "work", fiveHour: "后端·旧" }] })]]));
    expect(quotaCellOf("<local>", "work")).toBe("后端·5h 读不到");
  });
});
