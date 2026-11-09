// 号色：账号库里的事实（那台后端加号时分、存进清单），界面只画 `accounts-list` 每号的 `color`，不自己按名字算。
import { copyText } from "../../../src/frontend/ui/copy-table";
import { describe, it, expect, beforeEach } from "vitest";
import { accountAvatarEl, accountColorOf } from "../../../src/frontend/ui/account-color";
import { appStore, putIn } from "../../../src/frontend/ui/app-store";
import type { Account, AccountsState } from "../../../src/frontend/ui/accounts";
import { LOCAL, type Machine } from "../../../src/frontend/ui/ipc/origin";

const REMOTE = "ssh:box" as Machine;

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
    badge: { text: "", warn: false },
    color,
  };
}

function put(origin: Machine, accounts: Account[]): void {
  putIn(appStore.accounts, origin, { origin, available: true, error: null, oldBackend: false, meta: null, accounts, notice: null } as AccountsState);
}

/** 头像上挂的号色那一格（`--acct: var(--acct-c<格>)`）；没挂 ⇒ `null`。 */
const slotOf = (el: HTMLElement): number | null => {
  const m = /^var\(--acct-c(\d+)\)$/.exec(el.style.getPropertyValue("--acct").trim());
  return m ? Number(m[1]) : null;
};

describe("号色照那台账号库给的那一格画", () => {
  beforeEach(() => {
    put(LOCAL, [acct("team", 0), acct("lab", 2)]);
    put(REMOTE, [acct("team", 5)]);
  });

  it("名字哈希同格的 team / lab 画成不同色（各照各的 `color`）", () => {
    expect(slotOf(accountAvatarEl("team"))).toBe(0);
    expect(slotOf(accountAvatarEl("lab"))).toBe(2);
    expect(accountColorOf("lab")).toBe(2);
  });

  it("按号所在那台查：同名的号在别台是别的色", () => {
    expect(slotOf(accountAvatarEl("team", { origin: REMOTE }))).toBe(5);
    expect(accountColorOf("team", REMOTE)).toBe(5);
  });

  it("那台没给这个号 ⇒ 不猜色（中性，不挂 --acct）", () => {
    expect(accountColorOf("nobody")).toBeNull();
    expect(slotOf(accountAvatarEl("nobody"))).toBeNull();
  });

  it("删了一个号，别的号色不变（照新那份清单画）", () => {
    put(LOCAL, [acct("lab", 2)]);
    expect(slotOf(accountAvatarEl("lab"))).toBe(2);
  });
});

describe("accountAvatarEl（头像元素）", () => {
  it("类含 .acct-avatar + 缩写文本 + aria-hidden", () => {
    put(LOCAL, [{ ...acct("zhang", 3), plate: "核牌" }]);
    const el = accountAvatarEl("zhang");
    expect(el.classList.contains("acct-avatar")).toBe(true);
    expect(el.textContent, "号牌字照那台后端写好的那一格").toBe("核牌");
    expect(accountAvatarEl("nobody").textContent, "库里没有 ⇒ 不另取").toBe(copyText("acct.val.none"));
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
