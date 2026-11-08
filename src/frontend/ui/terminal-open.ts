/**
 * 在用户面前这台机器上开一个终端，跑 `command` —— 全仓开终端只有这一个家；命令都是后端出的成品，monitor 只开窗。
 * 本机远端同一条：① 本机后端 `terminal-ssh` 只收那台的名字（本机就是 `<local>`），渲出这台终端方言的那一行
 * （远端：Windows 上 PowerShell `& '<ssh 全路径>' -t[ -J …] … -- '<bash -lic ''…''>'`，别处 POSIX 一行，本机没装 ssh 客户端 / 判不了 ⇒
 * 后端不出那一行、说一句；本机：那一串本身）→ ② monitor 开窗（`open_terminal_window`）。
 *
 * 哪一步不成 ⇒ 抛一句人话（调用方出声）。这台开不了终端窗口时壳回 `noWindow`（带写好的那几句）⇒ 抛 {@link NoTerminalWindow}；
 * 调用方按类型判（照实说 ＋ 设置入口，不把命令塞进剪贴板），不按哪句话里的字判、也不按 OS 猜。
 */
import { commands } from "./ipc/commands";
import type { TerminalOpened } from "./generated/TerminalOpened";
import { copyText } from "./copy-table";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { TerminalSshReq } from "./generated/TerminalSshReq";
import type { SshCommand } from "./generated/SshCommand";
import { isLocalOrigin, LOCAL, type Machine } from "./ipc/origin";
import { toast, type ToastAction } from "./kit/toast";
import { writeClipboard } from "./clipboard";
import { detailOf, failSaid } from "./kit/detail";
import { openSettingsWindow } from "./open-settings";

/** 期限：`terminal-ssh` 是本机后端里的纯计算（不拨号），给足本机那条流的往返即可。 */
const RENDER_BUDGET_MS = 10_000;

/** 本机后端那一问（`terminal-ssh`）⇒ 前奏 · 那一行（开窗跑两段接起来的；开不了窗复制的只有那一行）。问不到 ⇒ 抛一句人话。 */
async function lineOf(req: TerminalSshReq): Promise<SshCommand> {
  return await callTyped(LOCAL, "terminal-ssh", req, budgetWithin(RENDER_BUDGET_MS));
}

/** 壳回「开不了窗」时带的那几句（壳写好：一句 · 标题 · 说明）。 */
export type NoWindowWords = Omit<Extract<TerminalOpened, { outcome: "noWindow" }>, "outcome">;

/** 这台开不了终端窗口（壳回 `noWindow`：没探到 · 设置里指定的不在）。`message` 是壳写好的那一句；判只认类型。
 *  `command` 是本来要在窗口里跑的那一行（用户点［复制命令］时才进剪贴板）；`title` · `detail` 是提示那一条照摆的两句。 */
export class NoTerminalWindow extends Error {
  readonly command: string;
  readonly title: string;
  readonly detail: string;
  constructor(command: string, words: NoWindowWords) {
    super(words.said);
    this.name = "NoTerminalWindow";
    this.command = command;
    this.title = words.title;
    this.detail = words.detail;
  }
}

/** 设置里指定终端的那一格（通用页 · 恢复组）。 */
const TERMINAL_SETTING = { page: "general", anchor: "terminal" } as const;

/** 开不了终端窗口时说的那一条（壳写好的标题 · 说明照摆）＋［设置］直达那一格 ＋［复制命令］（不自动写剪贴板，用户点了才复制那一行）。 */
export function sayNoTerminal(err: NoTerminalWindow): void {
  toast(err.title, err.detail, {
    level: "error",
    action: [{ label: copyText("mobDrawer.settings.aria"), run: () => void openSettingsWindow(null, TERMINAL_SETTING) }, copyCommand(err.command)],
  });
}

/** ［复制命令］：点了才把那一行写进剪贴板；写不进 ⇒ 说一句、那一行照列。终端没开成的各处都用它。 */
export function copyCommand(command: string): ToastAction {
  return {
    label: copyText("terminalOpen.noTerminal.copy"),
    run: () =>
      void writeClipboard(command).catch((e: unknown) => {
        toast(failSaid(copyText("terminalOpen.noTerminal.copyFailed"), e), command, { level: "error", detail: detailOf(e) });
      }),
  };
}

/** 壳回的结局：`noWindow` ⇒ 抛 {@link NoTerminalWindow}（带壳写好的那几句）；真失败壳那边抛一句人话，原样往上走。 */
async function opened(command: string, got: Promise<TerminalOpened>): Promise<void> {
  const r = await got;
  if (r.outcome === "noWindow") throw new NoTerminalWindow(command, r);
}

/** 开窗跑本机后端拼好的 `window`，起始目录 `cwd`；开不了窗 ⇒ 交给［复制命令］的只有 `line`。 */
async function openWindow({ window, line }: SshCommand, cwd: string | null): Promise<void> {
  await opened(line, commands.open_terminal_window({ command: window, cwd }));
}

/**
 * 在用户面前这台机器上开一个终端跑 `command`（`origin` = 命令要在哪台跑；本机远端同一条）。
 * `sid`：这个终端显示的那条会话（`session-open` 的成品带它）⇒ 远端那一串由本机后端另带前奏与收尾：开的那个 shell 记下自己的窗口标签、
 * 那条 ssh 一退就删，↗ 先照它在本机找；开不了窗时复制出去的只有那一行。
 * `cwd`：那条会话在那台的目录。它只当这台窗口的起始目录（本机起会话的直路那一行不带 `--cwd`），所以只在那台就是这台时交给壳；
 * 远端的目录在那台的那一行里。
 */
export async function openTerminal(
  origin: Machine,
  command: string,
  sid: string | null = null,
  cwd: string | null = null,
): Promise<void> {
  // 只交名字：地址 · 跳板 · 先试哪条都照本机后端的机器表；本机那一行也由它答。
  const got = await lineOf({ machine: origin, command, ...(sid !== null ? { sid } : {}) });
  await openWindow(got, isLocalOrigin(origin) ? cwd : null);
}

/** 在用户面前这台机器上开一个终端，进 `origin` 那台的登录 shell（本机 ⇒ 本机 shell；那一行是本机后端 `terminal-ssh` 渲的）。 */
export async function openLoginShell(origin: Machine): Promise<void> {
  await openWindow(await lineOf({ machine: origin }), null);
}
