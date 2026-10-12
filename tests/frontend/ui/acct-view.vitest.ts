/**
 * 额度几处界面的排版模型：状态栏按钮各态 · 悬停卡会话补上的那一样 · 记录原因 · 提示条 · 标签页 `✕ 5h`。期望手写（照额度稿修订 §1 那张表）。
 * 悬停卡里号那一段照抄 `quota-read` 每号的 `rows`（行模型住核心 `faces/quota_rows.rs`，金样 quota-text.golden.json 锁），这里只验会话补上的两样与采样那台。
 */
import { describe, expect, it } from "vitest";

import { bannerOf, sessionChip, sessionHoverRows, tabBlockedOf, whyOf } from "../../../src/frontend/ui/acct-view.ts";
import type { SessionRotationEntry } from "../../../src/frontend/ui/app-store.ts";
import { fiveHourCell, takeNames, type QuotaRead } from "../../../src/frontend/ui/acct-words.ts";
// 号名 / 位名照核心写好的那张表（后端 `accounts/quota/name_words.rs` 真写的金样）。
import NAMES from "../../__fixtures__/names.golden.json";
import type { QuotaShow } from "../../../src/frontend/ui/generated/QuotaShow.ts";
import type { SessionRotation } from "../../../src/frontend/ui/generated/SessionRotation.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../test-support/copy-pattern";
import { present } from "../../test-support/rot-fixtures";

// 时刻的字由后端写好（`…Text`），这里照回包的样子手写一份；界面不换算。
const NOW = 1_791_189_600;

function quota(p: Partial<QuotaShow> = {}): QuotaShow {
  return { kind: "sub", state: "ok", stale: false, limiting: "5h", usage: { slot: "5h", window: "5h", value: "63%", text: "5h 63%", tone: "plain" }, slots: [{ slot: "5h", pct: 63, resetsAt: NOW + 6600, resetsAtText: "13:50", text: "63%", tone: "plain" }, { slot: "7d", pct: 41, resetsAt: NOW + 3 * 86400, resetsAtText: "10-07 12:00", text: "41%", tone: "plain" }], login: "ok", ...p };
}

function entry(p: Partial<SessionRotation> = {}, q: Partial<QuotaShow> = {}): SessionRotationEntry {
  return { origin: "<local>", now: NOW, read: present({ start: "personal", current: "personal", since: NOW - 600, quota: quota(q), ...p }) };
}

const LEDGER: QuotaRead = {
  state: "present",
  reason: null,
  path: null,
  now: NOW,
  accounts: [
    { agent: "claude-code", account: "personal", seenAt: NOW - 120, seenAtText: "11:58", reading: {}, rows: [["personal", copyText("acct.kind.sub")], ["5h", "63%", "↻13:50", "+1h50m"], ["7d", "41%", "↻10-07 12:00", "+3d"], [copyText("acct.row.over"), "—"], [copyText("acct.row.seen"), "11:58"]].map((r) => r.map((text) => ({ text, tone: "plain" as const }))), warm: { act: "wait" as const, text: "" }, fiveHour: "5h 63%", ...quota() },
    { agent: "claude-code", account: "team", seenAt: NOW - 120, seenAtText: "11:58", reading: {}, rows: [], warm: { act: "wait" as const, text: "" }, fiveHour: "5h 4%", ...quota({ slots: [{ slot: "5h", pct: 4, resetsAt: NOW + 5400, resetsAtText: "13:30", text: "4%", tone: "plain" }], usage: { slot: "5h", window: "5h", value: "4%", text: "5h 4%", tone: "plain" } }) },
    { agent: "claude-code", account: "api", seenAt: NOW - 120, seenAtText: "11:58", reading: { resetsAt: NOW + 720, resetsAtText: "12:12" }, rows: [], warm: { act: "wait" as const, text: "" }, fiveHour: null, ...quota({ kind: "api", state: "refused", slots: [], limiting: undefined, usage: { value: copyText("acct.val.refusedOnly"), reset: "↻12:12", text: "↻12:12", tone: "fail" } }) },
  ],
  unseen: [],
  usableNow: ["personal", "team"],
  earliestReturn: null,
  fiveHour: null,
};

describe("状态栏账号按钮：照抄核心写好的用量那一格", () => {
  const chip = (e: SessionRotationEntry) => sessionChip(e, null)!;
  it("窗口 · 值 · 重置 · 语气都照 `usage`（颜色档在核心 `show.rs::usage_of`）；换过号带 ⇄", () => {
    expect(chip(entry())).toMatchObject({ name: "personal", window: "5h", value: "63%", reset: null, tone: "plain", swapped: false });
    const refused = chip(entry({}, { state: "refused", usage: { slot: "5h", window: "5h", value: "✕", reset: "↻13:00", text: "5h ✕ ↻13:00", tone: "fail" } }));
    expect(refused).toMatchObject({ window: "5h", value: "✕", reset: "↻13:00", tone: "fail" });
    expect(chip(entry({}, { kind: "api", usage: { value: copyText("acct.kind.api"), text: copyText("acct.kind.api"), tone: "plain" } }))).toMatchObject({ window: null, value: copyText("acct.kind.api"), reset: null });
    const sw = entry({ start: "work", current: "personal" });
    expect(chip(sw).swapped).toBe(true);
    expect(chip(entry({}, { stale: true })).stale).toBe(true);
  });
  it("中转没见过这个会话 ⇒ 只画归属的号、无用量；什么都不知道 ⇒ 不画；`_` 照核心那张表写", () => {
    const absent: SessionRotationEntry = { origin: "<local>", now: NOW, read: { state: "absent", inPlace: "noRelay" } };
    expect(sessionChip(absent, "work")).toMatchObject({ name: "work", value: null });
    expect(sessionChip(undefined, null)).toBeNull();
    takeNames(NAMES);
    expect(sessionChip(entry({ start: "_", current: "_" }), null)?.name).toBe(NAMES.accounts._);
  });
});

