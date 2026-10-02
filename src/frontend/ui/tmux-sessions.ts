/**
 * **这个会话此刻在那台哪个 tmux 会话里** —— 问那台后端（`sessions-tmux`），界面不判。
 *
 * 判定只在后端一处（带着它的 `@ccm_sid`、前台是不是 agent；没打上标记的不按目录猜）：单个菜单亮哪几项、
 * 批量停 / 起、换号重启找旧会话、分叉找源会话都读这一份答案。本文件只做调用方那一侧：发 · 按形状收 · 失败说成一句。
 * 从前这里是界面拿 tmux 名单自己筛的那几个过滤（`findClaudeTmux` · `findClaudeTmuxMatches` · `findIdleTmux` · `isCwdFallbackMatch`〔散文墓碑〕），连同按目录猜那一支一起删了。
 */
import type { TmuxSession } from "./tmux-reads";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import { exactKeys, isObj, machineName, settle, unreadable, type Refusals } from "./control-said";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

export type { TmuxSession };

/**
 * 一个会话的样子：`running` 恰好一个在跑（`names[0]`）· `ambiguous` 在跑的不止一个（按名单顺序）·
 * `idle` 没有在跑的、有带着它的空 tmux（`names[0]`）· `none` 没有哪个会话带着它 · `no_tmux` 那台没装 tmux。
 */
export interface Standing {
  kind: "running" | "ambiguous" | "idle" | "none" | "no_tmux";
  names: string[];
}

const KINDS = new Set(["running", "ambiguous", "idle", "none", "no_tmux"]);

/** 期限：那台问一次 tmux 名单就答（同 `tmux-list`）。 */
const STANDING_BUDGET_MS = 10_000;

function refusals(origin: Origin): Refusals {
  return {
    byCode: (_code, detail) => copyText("tabBatch.why.batchRefused", { machine: machineName(origin), detail }),
    noReason: () => copyText("tabBatch.why.batchRefused", { machine: machineName(origin), detail: "" }),
  };
}

/** 应答 ⇒ 逐个样子；形状不对（多一格缺一格 · 个数 / 次序不符）⇒ 抛。 */
export function decodeStandings(origin: Origin, sids: readonly string[], v: unknown): Standing[] {
  const bad = (): never => {
    throw unreadable(origin, "sessions-tmux", "is not exactly {results: [{sid, standing, names}]} in request order");
  };
  if (!isObj(v) || !exactKeys(v, ["results"]) || !Array.isArray(v.results) || v.results.length !== sids.length) bad();
  return (v as { results: unknown[] }).results.map((r, i) => {
    if (
      !isObj(r) ||
      !exactKeys(r, ["sid", "standing", "names"]) ||
      r.sid !== sids[i] ||
      typeof r.standing !== "string" ||
      !KINDS.has(r.standing) ||
      !Array.isArray(r.names) ||
      !r.names.every((n) => typeof n === "string")
    ) {
      return bad();
    }
    return { kind: r.standing as Standing["kind"], names: r.names as string[] };
  });
}

/** 那台上这几个会话各自的样子（与 `sids` 同序）。问不到 / 形状不对 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function standingsOf(origin: Origin, sids: readonly string[]): Promise<Standing[]> {
  const body = jsonBody({ sids });
  const budget = budgetWithin(STANDING_BUDGET_MS);
  const v = await settle(origin, "sessions-tmux", chan.call(origin, "sessions-tmux", body, budget), refusals(origin));
  return decodeStandings(origin, sids, v);
}

/** 一个会话的样子；问不到 ⇒ `undefined`（不知道，不是「不在」）。 */
export async function standingOf(origin: Origin, sid: string): Promise<Standing | undefined> {
  try {
    return (await standingsOf(origin, [sid]))[0];
  } catch {
    return undefined;
  }
}
