/**
 * 额度几处界面的排版模型：状态栏按钮各态 · 悬停卡会话那两样 · 记录原因 · 提示条 · 标签页 `✕ 5h`。期望手写（照额度稿修订 §1 那张表）。
 * 悬停卡里号那一段与金样同一个行模型（`quota-lines.vitest.ts` 锁），这里只验会话补上的两样。
 */
import { describe, expect, it } from "vitest";

import { bannerOf, sessionChip, sessionHoverRows, tabBlockedOf, whyOf } from "../../../src/frontend/ui/acct-view.ts";
import type { SessionRotationEntry } from "../../../src/frontend/ui/app-store.ts";
import type { QuotaRead } from "../../../src/frontend/ui/quota-lines.ts";
import type { QuotaShow } from "../../../src/frontend/ui/generated/QuotaShow.ts";
import type { SessionRotation } from "../../../src/frontend/ui/generated/SessionRotation.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../test-support/copy-pattern";

// 本地时区由 vitest 进程决定 ⇒ 时刻都取「此刻 ＋ 整小时」，只断言不依赖时区的那几格（值 · 窗口 · 色 · 距今）。
const NOW = 1_791_189_600;

function quota(p: Partial<QuotaShow> = {}): QuotaShow {
  return { kind: "sub", state: "ok", stale: false, limiting: "5h", slots: [{ slot: "5h", pct: 63, resetsAt: NOW + 6600 }, { slot: "7d", pct: 41, resetsAt: NOW + 3 * 86400 }], login: "ok", ...p };
}

function entry(p: Partial<SessionRotation> = {}, q: Partial<QuotaShow> = {}): SessionRotationEntry {
  return {
    origin: "<local>",
    now: NOW,
    read: {
      state: "present",
      agent: "claude-code",
      follow: true,
      account: { start: "personal", current: "personal", since: NOW - 600, history: [], inPlace: "ok" },
      atLimit: "continue",
      quota: quota(q),
      ...p,
    },
  };
}

const LEDGER: QuotaRead = {
  state: "present",
  reason: null,
  path: null,
  now: NOW,
  accounts: [
    { agent: "claude-code", account: "personal", seenAt: NOW - 120, reading: {}, ...quota() },
    { agent: "claude-code", account: "team", seenAt: NOW - 120, reading: {}, ...quota({ slots: [{ slot: "5h", pct: 4, resetsAt: NOW + 5400 }] }) },
    { agent: "claude-code", account: "api", seenAt: NOW - 120, reading: { resetsAt: NOW + 720 }, ...quota({ kind: "api", state: "refused", slots: [], limiting: undefined }) },
  ],
  unseen: [],
  usableNow: ["personal", "team"],
  earliestReturn: null,
};

