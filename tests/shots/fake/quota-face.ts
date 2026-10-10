/**
 * 假后端替真后端出口补 `quota-read` 的成品格（真的那一处：`show.rs::slot_words` · `faces/quota_rows.rs` · `common::time::with_texts` 的距今）：
 * 语义位那一格的字（场景没写才补，照 `slot_words` 的先后）· 每号几行 `rows` · 开窗 `warm`。
 * 距今 `…RelText` 在 `backend.ts::withTexts` 那一遍补。截图只要长得像；字的对错由后端那几条判据管（金样 quota-text.golden.json），不在这里。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";

type Obj = Record<string, unknown>;
const cell = (text: string, tone = "plain") => ({ text, tone });
export const relText = (at: number, now: number): string | null => {
  const d = at - now;
  if (d <= 0) return null;
  if (d >= 86_400) return `+${Math.floor(d / 86_400)}d`;
  const m = Math.ceil(d / 60);
  return m < 60 ? `+${m}m` : m % 60 === 0 ? `+${Math.floor(m / 60)}h` : `+${Math.floor(m / 60)}h${m % 60}m`;
};

function slotRow(a: Obj, slot: string, now: number): Obj[] {
  const label = cell(slot === "5h" ? copyText("acct.slot.fiveHour") : copyText("acct.slot.sevenDay"));
  const x = (a.slots as Obj[] | undefined)?.find((s) => s.slot === slot);
  if (!x) return [label, cell(copyText("acct.val.none"))];
  const row = [label, cell(String(x.text), String(x.tone))];
  if (typeof x.resetsAt === "number") {
    const r = relText(x.resetsAt, now);
    row.push(cell(copyText(r === null ? "acct.reset.past" : "acct.reset.at", { at: String(x.resetsAtText ?? "") })));
    if (r !== null) row.push(cell(r));
  }
  return row;
}

export function quotaFace(v: unknown): unknown {
  const q = v as Obj;
  const now = Number(q.now);
  for (const a of (q.accounts as Obj[] | undefined) ?? []) {
    const head = [cell(a.account === "_" ? copyText("acct.home.name") : String(a.account)), cell(copyText(a.kind === "api" ? "acct.kind.api" : "acct.kind.sub"))];
    a.rows ??= a.kind === "api"
      ? [head, [cell(copyText("acct.slot.both")), cell(copyText("acct.val.none")), cell(copyText("acct.val.noLimit"))], [cell(copyText("acct.row.seen")), cell(String(a.seenAtText ?? ""))]]
      : [head, slotRow(a, "5h", now), slotRow(a, "7d", now), [cell(copyText("acct.row.over")), cell(copyText("acct.val.none"))], [cell(copyText("acct.row.seen")), cell(String(a.seenAtText ?? ""))]];
    a.warm ??= { act: "wait", text: "" };
  }
  for (const u of (q.unseen as Obj[] | undefined) ?? []) {
    u.rows ??= [[cell(String(u.account)), cell(copyText(u.kind === "api" ? "acct.kind.api" : "acct.kind.sub"))], [cell(copyText("acct.slot.fiveHour")), cell(copyText("acct.val.none")), cell(copyText("acct.seen.none"))]];
    u.warm ??= { act: "send", text: "" };
  }
  return q;
}

/** 回包里每一处语义位那一格（`slots[]`）场景没写字就补上（照 `slot_words` 的先后，按同一对象上的 `state` · `limiting`）。 */
export function slotWords(v: unknown): unknown {
  if (Array.isArray(v)) v.forEach(slotWords);
  else if (v !== null && typeof v === "object") {
    const o = v as Obj;
    if (Array.isArray(o.slots))
      for (const x of o.slots as Obj[]) {
        // 照 `show.rs::slot_words` 的先后：卡人的那一格按显示态换字，其余看用满与百分比。
        const here = (o.limiting ?? "5h") === x.slot;
        const pct = x.pct === undefined ? null : String(x.pct);
        const [text, tone] =
          here && o.state === "overageInUse" ? [copyText("acct.val.over"), "warn"]
          : here && (o.state === "resetSinceSeen" || o.state === "unseen") ? [copyText("acct.val.none"), "plain"]
          : x.full ? [copyText("acct.val.full"), "fail"]
          : here && o.state === "refused" ? [pct === null ? copyText("acct.val.refusedOnly") : copyText("acct.val.refusedPct", { pct }), "fail"]
          : pct === null ? [copyText("acct.val.none"), "plain"]
          : [copyText("acct.val.pct", { pct }), here && o.state === "near" ? "warn" : "plain"];
        x.text ??= text;
        x.tone ??= tone;
      }
    Object.values(o).forEach(slotWords);
  }
  return v;
}
