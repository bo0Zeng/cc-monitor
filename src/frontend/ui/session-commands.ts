/**
 * 命令面板里「切到会话」那几行：按标签页栏看到的顺序，每行一个会话（状态点 ＋ 机器徽标 ＋ 项目 ＋ 标题 ＋ 在等什么）、
 * 右侧它的数字键（前 9 个）。只读标签页栏那一份事实（`session-face.ts`），这里不另判。
 */
import type { Command } from "./views/command-bar";
import type { Tab } from "./tab-model";
import { dotOf, needsOf, titleParts } from "./session-face";
import { dotLabel, needsWord } from "./session-words";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";

export function sessionCommands(tabs: readonly Tab[], switchTo: (sid: string) => void, keyOf: (n: number) => string | undefined): Command[] {
  return tabs.map((t, i) => {
    const p = titleParts(t);
    const dot = dotOf(t);
    const n = needsOf(t);
    const machine = isRemoteOrigin(t.origin) ? t.origin : null;
    return {
      id: `switch-${t.sessionId}`,
      title: p.proj ? `${p.proj} ${p.title}` : p.title,
      keywords: copyText("main.cmd.switchSessionKeywords", { cwd: t.projectDir ?? "", machine: machine ?? "" }),
      hint: i < 9 ? keyOf(i + 1) : undefined,
      session: { dot, dotLabel: dotLabel(dot), machine, project: p.proj, title: p.title, waiting: n ? needsWord(n.kind) : null },
      run: () => switchTo(t.sessionId),
    };
  });
}
