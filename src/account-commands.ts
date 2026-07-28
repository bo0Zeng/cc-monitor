// account-ux U8：Ctrl+K 命令面板里的**账号命令**构造（纯函数，可测）。
//
// 为什么单独一个文件：原来这段长在 main.ts 的 `DOMContentLoaded` 闭包里（局部 const、无导出），
// 于是"命令的出现/消失条件"完全测不到——把可用性判定改成恒 true，550 个用例照样全绿。
// D 审计点名这是"计划要求的变异验证根本红不了"。挪成纯函数后，判定就能被锁住。
//
// 这里只**造**命令（title / keywords / hint / run 闭包），不碰 DOM、不查数据：调用方把快照
// （账号列表）喂进来。
//
// F09：对齐类命令（acct-align-active/acct-align-all）随对齐全套一并删除——批量/一键对齐是
// 组合层便利，不做等价替代，用户改走 tab 右键的 Restart flyout 逐会话操作。
import { isSelectable, type Account } from "./accounts";

/** 与 views/command-bar.ts 的 Command 结构对齐（这里不 import 以免把 DOM 依赖拖进纯函数模块）。 */
export interface AccountCommand {
  id: string;
  title: string;
  keywords: string;
  hint?: string;
  run: () => void;
}

export interface AccountCommandsInput {
  /** chip 的 ready 快照：非 ready（未连远端 / 未启用 / 老 daemon）传 null。 */
  snapshot: { accounts: Account[]; defaultName: string | null } | null;
  /** 取某 action 当前生效 chord 的友好名（用于命令右侧的快捷键提示）。 */
  chordHint: (id: string) => string | undefined;
  // —— 动作（由调用方接到 AccountChip）——
  setCurrent: (name: string) => void;
  openSettings: () => void;
}

/**
 * 造账号相关命令。规则：
 * - 术语统一「当前账号」（U4 起的全 UI 口径；keywords 仍保留"默认"做搜索别名）。
 */
export function buildAccountCommands(input: AccountCommandsInput): AccountCommand[] {
  const { snapshot, chordHint } = input;
  const cmds: AccountCommand[] = [];

  if (snapshot) {
    for (const a of snapshot.accounts) {
      if (!isSelectable(a)) continue; // 单一来源，随 isSelectable 演进
      const isCur = snapshot.defaultName === a.name;
      cmds.push({
        id: `acct-default-${a.name}`,
        title: `账号：设 ${a.name} 为当前账号${isCur ? "（已是当前）" : ""}`,
        keywords: `account 账号 切换 default 默认 当前账号 ${a.name} ${a.email}`,
        // 教学式发现：把「打开账号菜单」的键位露在这里（用户若绑过）。
        hint: chordHint("account.switch-default"),
        run: () => {
          if (!isCur) input.setCurrent(a.name);
        },
      });
    }
  }

  cmds.push({
    id: "acct-manage",
    title: "账号：管理…",
    keywords: "account 账号 管理 manage 设置",
    run: () => input.openSettings(),
  });

  return cmds;
}