describe("状态栏账号按钮：各态", () => {
  const chip = (e: SessionRotationEntry) => sessionChip(e, null, LEDGER)!;
  it("正常 `personal 5h 63%` · 中性；换过号带 ⇄", () => {
    expect(chip(entry())).toMatchObject({ name: "personal", window: "5h", value: "63%", reset: null, tone: "neutral", swapped: false });
    const sw = entry({ account: { start: "work", current: "personal", since: NOW - 600, history: [], inPlace: "ok" } });
    expect(chip(sw).swapped).toBe(true);
  });
  it("到阈值数字琥珀 · 用满 ✕ / 被拒没用满 `58% · 被拒`，都红带 ↻ · 超额琥珀 · 无数 `—` · 数旧降透明度", () => {
    expect(chip(entry({}, { state: "near", slots: [{ slot: "5h", pct: 86 }] }))).toMatchObject({ value: "86%", tone: "near" });
    const refused = chip(entry({}, { state: "refused", slots: [{ slot: "5h", pct: 100, resetsAt: NOW + 3600, full: true }] }));
    expect(refused).toMatchObject({ value: "✕", tone: "refused" });
    expect(refused.reset).toMatch(/^↻\d\d:\d\d$/);
    const brief = chip(entry({}, { state: "refused", slots: [{ slot: "5h", pct: 58, resetsAt: NOW + 3600 }] }));
    expect(brief).toMatchObject({ value: copyText("acct.val.refusedPct", { pct: "58" }), tone: "refused" });
    expect(chip(entry({}, { state: "refused", slots: [] })).value).toBe(copyText("acct.val.refusedOnly"));
    expect(chip(entry({}, { slots: [{ slot: "5h", pct: 103, full: true }] }))).toMatchObject({ value: "✕", tone: "refused" });
    expect(chip(entry({}, { state: "overageInUse" }))).toMatchObject({ value: copyText("acct.val.over"), tone: "over" });
    expect(chip(entry({}, { state: "resetSinceSeen" })).value).toBe("—");
    expect(chip(entry({}, { state: "unseen", slots: [], limiting: undefined })).value).toBe("—");
    expect(chip(entry({}, { stale: true })).stale).toBe(true);
  });
  it("按量 `按量`；按量被拒 `被拒 ↻..`（时刻取额度账那一条）", () => {
    expect(chip(entry({}, { kind: "api", slots: [], limiting: undefined }))).toMatchObject({ window: null, value: copyText("acct.kind.api"), tone: "neutral" });
    const e = entry({ account: { start: "api", current: "api", since: NOW, history: [], inPlace: "ok" } }, { kind: "api", state: "refused", slots: [], limiting: undefined });
    expect(chip(e)).toMatchObject({ value: copyText("acct.val.refusedOnly"), tone: "refused" });
    expect(chip(e).reset).toMatch(/^↻/);
  });
  it("中转没见过这个会话 ⇒ 只画归属的号、无用量；什么都不知道 ⇒ 不画；`_` 写 ~/.claude", () => {
    const absent: SessionRotationEntry = { origin: "<local>", now: NOW, read: { state: "absent", inPlace: "noRelay" } };
    expect(sessionChip(absent, "work", LEDGER)).toMatchObject({ name: "work", value: null });
    expect(sessionChip(undefined, null, LEDGER)).toBeNull();
    expect(sessionChip(entry({ account: { start: "_", current: "_", since: NOW, history: [], inPlace: "ok" } }), null, null)?.name).toBe("~/.claude");
  });
});

describe("悬停卡：会话补上的两样", () => {
  it("首行尾 `⇄ 时刻 ← 原号`；`下一个  team  5h 4%` 插在采样之前", () => {
    const e = entry({
      account: { start: "work", current: "personal", since: NOW - 600, history: [{ at: NOW - 600, from: "work", to: "personal", why: { full: { w: "5h" } } }], inPlace: "ok" },
      next: "team",
    });
    const rows = sessionHoverRows(e, null, LEDGER)!;
    expect(rows[0].slice(0, 2)).toEqual(["personal", copyText("acct.kind.sub")]);
    expect(rows[0][2]).toMatch(/^⇄ \d\d:\d\d ← work$/);
    expect(rows.map((r) => r[0])).toEqual(["personal", "5h", "7d", copyText("acct.row.over"), copyText("acct.hover.next"), copyText("acct.row.seen")]);
    expect(rows[4]).toEqual([copyText("acct.hover.next"), "team", "5h 4%"]);
  });
  it("没有下一个 ⇒ `下一个  —  轮换内无未满`；中转没见过 ⇒ `用量 — 未实时显示` · `切换 仅重启切换`", () => {
    expect(sessionHoverRows(entry(), null, LEDGER)!.find((r) => r[0] === copyText("acct.hover.next"))).toEqual([copyText("acct.hover.next"), "—", copyText("acct.hover.nextNone")]);
    const absent: SessionRotationEntry = { origin: "<local>", now: NOW, read: { state: "absent", inPlace: "noRelay" } };
    expect(sessionHoverRows(absent, "personal", LEDGER)).toEqual([["personal", copyText("acct.kind.sub")], [copyText("acct.hover.usage"), "—", copyText("acct.hover.noRelay")], [copyText("acct.hover.switch"), copyText("acct.hover.restartOnly")]]);
  });
});

