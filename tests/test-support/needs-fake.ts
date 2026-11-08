/**
 * 「需手动」那本账（`needs-book.ts`）的替身：本机后端那一问（`sessions-needs-merge`，它问到各台再合）只回录好的成品。
 *
 * 每台答什么由 `answers` 定（改了再 `refresh` / `again`；缺 ⇒ 空的整份；`{t: "offline"}` ＝ 那台常驻不在）。
 * 每一问的输入（问了哪几台 · 答了的那几台各自的整份）到金样 `tests/__fixtures__/needs-merged.golden.json` 里找同一组，
 * 交它的 `merged` —— 那一份由后端判据（`tests/backend/faces/needs_merge_tests.rs`）跑真的合并与扇出现打，
 * 先后 · 总数 · 没问到的那一句都是后端的，这里一处都不判。金样里没有这一组 ⇒ 抛（这一组同时记进本树
 * `.build/golden-missing/needs-merged.jsonl`）：照它在 `cases` 里添一条（`merged` 写 `null`），跑那条后端判据现打、bless 拷回。
 * 夹具只造结构（sid · 占位句子），不采真正文。
 */
import { appendFileSync, mkdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { NeedsBook } from "../../src/frontend/ui/needs-book";
import type { NeedsItem } from "../../src/frontend/ui/generated/NeedsItem";
import type { NeedsList } from "../../src/frontend/ui/generated/NeedsList";
import type { NeedsMerged } from "../../src/frontend/ui/generated/NeedsMerged";
import type { Needs } from "../../src/frontend/ui/generated/Needs";
import type { ManualItem } from "../../src/frontend/ui/generated/ManualItem";
import { LOCAL, type Machine } from "../../src/frontend/ui/ipc/origin";
import { liveFace } from "./session-faces";
import { REPO_ROOT } from "./repo-root";

/** 一条在等的会话那一项（线上那一形：界面收到时才盖的 `receivedAt` 不在线上，摘掉）。 */
export function sessionItem(sid: string, needs: Needs, label = sid): NeedsItem {
  const wire = { ...needs } as Needs & { receivedAt?: number };
  delete wire.receivedAt;
  return { kind: "session", rank: needs.rank, session: { sid, label, face: liveFace("needs_you"), needs: wire, hidden: null } };
}

/** 会话外的一项。 */
export function manualItem(manual: ManualItem): NeedsItem {
  return { kind: "manual", rank: 2e15, manual };
}

/** 一台怎么答：那一份整份 · 常驻不在。 */
export type NeedsRead = { t: "ok"; list: NeedsList } | { t: "offline" };

/** 一台的整份。 */
export function listOf(items: NeedsItem[], manualUnreadable?: string): NeedsRead {
  const list: NeedsList = { items, ended: [],...(manualUnreadable === undefined ? {} : { manualUnreadable }) };
  return { t: "ok", list };
}

/** 金样里的一条：输入原样 ＋ 后端现打的合并。 */
interface Case {
  asked: string[];
  answers: Record<string, NeedsList>;
  merged: NeedsMerged | null;
}

const GOLDEN = "tests/__fixtures__/needs-merged.golden.json";
const MISSING = ".build/golden-missing/needs-merged.jsonl";

/** 键排好再写（金样拷回时键序可能变，比的是内容）。 */
function canon(v: unknown): string {
  return JSON.stringify(v, (_k, x: unknown) =>
    x && typeof x === "object" && !Array.isArray(x) ? Object.fromEntries(Object.entries(x as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) : x,
  );
}

let cases: Case[] | null = null;

/** 这一组输入录好的合并；没录 ⇒ 抛。 */
export function recordedMerge(asked: readonly string[], answers: Record<string, NeedsList>): NeedsMerged {
  cases ??= (JSON.parse(readFileSync(path.join(REPO_ROOT, GOLDEN), "utf8")) as { cases: Case[] }).cases;
  const want = canon({ asked, answers });
  const hit = cases.find((c) => canon({ asked: c.asked, answers: c.answers }) === want);
  if (hit?.merged) return hit.merged;
  const line = JSON.stringify({ asked, answers, merged: null });
  try {
    mkdirSync(path.join(REPO_ROOT, path.dirname(MISSING)), { recursive: true });
    appendFileSync(path.join(REPO_ROOT, MISSING), line + "\n");
  } catch {
    // 记不下只少一份方便，照样抛。
  }
  throw new Error(`需手动替身：${GOLDEN} 里没录这一组输入（${hit ? "录了还没现打" : "没有"}）；添进 cases、跑 needs_merge_tests 现打后 bless：${line}`);
}

/**
 * 一本账 ＋ 记下每一问问了哪几台（`asked` 摊平）。本机后端问哪几台照它自己的机器表：替身里那张表就是 `answers` 的键（本机恒第一）。
 * `reply` 给了 ⇒ 这一问不查金样、照它回（测试自己摆的整份回包，收到的是答了的那几台各自的整份）。
 */
export function fakeNeedsBook(answers: Record<string, NeedsRead>, reply?: (m: { origin: Machine; list: NeedsList }[]) => NeedsMerged) {
  const asked: string[] = [];
  const book = new NeedsBook(() => {
    const machines: Machine[] = [LOCAL, ...Object.keys(answers).filter((o) => o !== LOCAL)];
    asked.push(...machines);
    const got: { origin: Machine; list: NeedsList }[] = [];
    for (const o of machines) {
      const a = answers[o] ?? listOf([]);
      if (a.t === "ok") got.push({ origin: o, list: a.list });
    }
    if (reply) return Promise.resolve(reply(got));
    try {
      return Promise.resolve(recordedMerge(machines, Object.fromEntries(got.map((g) => [g.origin, g.list]))));
    } catch (e) {
      return Promise.reject(e);
    }
  });
  return { book, asked, answers };
}
