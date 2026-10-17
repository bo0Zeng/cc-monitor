/**
 * 额度几处界面的排版模型：状态栏按钮各态 · 悬停卡会话补上的那一样。期望手写。
 * 输入：一个会话的 `rotation-view`（开会话的号 · 在用的号）＋ 那台的 `quota-read`（用量那一格照在用的号那一条）。
 * 悬停卡里号那一段照抄 `quota-read` 每号的 `rows`（行模型住核心 `faces/quota_rows.rs`，金样 quota-text.golden.json 锁），这里只验采样那台。
 */
import { describe, expect, it } from "vitest";

import { sessionChip, sessionHoverRows } from "../../../src/frontend/ui/acct-view.ts";
import type { SessionRotationEntry } from "../../../src/frontend/ui/app-store.ts";
import { takeNames, type QuotaRead } from "../../../src/frontend/ui/acct-words.ts";
// 号名 / 位名照核心写好的那张表（后端 `accounts/quota/name_words.rs` 真写的金样）。
import NAMES from "../../__fixtures__/names.golden.json";
import type { QuotaShow } from "../../../src/frontend/ui/generated/QuotaShow.ts";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";

// 时刻的字由后端写好（`…Text`），这里照回包的样子手写一份；界面不换算。
const NOW = 1_791_189_600;

function quota(p: Partial<QuotaShow> = {}): QuotaShow {
  return { kind: "sub", state: "ok", stale: false, limiting: "5h", usage: { slot: "5h", window: "5h", value: "63%", text: "5h 63%", tone: "plain" }, slots: [{ slot: "5h", pct: 63, resetsAt: NOW + 6600, resetsAtText: "13:50", text: "63%", tone: "plain" }, { slot: "7d", pct: 41, resetsAt: NOW + 3 * 86400, resetsAtText: "10-07 12:00", text: "41%", tone: "plain" }], login: "ok", ...p };
}

/** 一个会话的 `rotation-view`：这几处只看 `start` · `current`。 */
function entry(start: string, current: string): SessionRotationEntry {
  const read = { now: NOW, nowText: "14:30", title: null, strategy: null, board: { all: [], rows: [] }, start, current, lineup: [], off: [], accounts: [], listed: 0, decision: null, timeline: null, past: null, log: [], hints: [] } satisfies RotationViewReply;
  return { origin: "<local>", read };
}

const LEDGER: QuotaRead = {
  state: "present",
  reason: null,
  path: null,
  now: NOW,
  accounts: [
    { agent: "claude", account: "personal", seenAt: NOW - 120, seenAtText: "11:58", reading: { refused: false, windows: [] }, rows: [["personal", copyText("acct.kind.sub")], ["5h", "63%", "↻13:50", "+1h50m"], ["7d", "41%", "↻10-07 12:00", "+3d"], [copyText("acct.row.over"), "—"], [copyText("acct.row.seen"), "11:58"]].map((r) => r.map((text) => ({ text, tone: "plain" as const }))), warm: { act: "wait" as const, text: "" }, fiveHour: "5h 63%", ...quota() },
    { agent: "claude", account: "team", seenAt: NOW - 120, seenAtText: "11:58", reading: { refused: false, windows: [] }, rows: [], warm: { act: "wait" as const, text: "" }, fiveHour: "5h 4%", ...quota({ slots: [{ slot: "5h", pct: 4, resetsAt: NOW + 5400, resetsAtText: "13:30", text: "4%", tone: "plain" }], usage: { slot: "5h", window: "5h", value: "4%", text: "5h 4%", tone: "plain" } }) },
    { agent: "claude", account: "api", seenAt: NOW - 120, seenAtText: "11:58", reading: { refused: true, windows: [], resetsAt: NOW + 720, resetsAtText: "12:12" }, rows: [], warm: { act: "wait" as const, text: "" }, fiveHour: null, ...quota({ kind: "api", state: "refused", slots: [], limiting: undefined, usage: { value: copyText("reason.code.refused"), reset: "↻12:12", text: "↻12:12", tone: "fail" } }) },
  ],
  unseen: [],
  usableNow: ["personal", "team"],
  candidates: [],
  earliestReturn: null,
  detail: null,
  text: null,
  fiveHour: null,
  names: { accounts: {}, slots: {} },
  verdict: null,
};

