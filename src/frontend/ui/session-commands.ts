/**
 * 命令面板里「切到会话」那几行：按标签页栏看到的顺序，每行一个会话（状态点 ＋ 机器徽标 ＋ 那台写好的整行标题 ＋ 在等什么）、
 * 右侧它的数字键（前 9 个）。只读标签页栏那一份事实（`session-face.ts`），这里不另判。
 * 给了 `terminal` ⇒ 「终端」点得了的会话行（`terminalWay`，同会话头「终端」那一颗）Shift+Enter 去它的终端。
 */
import type { Command } from "./views/command-bar";
import type { Tab } from "./tab-model";
import { dotOf, needsOf, stateWord } from "./session-face";
import { terminalWay } from "./face-can";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";

export function sessionCommands(
  tabs: readonly Tab[],
  switchTo: (sid: string) => void,
  keyOf: (n: number) => string | undefined,
  terminal?: (sid: string) => void,
): Command[] {
  return tabs.map((t, i) => {
    const dot = dotOf(t);
    const n = needsOf(t);
    const machine = isRemoteOrigin(t.origin) ? t.origin : null;
    return {
      id: `switch-${t.sessionId}`,
      title: t.title,
      keywords: copyText("main.cmd.switchSessionKeywords", { cwd: t.projectDir ?? "", machine: machine ?? "" }),
      hint: i < 9 ? keyOf(i + 1) : undefined,
      session: { dot, dotLabel: stateWord(t), machine, title: t.title, waiting: n ? n.text : null, origin: t.origin },
      alt: terminal && (terminalWay(t.face, machine !== null)?.go ?? "no") !== "no" ? () => terminal(t.sessionId) : undefined,
      run: () => switchTo(t.sessionId),
    };
  });
}
