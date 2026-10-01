/**
 * 要求：「待迁」最后一行 ——「远端拉起那串的 ssh 外壳（`ssh -t -J … host '<串>'` · PowerShell 窗口载荷）
 * 由本机后端渲（组请求用 `dial/machine.rs::resolve`），monitor 只开终端」（FIX4 题面第 1 条）。
 *
 * **在用户面前这台机器上开一个终端，跑 `command`** —— 全仓开终端只有这一个家；本机远端同形：本机后端出成品，monitor 只开窗。
 * - 本机：① 本机后端 `terminal-local` 交回那一串 → ② monitor 开窗（`open_terminal_window`，`ssh: false`）；
 * - 远端：① monitor 交那台的机器事实（`terminal_dial`：它的机器表 ＋ 上次赢的那条）→ ② 本机后端 `terminal-ssh` 渲出那一行
 *   PowerShell（`& ssh -t[ -J …] … -- '<bash -lic ''…''>'`）→ ③ monitor 开窗（`ssh: true`）。
 * 〔本地半〕这次拉起的启动期令牌交给本机后端，由它在成品前面接令牌握手前奏（PowerShell 只在后端写）。
 *
 * 哪一步不成 ⇒ 抛一句人话（调用方照旧走剪贴板回退 / 出声）。POSIX 上开窗那一步回 `POSIX_NO_TERMINAL_WINDOW`
 * （既定设计：刻意不替你挑终端模拟器；调用方按 `POSIX_NO_WINDOW_MARKER` 判，不按 OS 猜）。
 */
import { commands } from "./ipc/commands";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 期限：`terminal-ssh` / `terminal-local` 是本机后端里的纯计算（不拨号），给足本机那条流的往返即可。 */
const RENDER_BUDGET_MS = 10_000;

/** `terminal-ssh` / `terminal-local` 的成品 `{command}` ⇒ 那一行；形状不认 ⇒ 抛。严格收（恰好一个键、非空串）。 */
export function decodeTerminalLine(v: unknown): string {
  if (v !== null && typeof v === "object" && !Array.isArray(v)) {
    const o = v as Record<string, unknown>;
    const keys = Object.keys(o);
    if (keys.length === 1 && keys[0] === "command" && typeof o.command === "string" && o.command !== "") {
      return o.command;
    }
  }
  throw new Error(copyText("terminalOpen.reply.badShape"));
}

/** 本机后端那一问（`terminal-ssh` / `terminal-local`）的成品 ⇒ 那一串。问不到 ⇒ 抛一句人话。 */
async function lineOf(ask: Promise<Uint8Array>): Promise<string> {
  let reply: Uint8Array;
  try {
    reply = await ask;
  } catch (e) {
    throw new Error(saidOf(e, copyText("terminalOpen.backend.tooOld")));
  }
  return decodeTerminalLine(readJson(reply));
}

/** 在用户面前这台机器上开一个终端跑 `command`（`origin` = 命令要在哪台跑）。
 *  `rbindToken` = 这次拉起铸的启动期令牌（`attach` · 部署那几条不起 agent 进程的给 `null`），交给本机后端接前奏。 */
export async function openTerminal(origin: Origin, command: string, rbindToken: string | null = null): Promise<void> {
  if (isLocalOrigin(origin)) {
    const body = jsonBody({ command, rbindToken });
    const budget = budgetWithin(RENDER_BUDGET_MS);
    const line = await lineOf(chan.call(LOCAL_ORIGIN, "terminal-local", body, budget));
    await commands.open_terminal_window({ command: line, ssh: false });
    return;
  }
  const facts = await commands.terminal_dial({ origin });
  const body = jsonBody({ ...(facts as Record<string, unknown>), command, rbindToken });
  const budget = budgetWithin(RENDER_BUDGET_MS);
  const line = await lineOf(chan.call(LOCAL_ORIGIN, "terminal-ssh", body, budget));
  await commands.open_terminal_window({ command: line, ssh: true });
}
