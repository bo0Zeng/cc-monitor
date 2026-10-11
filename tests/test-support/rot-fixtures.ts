/**
 * 判据用的「会话那一份轮换」（`rotation-session-read` 里 `state: "present"` 那一形）：缺省一份平常的，判据只覆盖要的那几格。
 * 字都是合成的占位（不放真正文）；后端写好的那几句（为什么 · 描述 · 卡住）用 `W-` `D-` 开头的记号。
 */
import type { SessionRotationState } from "../../src/frontend/ui/generated/SessionRotationState";
import type { SessionRotation } from "../../src/frontend/ui/generated/SessionRotation";
import type { QuotaShow } from "../../src/frontend/ui/generated/QuotaShow";
import type { RotationRow } from "../../src/frontend/ui/generated/RotationRow";

export type Present = Extract<SessionRotationState, { state: "present" }>;

export const quotaShow = (p: Partial<QuotaShow> = {}): QuotaShow => ({
  kind: "sub",
  state: "ok",
  stale: false,
  limiting: "5h",
  slots: [{ slot: "5h", pct: 30, text: "30%", tone: "plain" }],
  login: "ok",
  usage: { slot: "5h", window: "5h", value: "30%", text: "5h 30%", tone: "plain" },
  ...p,
} as QuotaShow);

export const row = (account: string, p: Partial<RotationRow> = {}): RotationRow => ({
  account,
  why: `W-${account}`,
  alsoUsed: 0,
  facts: { id: account, kind: "sub", ok: true, w5h: { open: true, used: 30, resets: null }, w7d: { open: true, used: 10, resets: null }, others: 0 },
  ...p,
});

export function present(p: Partial<SessionRotation> = {}): Present {
  return {
    state: "present",
    agent: "claude-code",
    start: "work",
    current: "work",
    since: 1_791_189_000,
    history: [],
    inPlace: "ok",
    // 线上的 `rev` 是数（生成的类型此刻写成 bigint，等后端那一格配上 `ts(type = "number")`）。
    config: { id: "c2", name: "C-two", rev: 3 as unknown as bigint, dirty: false },
    text: "D-desc",
    strategy: { id: "2", name: "S-two", ordered: false },
    values: {
      params: { grab_min: 30, delay: 0, all_out: "stop" },
      accounts: [
        { who: "@start", params: { line: 90, slots: [], mode: "normal" } },
        { who: "@rest", params: { line: 90, slots: [], mode: "normal" } },
        { who: "team", params: { line: 99, slots: [], mode: "normal" } },
        { who: "api", params: { line: 90, slots: [], mode: "standby" } },
      ],
      acctInit: { line: 90, slots: [], mode: "normal" },
    },
    rows: [row("work"), row("team"), row("lab", { fromRest: true }), row("api", { facts: { id: "api", kind: "api", ok: true, w5h: { open: false, used: 0, resets: null }, w7d: { open: false, used: 0, resets: null }, others: 0 } })],
    quota: quotaShow(),
    ...p,
  };
}
