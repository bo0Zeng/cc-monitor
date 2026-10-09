/**
 * 在用户面前这台机器上开一个终端，跑 `command` —— 全仓开终端只有这一个家；命令都是后端出的成品，monitor 只开窗。
 * - 本机：`command` 已是本机后端渲好的那一串 ⇒ monitor 直接开窗（`open_terminal_window`，`ssh: false`；要起始目录的走
 *   {@link openLocalTerminal} → `open_local_terminal`）；
 * - 远端：① monitor 交那台的机器事实（`terminal_dial`）→ ② 本机后端 `terminal-ssh` 渲出这台终端方言的那一行
 *   （Windows 上 PowerShell `& ssh -t[ -J …] … -- '<bash -lic ''…''>'`，别处 POSIX 一行）→ ③ monitor 开窗（`ssh: true`）。
 *
 * 哪一步不成 ⇒ 抛一句人话（调用方出声）。这台找不到终端时壳回 `"noWindow"` ⇒ 抛 {@link NoTerminalWindow}；
 * 调用方按类型判（照实说 ＋ 设置入口，不把命令塞进剪贴板），不按哪句话里的字判、也不按 OS 猜。
 */
import { commands, type TerminalOpened } from "./ipc/commands";
import { copyText } from "./copy-table";
import { chan } from "../../comms/inward/chan";
import {
  budgetWithin,
  jsonBody,
  readJson,
  ReplyUnreadable,
  saidFrom,
} from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { toast } from "./kit/toast";
import { writeClipboard } from "./clipboard";
import { detailOf, failSaid } from "./kit/detail";
import { openSettingsWindow } from "./settings/open-settings";

/** 期限：`terminal-ssh` 是本机后端里的纯计算（不拨号），给足本机那条流的往返即可。 */
const RENDER_BUDGET_MS = 10_000;

/** `terminal-ssh` 的成品 `{command}` ⇒ 那一行；形状不认 ⇒ 抛。严格收（恰好一个键、非空串）。 */
export function decodeTerminalLine(v: unknown): string {
  if (v !== null && typeof v === "object" && !Array.isArray(v)) {
    const o = v as Record<string, unknown>;
    const keys = Object.keys(o);
    if (
      keys.length === 1 &&
      keys[0] === "command" &&
      typeof o.command === "string" &&
      o.command !== ""
    ) {
      return o.command;
    }
  }
  throw new ReplyUnreadable("terminalOpen reply shape");
}

/** 本机后端那一问（`terminal-ssh`）的成品 ⇒ 那一串。问不到 ⇒ 抛一句人话。 */
async function lineOf(ask: Promise<Uint8Array>): Promise<string> {
  try {
    return decodeTerminalLine(readJson(await ask));
  } catch (e) {
    throw new Error(saidFrom(e, LOCAL_ORIGIN));
  }
}

/** 为什么没开窗：一个终端都没探到（`none`）· 设置里指定的那个不在（`setMissing`）。 */
export type NoTerminalWhy = "none" | "setMissing";

/** 这台开不了终端窗口（壳回 `"noWindow"` / `"setMissing"`）。`message` 是给人看的那一句（说去设置里哪一格）；判只认类型。
 *  `command` 是本来要在窗口里跑的那一行（用户点［复制命令］时才进剪贴板）。 */
export class NoTerminalWindow extends Error {
  readonly command: string;
  readonly why: NoTerminalWhy;
  constructor(command: string, why: NoTerminalWhy = "none") {
    super(why === "setMissing" ? copyText("rsLaunch.posix.setTerminalMissing") : copyText("rsLaunch.posix.noTerminalWindow"));
    this.name = "NoTerminalWindow";
    this.command = command;
    this.why = why;
  }
}

/** 设置里指定终端的那一格（通用页 · 恢复组）。 */
const TERMINAL_SETTING = { page: "general", anchor: "terminal" } as const;

/** 开不了终端窗口时说的那一条（没探到 / 指定的不在）：照实说 ＋［设置］直达那一格 ＋［复制命令］（不自动写剪贴板，用户点了才复制那一行）。 */
export function sayNoTerminal(err: NoTerminalWindow): void {
  const [title, detail] =
    err.why === "setMissing"
      ? [copyText("terminalOpen.setMissing.title"), copyText("terminalOpen.setMissing.detail")]
      : [copyText("terminalOpen.noTerminal.title"), copyText("terminalOpen.noTerminal.detail")];
  toast(title, detail, {
    level: "error",
    action: [
      { label: copyText("terminalOpen.noTerminal.settings"), run: () => void openSettingsWindow(null, TERMINAL_SETTING) },
      {
        label: copyText("terminalOpen.noTerminal.copy"),
        run: () =>
          void writeClipboard(err.command).catch((e: unknown) => {
            toast(failSaid(copyText("terminalOpen.noTerminal.copyFailed"), e), err.command, { level: "error", detail: detailOf(e) });
          }),
      },
    ],
  });
}

/** 壳回的结局：`"noWindow"` / `"setMissing"` ⇒ 抛 {@link NoTerminalWindow}；真失败壳那边抛一句人话，原样往上走。 */
async function opened(command: string, got: Promise<TerminalOpened>): Promise<void> {
  const r = await got;
  if (r === "noWindow") throw new NoTerminalWindow(command, "none");
  if (r === "setMissing") throw new NoTerminalWindow(command, "setMissing");
}

async function openWindow(command: string, ssh: boolean): Promise<void> {
  await opened(command, commands.open_terminal_window({ command, ssh }));
}

/** 在本机开一个终端窗口、起始目录 `cwd`，跑本机后端渲好的 `cmd`（本机起会话 · 批量各开一个）。 */
export async function openLocalTerminal(cmd: string, cwd: string | null): Promise<void> {
  await opened(cmd, commands.open_local_terminal({ cmd, cwd }));
}

/** 在用户面前这台机器上开一个终端跑 `command`（`origin` = 命令要在哪台跑）。 */
export async function openTerminal(
  origin: Origin,
  command: string,
): Promise<void> {
  if (isLocalOrigin(origin)) {
    await openWindow(command, false);
    return;
  }
  const facts = await commands.terminal_dial({ origin });
  const body = jsonBody({ ...(facts as Record<string, unknown>), command });
  const budget = budgetWithin(RENDER_BUDGET_MS);
  const line = await lineOf(
    chan.call(LOCAL_ORIGIN, "terminal-ssh", body, budget),
  );
  await openWindow(line, true);
}
