/**
 * **起会话时那个 tmux 会话名从哪来 —— 本机与远端同一个家。**
 *
 * 守的要求：「**一个判定只有一个家**」＋ D4「**一条都不许静默忽略**」；
 * 「tmux 名派生 ＋ 撞名避让只留后端」。
 *
 * # 名字是那台后端铸的
 *
 * 「从 cwd 派生基名 → 撞名避让」原先前端一份（`remote-launch.ts` 的 `deriveTmuxName` · `mintTmuxName` ·
 * `mintSessionTmuxName`，分叉那条 `fork-launch.ts::forkTmuxName`）、后端一份（`control/ccm/plan.rs`，终端里敲 `ccm` 走它），
 * 两边逐字同规则、跨语言对拍钉着。前端那份删了：要名字就问**那台**后端的 `tmux-name-mint`
 * （`{cwd}` ⇒ `<项目名>-cc` · `{forkOf}` ⇒ `<…>-fork-cc`，按那台那张会话快照避让）—— 规则与名单都在那一台，
 * 于是界面铸出来的名字与终端里在同一目录敲 `ccm` 铸的是同一个。
 *
 * # 规则只有一条：问不到 ⇒ 不铸名
 *
 * 问不到（链路断 · 那台后端比这一问老 · 回的形状不认）⇒ [`MintOutcome`] `ok:false` 带原因，**不许**自己拼一个名字顶上 ——
 * 不避让的名字就是 issue #76 的形状：同一个 cwd 派生出同一个名字，撞上已有会话，**静默接进第一个会话，而用户以为开了新的**。
 * 「不铸名」之后怎么办归调用方：本机把 `null` 交给后端（它有一条如实的「不进容器」旧路）；
 * 远端**没有**不进 tmux 的起法 ⇒ [`refuseUnmintable`]：不起，说清是哪台、为什么。
 *
 * 分叉那条要知道源会话此刻在哪个 tmux 里，那是另一问（问那台后端 `sessions-tmux`，`tmux-sessions.ts::standingOf`）。
 */
import type { Origin } from "./ipc/origin";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";

export type MintOutcome = { ok: true; name: string } | { ok: false; why: string };

/** 期限：那台后端问一次会话快照（起一次 `tmux ls`）就答；15 s 盖住回程（远端没连着还要握手），与 `tmux-list` 同一档。 */
const MINT_BUDGET_MS = 15_000;

/** `tmux-name-mint` 的成品 `{name}` ⇒ 名字；形状不认 ⇒ `null`。严格收（恰好一个键、非空串）。 */
export function decodeMinted(v: unknown): string | null {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  const keys = Object.keys(o);
  return keys.length === 1 && keys[0] === "name" && typeof o.name === "string" && o.name !== "" ? o.name : null;
}

/** 问那台后端铸一个名字（两形共用的那一趟）。**不抛**。 */
async function askMint(origin: Origin, args: { cwd: string } | { forkOf: string }): Promise<MintOutcome> {
  let reply: Uint8Array;
  try {
    const body = jsonBody(args);
    const budget = budgetWithin(MINT_BUDGET_MS);
    reply = await chan.call(origin, "tmux-name-mint", body, budget);
  } catch (e) {
    return { ok: false, why: saidOf(e, copyText("tmuxMint.backend.tooOld")) };
  }
  let name: string | null = null;
  try {
    name = decodeMinted(readJson(reply));
  } catch {
    name = null;
  }
  return name !== null ? { ok: true, name } : { ok: false, why: copyText("tmuxMint.reply.badShape") };
}

/** 起新会话 / 全新 resume：那台按 cwd 派生 `<项目名>-cc`、按它的会话快照避让。 */
export function mintFreshTmuxName(origin: Origin, cwd: string): Promise<MintOutcome> {
  return askMint(origin, { cwd });
}

/** 分叉：源会话的 tmux 名（源已退出 ⇒ 它的 cwd）⇒ 那台铸 `<…>-fork-cc`（必与源名不同）、同样避让。 */
export function mintForkTmuxName(origin: Origin, source: string): Promise<MintOutcome> {
  return askMint(origin, { forkOf: source });
}

/** 远端铸不出名字 ⇒ 不起，说清是哪台、为什么（D4）。 */
export function refuseUnmintable(origin: Origin, why: string): void {
  showActionFailureToast(
    copyText("tmuxMint.refused.title"),
    copyText("tmuxMint.refused.body", { machine: origin, reason: why }),
    { level: "error", durationMs: 10000 },
  );
}
