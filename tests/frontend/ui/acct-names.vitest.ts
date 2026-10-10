// 号名 / 位名照核心写好的那张表（成品上的 `names`，后端 `accounts/quota/name_words.rs`）：界面不认 `_`、不认 `5h` / `7d`。
// 没收到表之前照原样出（不自己补「默认号」那个字）；收到哪一件成品带来的表都一样。
import { describe, it, expect } from "vitest";
import { accountLabel, slotLabel } from "../../../src/frontend/ui/acct-words";
import { decodeQuotaRead, decodeRulesRead } from "../../../src/frontend/ui/quota-reads";

const NAMES = { accounts: { _: "家里那个号" }, slots: { "5h": "五小时", "7d": "七天" } };
const quota = (names?: unknown) => ({ state: "absent", reason: null, path: null, now: 1, accounts: [], unseen: [], usableNow: [], earliestReturn: null, fiveHour: null, ...(names ? { names } : {}) });

describe("号名 / 位名照核心那张表", () => {
  it("没收到表之前照原样", () => {
    expect(accountLabel("_")).toBe("_");
    expect(slotLabel("5h")).toBe("5h");
  });
  it("额度成品带来的表：`_` 与两个语义位照表里的字，别的号照原名", () => {
    decodeQuotaRead(quota(NAMES));
    expect(accountLabel("_")).toBe("家里那个号");
    expect(accountLabel("work")).toBe("work");
    expect(slotLabel("5h")).toBe("五小时");
    expect(slotLabel("7d")).toBe("七天");
  });
  it("规则表带来的表同样收", () => {
    decodeRulesRead({ state: "absent", reason: null, detail: null, defaultRule: "d", followText: "f", rules: [], names: { accounts: { _: "甲" }, slots: { "5h": "乙", "7d": "丙" } } });
    expect(accountLabel("_")).toBe("甲");
    expect(slotLabel("7d")).toBe("丙");
  });
});
