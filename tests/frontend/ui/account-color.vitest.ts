// 号色：账号库里的事实（那台后端加号时分、存进清单），界面只画 `accounts-list` 每号的 `color`，不自己按名字算。
import { describe, it, expect, beforeEach } from "vitest";
import { accountAvatarEl, accountColorOf } from "../../../src/frontend/ui/account-color";
import { putAccounts } from "../../../src/frontend/ui/app-store";
import type { Account, AccountsState } from "../../../src/frontend/ui/accounts";
import { LOCAL_ORIGIN, type Origin } from "../../../src/frontend/ui/ipc/origin";

const REMOTE = "ssh:box" as Origin;

function acct(name: string, color: number): Account {
  return {
    name,
    email: "",
    configDir: `/h/${name}`,
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    authKind: "subscription",
    authReady: true,
    selectable: true,
    badge: { text: "", warn: false, title: "" },
    color,
  };
}

function put(origin: Origin, accounts: Account[]): void {
  putAccounts(origin, { origin, available: true, error: null, oldBackend: false, meta: null, accounts, notice: null } as AccountsState);
}

const slotClasses = (el: HTMLElement): string[] => [...el.classList].filter((c) => /^acct-c\d$/.test(c));

describe("号色照那台账号库给的那一格画", () => {
  beforeEach(() => {
    put(LOCAL_ORIGIN, [acct("team", 0), acct("lab", 2)]);
    put(REMOTE, [acct("team", 5)]);
  });

  it("名字哈希同格的 team / lab 画成不同色（各照各的 `color`）", () => {
    expect(slotClasses(accountAvatarEl("team"))).toEqual(["acct-c0"]);
    expect(slotClasses(accountAvatarEl("lab"))).toEqual(["acct-c2"]);
    expect(accountColorOf("lab")).toBe(2);
  });

  it("按号所在那台查：同名的号在别台是别的色", () => {
    expect(slotClasses(accountAvatarEl("team", { origin: REMOTE }))).toEqual(["acct-c5"]);
    expect(accountColorOf("team", REMOTE)).toBe(5);
  });

  it("那台没给这个号 ⇒ 不猜色（中性，没有 acct-cN）", () => {
    expect(accountColorOf("nobody")).toBeNull();
    expect(slotClasses(accountAvatarEl("nobody"))).toEqual([]);
  });

  it("删了一个号，别的号色不变（照新那份清单画）", () => {
    put(LOCAL_ORIGIN, [acct("lab", 2)]);
    expect(slotClasses(accountAvatarEl("lab"))).toEqual(["acct-c2"]);
  });
});

describe("accountAvatarEl（头像元素）", () => {
  it("类含 .acct-avatar + 缩写文本 + aria-hidden", () => {
    put(LOCAL_ORIGIN, [acct("zhang", 3)]);
    const el = accountAvatarEl("zhang");
    expect(el.classList.contains("acct-avatar")).toBe(true);
    expect(el.textContent).toBe("zh"); // badgeText：ASCII 取前 2
    expect(el.getAttribute("aria-hidden")).toBe("true");
  });
  it("ghost=true → 加 .ghost（幽灵态）", () => {
    expect(accountAvatarEl("bob", { ghost: true }).classList.contains("ghost")).toBe(true);
  });
  it("size 覆盖边长", () => {
    const el = accountAvatarEl("z", { size: 20 });
    expect(el.style.width).toBe("20px");
    expect(el.style.height).toBe("20px");
  });
});
