/**
 * 账号页「登录」那两条路：开一个终端窗口跑那台答的登录那一行（能不能开、用哪个由壳那一处答，`open_window`）；
 * 开不了 ⇒ 调用方说「本机无法开终端窗口」并给［在 tmux 里登录］：用这个号在那台起一个 tmux 会话（现有的起会话口，
 * 会话作为标签出现在主窗口里，进去 /login）。
 */
import { accountsLoginCmd } from "../account-ops";
import { openTerminal } from "../terminal-open";
import { POSIX_NO_WINDOW_MARKER } from "../remote-launch-run";
import { chosenAccount } from "../launch-account";
import type { Origin } from "../ipc/origin";
import { startNewSession } from "../new-session";

/** 开登录窗口的结局：开了 · 这台电脑开不了终端窗口（换在 tmux 里登录）。其余失败抛（已说成一句）。 */
export type LoginWindow = "opened" | "noWindow";

/** 问那台要登录那一行（`cmd` 已有就不问），开终端窗口跑它。 */
export async function openLoginWindow(origin: Origin, name: string, cmd?: string): Promise<LoginWindow> {
  const line = cmd ?? (await accountsLoginCmd(origin, name));
  try {
    await openTerminal(origin, line);
    return "opened";
  } catch (e) {
    if (String(e instanceof Error ? e.message : e).includes(POSIX_NO_WINDOW_MARKER)) return "noWindow";
    throw e;
  }
}

/** 在 tmux 里登录：用这个号在那台起一个 tmux 会话（目录 = 那个号的目录；账号 0 没有目录 ⇒ 不给这条路）。与起新会话同一个请求。 */
export async function loginInTmux(origin: Origin, name: string, dir: string): Promise<void> {
  await startNewSession({ origin, cwd: dir, account: chosenAccount(name), place: "tmux" });
}
