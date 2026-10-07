/**
 * **这个会话此刻在那台哪个 tmux 会话里** —— 问那台后端（`sessions-where`），界面不判。
 *
 * 判定只在后端一处（带着它的 `@ccm_sid`、前台是不是 agent；没打上标记的不按目录猜）：单个菜单亮哪几项、
 * 批量停 / 起、换号重启找旧会话都读这一份答案（分叉找源会话那一问在那台后端做）。本文件只做调用方那一侧：发 · 按形状收 · 失败说成一句。
 * 界面不拿名单自己筛、不按目录猜。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import { machineName, settle, unreadable, type Refusals } from "./control-said";
import { exactKeys, isObj } from "./ipc/decode";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { noteMachineTmux } from "./resume-defaults";

/**
 * 一个会话的样子：`running` 恰好一个在跑（`names[0]`）· `ambiguous` 在跑的不止一个（按名单顺序）·
 * `idle` 没有在跑的、有带着它的空 tmux（`names[0]`）· `none` 没有哪个会话带着它 · `no_tmux` 那台没装 tmux。
 * `terminals` 与 `names` 同序同数：各自的终端句柄（预览按它指，不按 tmux 名）。
 */
export interface Standing {
  kind: "running" | "ambiguous" | "idle" | "none" | "no_tmux";
  names: string[];
  terminals: string[];
}

const KINDS = new Set(["running", "ambiguous", "idle", "none", "no_tmux"]);

/** 期限：那台问一次 tmux 名单就答。 */
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
    throw unreadable(origin, "sessions-where", "is not exactly {results: [{sid, standing, names, terminals}]} in request order");
  };
  if (!isObj(v) || !exactKeys(v, ["results"]) || !Array.isArray(v.results) || v.results.length !== sids.length) bad();
  return (v as { results: unknown[] }).results.map((r, i) => {
    if (
      !isObj(r) ||
      !exactKeys(r, ["sid", "standing", "names", "terminals"]) ||
      r.sid !== sids[i] ||
      typeof r.standing !== "string" ||
      !KINDS.has(r.standing) ||
      !Array.isArray(r.names) ||
      !r.names.every((n) => typeof n === "string") ||
      !Array.isArray(r.terminals) ||
      r.terminals.length !== r.names.length ||
      !r.terminals.every((t) => isObj(t) && exactKeys(t, ["host", "terminal"]) && typeof t.host === "string" && typeof t.terminal === "string")
    ) {
      return bad();
    }
    const terminals = (r.terminals as { terminal: string }[]).map((t) => t.terminal);
    const kind = r.standing as Standing["kind"];
    // 那台答的就是它有没有 tmux（没有 ⇒ `no_tmux`；有带着它的 tmux 会话 ⇒ 有）：恢复菜单默认的「运行于」照它回落。
    if (kind === "no_tmux") noteMachineTmux(origin, false);
    else if (kind !== "none") noteMachineTmux(origin, true);
    return { kind, names: r.names as string[], terminals };
  });
}

/** 那台上这几个会话各自的样子（与 `sids` 同序）。问不到 / 形状不对 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function standingsOf(origin: Origin, sids: readonly string[]): Promise<Standing[]> {
  const body = jsonBody({ sids });
  const budget = budgetWithin(STANDING_BUDGET_MS);
  const v = await settle(origin, "sessions-where", chan.call(origin, "sessions-where", body, budget), refusals(origin));
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