describe("悬停卡：会话补上的那一样", () => {
  it("★ 到了这号这一窗此刻的线的那一窗（后端按本会话那一份判 `atLine`）⇒ 那一行行尾「到线」；别的窗不写", () => {
    const e = entry({}, { slots: [{ slot: "5h", pct: 22, text: "22%", tone: "plain" }, { slot: "7d", pct: 91, atLine: true, text: "91%", tone: "plain" }] });
    const rows = sessionHoverRows(e, null, LEDGER)!;
    expect(rows.find((r) => r[0] === "7d")!.at(-1)).toBe(copyText("acct.hover.atLine"));
    expect(rows.find((r) => r[0] === "5h")).not.toContain(copyText("acct.hover.atLine"));
  });
  it("首行尾 `⇄ 时刻 ← 原号`（照最后一条落到此刻这个号上的换号记录）；没有「下一个」那一行；采样那一格后面接上是哪台采的", () => {
    const e = entry({
      start: "work",
      current: "personal",
      history: [{ at: NOW - 600, atText: "11:50", from: "work", to: "personal", leave: { k: "top" }, text: "T" }],
    });
    const rows = sessionHoverRows(e, null, LEDGER)!;
    expect(rows[0].slice(0, 2)).toEqual(["personal", copyText("acct.kind.sub")]);
    expect(rows[0][2]).toBe(copyText("acct.hover.from", { at: "11:50", name: "work" }));
    expect(rows.map((r) => r[0])).toEqual(["personal", "5h", "7d", copyText("acct.row.over"), copyText("acct.row.seen")]);
    expect(rows[4]).toEqual([copyText("acct.row.seen"), "11:58", copyText("acct.machine.here")]);
  });
  it("中转没见过 ⇒ `用量 — 未实时显示` · `切换 仅重启切换`", () => {
    const absent: SessionRotationEntry = { origin: "<local>", now: NOW, read: { state: "absent", inPlace: "noRelay" } };
    expect(sessionHoverRows(absent, "personal", LEDGER)).toEqual([["personal", copyText("acct.kind.sub")], [copyText("acct.hover.usage"), "—", copyText("acct.hover.noRelay")], [copyText("acct.hover.switch"), copyText("acct.hover.restartOnly")]]);
  });
});

describe("记录 · 提示条 · 标签页", () => {
  it("原因照后端写好的那一句（`leave` ＋ `pick`）＋ 那一刻原号几点重置", () => {
    expect(whyOf({ at: NOW, atText: "12:00", from: "work", to: "personal", leave: { k: "unusable", why: {} }, text: "T-why", fromResetsAt: NOW + 3600, fromResetsAtText: "13:00" })).toEqual({ why: "T-why", reset: copyText("acct.reset.at", { at: "13:00" }) });
    expect(whyOf({ at: NOW, atText: "12:00", from: "a", to: "b", leave: { k: "manual" } })).toEqual({ why: "", reset: null });
  });
  it("卡住：照后端写好的那一句；最早回来的那一刻已过 ⇒ 中性的「已过 · 可续发」；能发 ⇒ 不出", () => {
    expect(bannerOf(entry())).toBeNull();
    const all = entry({ blocked: { earliest: { account: "team", at: NOW + 5400, atText: "13:30", atRelText: "+1h30m" }, text: "B-all" } }, { state: "refused" });
    expect(bannerOf(all)).toEqual({ text: "B-all", tone: "warn" });
    expect(bannerOf(entry({ blocked: { text: "B-none" } }))).toEqual({ text: "B-none", tone: "warn" });
    const back = entry({ blocked: { earliest: { account: "team", at: NOW - 60, atText: "11:59" }, text: "B-back" } }, { state: "refused" });
    expect(bannerOf(back), "已过（核心没写距今）").toEqual({ text: expect.stringMatching(copyPattern("acct.banner.back", { name: "team" }, { whole: true })), tone: "neutral" });
  });
  it("标签页：被卡 ⇒ `✕ 5h` ＋ 悬停一行；能发 ⇒ 不出", () => {
    expect(tabBlockedOf(entry())).toBeNull();
    const b = tabBlockedOf(entry({ blocked: { earliest: { account: "team", at: NOW + 600, atText: "12:10" }, text: "B" } }, { state: "refused" }));
    expect(b?.text).toBe("✕ 5h");
    expect(b?.hover).toMatch(copyPattern("acct.tab.hover", { w: "5h", name: "team" }, { whole: true }));
  });
});

describe("每号 5h 那一格（恢复菜单 · 新建会话的账号项）", () => {
  it("照抄核心写好的那一格：读不出 ⇒ 顶上那一格；读得出 ⇒ 那个号那一格；按量号 / 账上没有 ⇒ 不出", () => {
    const r = { state: "unreadable", reason: "x", detail: null, path: null, now: 0, accounts: [], unseen: [], usableNow: [], earliestReturn: null, fiveHour: "那台写的读不到" } as QuotaRead;
    expect(fiveHourCell(r, "claude-code", "work")).toBe("那台写的读不到");
    expect(fiveHourCell(LEDGER, "claude-code", "personal")).toBe("5h 63%");
    expect(fiveHourCell(LEDGER, "claude-code", "api")).toBeNull();
    expect(fiveHourCell(LEDGER, "claude-code", "nobody")).toBeNull();
    expect(fiveHourCell(null, "claude-code", "work"), "没问过 ⇒ 照旧不出").toBeNull();
  });
});
