/**
 * 命令面板里「切到会话」那几行：按标签页栏看到的顺序，每行一个会话（状态点 ＋ 机器徽标 ＋ 那台写好的整行标题 ＋ 在等什么）、
 * 右侧它的数字键（前 9 个）。只读标签页栏那一份事实（`session-face.ts`），这里不另判。
 * 给了 `attach` ⇒ 在终端里打开得了的会话行（`canAttach`，同会话头［在终端里打开］那一道）Shift+Enter 在终端里打开。
 */
import type { Command } from "./views/command-bar";
import type { Tab } from "./tab-model";
import { dotOf, needsOf, stateWord } from "./session-face";
import { canAttach } from "./face-can";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";

export function sessionCommands(
  tabs: readonly Tab[],
  switchTo: (sid: string) => void,
  keyOf: (n: number) => string | undefined,
  attach?: (sid: string) => void,
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
      alt: attach && canAttach(t.face, machine !== null) ? () => attach(t.sessionId) : undefined,
      run: () => switchTo(t.sessionId),
    };
  });
}