describe("记录 · 提示条 · 标签页", () => {
  it("原因：满 `work 5h ✕` ＋ 那一刻原号几点重置；手动；跳过；停发", () => {
    expect(whyOf({ at: NOW, from: "work", to: "personal", why: { full: { w: "5h" } }, fromResetsAt: NOW + 3600 }, NOW, 0)).toEqual({ why: "work 5h ✕", reset: expect.stringMatching(/^↻/) });
    expect(whyOf({ at: NOW, from: "a", to: "b", why: "manualHot" }, NOW, 0).why).toBe(copyText("acct.hist.manualHot"));
    expect(whyOf({ at: NOW, from: "a", to: "a", why: { skipped: { account: "team", reason: "needsLogin" } } }, NOW, 0).why).toBe(copyText("acct.hist.skipped", { name: "team", reason: copyText("acct.reason.needsLogin", { name: "team" }) }));
    expect(whyOf({ at: NOW, from: "a", to: "a", why: { held: { n: 90 } } }, NOW, 0).why).toBe(copyText("acct.hist.held", { name: "a", n: "90" }));
  });
  it("原因：段预算用完 `5h 段预算 +5`；前面的号回来 `z 已恢复`（名是换去的那个号）", () => {
    expect(whyOf({ at: NOW, from: "b", to: "c", why: { stint: { w: "5h", n: 5 } } }, NOW, 0)).toEqual({ why: copyText("acct.hist.stint", { w: "5h", n: "5" }), reset: null });
    expect(whyOf({ at: NOW, from: "b", to: "z", why: "preempt" }, NOW, 0)).toEqual({ why: copyText("acct.hist.preempt", { name: "z" }), reset: null });
  });
  it("卡住：单号 · 全满 · 硬上限 · 时刻已过；能发 ⇒ 不出", () => {
    expect(bannerOf(entry(), NOW)).toBeNull();
    const single = entry({ blocked: { earliest: { account: "personal", at: NOW + 5400 } } }, { state: "refused" });
    expect(bannerOf(single, NOW)?.text).toMatch(/^personal 5h ✕ ↻\d\d:\d\d \(\+1h30m\)$/);
    const all = entry({ blocked: { earliest: { account: "team", at: NOW + 5400 } } }, { state: "refused" });
    expect(bannerOf(all, NOW)?.text).toMatch(copyPattern("acct.banner.allFull", { name: "team", rel: "1h30m" }, { whole: true }));
    const held = entry({ blocked: { earliest: { account: "team", at: NOW + 600 } }, account: { start: "personal", current: "personal", since: NOW, history: [{ at: NOW, from: "personal", to: "personal", why: { held: { n: 90 } } }], inPlace: "ok" } });
    expect(bannerOf(held, NOW)?.text).toMatch(copyPattern("acct.banner.held", { n: 90, name: "team" }, { whole: true }));
    expect(bannerOf(all, NOW + 6000)).toEqual({ text: expect.stringMatching(/^team ↻\d\d:\d\d 已过 · 可续发$/), tone: "neutral" });
  });
  it("标签页：被卡 ⇒ `✕ 5h` ＋ 悬停一行；能发 ⇒ 不出", () => {
    expect(tabBlockedOf(entry())).toBeNull();
    const b = tabBlockedOf(entry({ blocked: { earliest: { account: "team", at: NOW + 600 } } }, { state: "refused" }));
    expect(b?.text).toBe("✕ 5h");
    expect(b?.hover).toMatch(copyPattern("acct.tab.hover", { w: "5h", name: "team" }, { whole: true }));
  });
});