const ledger = (p: Partial<QuotaShow>): QuotaRead => ({ ...LEDGER, accounts: LEDGER.accounts.map((a) => (a.account === "personal" ? { ...a, ...quota(p) } : a)) });

describe("状态栏账号按钮：在用的号照 rotation-view，用量那一格照抄核心写好的", () => {
  const chip = (e: SessionRotationEntry, q: QuotaRead = LEDGER) => sessionChip(e, null, q)!;
  it("★ 号照 `current`；窗口 · 值 · 重置 · 语气都照 `quota-read` 里那个号的 `usage`；换过号（`current` ≠ `start`）带 ⇄", () => {
    expect(chip(entry("personal", "personal"))).toMatchObject({ account: "personal", name: "personal", window: "5h", value: "63%", reset: null, tone: "plain", swapped: false });
    const refused = chip(entry("personal", "personal"), ledger({ state: "refused", usage: { slot: "5h", window: "5h", value: "✕", reset: "↻13:00", text: "5h ✕ ↻13:00", tone: "fail" } }));
    expect(refused).toMatchObject({ window: "5h", value: "✕", reset: "↻13:00", tone: "fail" });
    const sw = chip(entry("work", "team"));
    expect(sw).toMatchObject({ account: "team", swapped: true, value: "4%" });
    expect(chip(entry("personal", "personal"), ledger({ stale: true })).stale).toBe(true);
    expect(chip(entry("work", "nobody"))).toMatchObject({ account: "nobody", value: null, tone: "plain" });
  });
  it("中转没见过这个会话 ⇒ 只画归属的号、无用量；什么都不知道 ⇒ 不画；`_` 照核心那张表写", () => {
    const absent: SessionRotationEntry = { origin: "<local>", read: null };
    expect(sessionChip(absent, "work", LEDGER)).toMatchObject({ name: "work", value: null });
    expect(sessionChip(undefined, null, LEDGER)).toBeNull();
    takeNames(NAMES);
    expect(sessionChip(entry("_", "_"), null, LEDGER)?.name).toBe(NAMES.accounts._);
  });
});

describe("悬停卡：会话补上的那一样", () => {
  it("★ 在用的号那一段照 `quota-read`；采样那一格后面接上是哪台采的", () => {
    const rows = sessionHoverRows(entry("work", "personal"), null, LEDGER)!;
    expect(rows[0]).toEqual(["personal", copyText("acct.kind.sub")]);
    expect(rows.map((r) => r[0])).toEqual(["personal", "5h", "7d", copyText("acct.row.over"), copyText("acct.row.seen"), copyText("rot.hover.from", { account: "work" })]);
    expect(rows[4]).toEqual([copyText("acct.row.seen"), "11:58", copyText("acct.machine.here")]);
  });
  it("★ 轮换换过号 ⇒ 末尾一行「从哪个号换过来」（开会话的号）；没换过 ⇒ 没有这一行", () => {
    const swapped = sessionHoverRows(entry("work", "personal"), null, LEDGER)!;
    expect(swapped[swapped.length - 1]).toEqual([copyText("rot.hover.from", { account: "work" })]);
    const same = sessionHoverRows(entry("personal", "personal"), null, LEDGER)!;
    expect(same.flat()).not.toContain(copyText("rot.hover.from", { account: "personal" }));
  });
  it("中转没见过 ⇒ `用量 — 未实时显示` · `切换 仅重启切换`", () => {
    const absent: SessionRotationEntry = { origin: "<local>", read: null };
    expect(sessionHoverRows(absent, "personal", LEDGER)).toEqual([["personal", copyText("acct.kind.sub")], [copyText("acct.hover.usage"), "—", copyText("acct.hover.noRelay")], [copyText("acct.hover.switch"), copyText("acct.hover.restartOnly")]]);
  });
});
