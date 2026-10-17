/**
 * 「需手动」那本账（`needs-book.ts`）的替身：每台答什么由 `answers` 定（改了再 `refresh` / `again`）；
 * 本机合一份那一问换成手上的拼法（本机在前、再照交进来的先后，`total` ＝ 项数），合的判法另有后端判据管。
 * 夹具只造结构（sid · 占位句子），不采真正文。
 */
import { NeedsBook, type NeedsRead } from "../../src/frontend/ui/needs-book";
import type { NeedsItem } from "../../src/frontend/ui/generated/NeedsItem";
import type { NeedsList } from "../../src/frontend/ui/generated/NeedsList";
import type { NeedsMerged } from "../../src/frontend/ui/generated/NeedsMerged";
import type { Needs } from "../../src/frontend/ui/generated/Needs";
import type { ManualItem } from "../../src/frontend/ui/generated/ManualItem";
import { isLocalOrigin, type Origin } from "../../src/frontend/ui/ipc/origin";
import { liveFace } from "./session-faces";

/** 一条在等的会话那一项。 */
export function sessionItem(sid: string, needs: Needs, label = sid): NeedsItem {
  return { kind: "session", rank: needs.rank, session: { sid, label, face: liveFace("needs_you"), needs, hidden: null } };
}

/** 计划里一格。 */
export function planItem(workspace: string, slice: string, k: number): NeedsItem {
  return { kind: "plan", rank: 1e15, workspace, slice, k };
}

/** 会话外的一项。 */
export function manualItem(manual: ManualItem): NeedsItem {
  return { kind: "manual", rank: 2e15, manual };
}

/** 一台的整份。 */
export function listOf(items: NeedsItem[], manualUnreadable?: string): NeedsRead {
  const list: NeedsList = { items, answered: [], ...(manualUnreadable === undefined ? {} : { manualUnreadable }) };
  return { t: "ok", list };
}

/** 合一份（替身）：本机在前、再照交进来的先后。 */
export function fakeMerge(machines: { origin: Origin; list: NeedsList }[]): NeedsMerged {
  const sorted = [...machines].sort((a, b) => Number(isLocalOrigin(b.origin)) - Number(isLocalOrigin(a.origin)));
  const items = sorted.flatMap((m) => m.list.items.map((item) => ({ origin: m.origin, item })));
  const unreadable = sorted.flatMap((m) => (m.list.manualUnreadable ? [{ origin: m.origin, said: m.list.manualUnreadable }] : []));
  return { items, total: items.length, unreadable };
}

/** 一本账 ＋ 记下问了哪几台 · 合了几回。 */
export function fakeNeedsBook(answers: Record<string, NeedsRead>, merge: (m: { origin: Origin; list: NeedsList }[]) => NeedsMerged = fakeMerge) {
  const asked: string[] = [];
  const merges: { origin: Origin; list: NeedsList }[][] = [];
  const book = new NeedsBook(
    (o) => {
      asked.push(o);
      return Promise.resolve(answers[o] ?? listOf([]));
    },
    (m) => {
      merges.push(m);
      return Promise.resolve(merge(m));
    },
  );
  return { book, asked, merges, answers };
}
