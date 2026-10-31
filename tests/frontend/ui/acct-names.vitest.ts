// 号名 / 位名照核心写好的那张表（成品上的 `names`，后端 `accounts/quota/name_words.rs`）：界面不认 `_`、不认 `5h` / `7d`。
// 没收到表之前号名照原样出（不自己补「默认号」那个字）；位名不拿码兜：回包里某一格的语义位不在同一份的 `names.slots` ⇒
// 两端契约对不上，整份照「回包读不出」（同手机 `usageView`）。
import { describe, it, expect } from "vitest";
import { accountLabel, slotLabel } from "../../../src/frontend/ui/acct-words";
import { tookQuota } from "../../../src/frontend/ui/quota-reads";
import { ReplyUnreadable, unreadableSaid } from "../../../src/frontend/ui/ipc/chan-caller";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import type { QuotaRead } from "../../../src/frontend/ui/acct-words";

const NAMES = { accounts: { _: "家里那个号" }, slots: { "5h": "五小时", "7d": "七天" } };
const quota = (names: QuotaRead["names"], accounts: QuotaRead["accounts"] = []): QuotaRead => ({ state: "absent", reason: null, detail: null, path: null, now: 1, accounts, unseen: [], candidates: [], text: null, fiveHour: null, names, verdict: null });
/** 一个号只带一格语义位（别的格这一条用不着）。 */
const withSlot = (slot: string) => [{ slots: [{ slot, text: "63%" }] } as unknown as QuotaRead["accounts"][number]];

describe("号名 / 位名照核心那张表", () => {
  it("没收到表之前：号名照原样，位名不露码", () => {
    expect(accountLabel("_")).toBe("_");
    expect(slotLabel("5h")).not.toBe("5h");
  });
  it("额度成品带来的表：`_` 与两个语义位照表里的字，别的号照原名", () => {
    tookQuota(LOCAL_ORIGIN, quota(NAMES));
    expect(accountLabel("_")).toBe("家里那个号");
    expect(accountLabel("work")).toBe("work");
    expect(slotLabel("5h")).toBe("五小时");
    expect(slotLabel("7d")).toBe("七天");
  });
  it("回包带的一格语义位不在 `names.slots` ⇒ 回包读不出、不收这张表、界面不出 `5h`", () => {
    tookQuota(LOCAL_ORIGIN, quota(NAMES));
    const bad = quota({ accounts: { _: "别的字" }, slots: { "7d": "七天" } }, withSlot("5h"));
    expect(() => tookQuota(LOCAL_ORIGIN, bad)).toThrow(ReplyUnreadable);
    expect(() => tookQuota(LOCAL_ORIGIN, bad)).toThrow(unreadableSaid(LOCAL_ORIGIN));
    expect(accountLabel("_")).toBe("家里那个号");
    expect(slotLabel("5h")).toBe("五小时");
  });
});
