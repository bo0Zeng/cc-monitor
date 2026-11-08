/**
 * 「号 ＋ 余量」小件与它读的那一行：号牌 · 号名 · 那台写好的那一格余量；没有那一格只露号。号名与余量都照那台 `accounts-list`
 * 那一行（`label` · `slots` 第一格），两台各照各的；那台那一行没有 ⇒ 号名照原名、不出余量。字都是夹具里的「后端·…」样字，界面不拼。
 */
import { beforeEach, describe, expect, it } from "vitest";
import { accountLine, labelOf, quotaCellOf } from "../../../src/frontend/ui/account-line";
import { appStore } from "../../../src/frontend/ui/app-store";
import type { AccountsState } from "../../../src/frontend/ui/accounts";

const row = (name: string, label: string, slots: { text: string; tone: string }[] = []) => ({ name, label, slots: slots.map((s) => ({ slot: "5h", ...s })) });
const listed = (origin: string, rows: ReturnType<typeof row>[]): [string, AccountsState] => [
  origin,
  { origin, available: true, error: null, oldBackend: false, meta: null, accounts: rows, notice: null } as unknown as AccountsState,
];

beforeEach(() => appStore.accounts.set(new Map()));

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

  it("★ 两台号名不同 ⇒ 各照那台那一行的 label；那台那一行没有 ⇒ 原名", () => {
    appStore.accounts.set(new Map([listed("<local>", [row("0", "后端·本机的零号")]), listed("devbox", [row("0", "后端·devbox 的零号")])]));
    expect(labelOf("<local>", "0")).toBe("后端·本机的零号");
    expect(labelOf("devbox", "0")).toBe("后端·devbox 的零号");
    expect(labelOf("devbox", "work")).toBe("work");
    expect(labelOf("gpu-01", "0")).toBe("0");
  });

  it("★ 余量取那台那一行的第一格；没有格 · 那一行没有 · 那台没读过 ⇒ 没有", () => {
    appStore.accounts.set(new Map([listed("<local>", [row("work", "work", [{ text: "后端·work 那格", tone: "plain" }, { text: "后端·7d", tone: "plain" }]), row("team", "team")])]));
    expect(quotaCellOf("<local>", "work")).toBe("后端·work 那格");
    expect(quotaCellOf("<local>", "team")).toBeNull();
    expect(quotaCellOf("<local>", "nope")).toBeNull();
    expect(quotaCellOf("devbox", "work")).toBeNull();
  });
});
