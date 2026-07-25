// account-ux U8：Ctrl+K 命令面板里的**账号命令**构造（纯函数，可测）。
//
// 为什么单独一个文件：原来这段长在 main.ts 的 `DOMContentLoaded` 闭包里（局部 const、无导出），
// 于是"命令的出现/消失条件"完全测不到——把可用性判定改成恒 true，550 个用例照样全绿。
// D 审计点名这是"计划要求的变异验证根本红不了"。挪成纯函数后，判定就能被锁住。
//
// 这里只**造**命令（title / keywords / hint / run 闭包），不碰 DOM、不查数据：调用方把快照
// （账号列表、可对齐 sid、当前会话）喂进来。
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
  /** U6 的可对齐会话 sid 列表（`tabs.accountMismatchSids()`）。 */
  alignableSids: string[];
  /** 当前活跃会话 sid（`tabs.activeSessionId()`）。 */
  activeSid: string | null;
  /** 取某 action 当前生效 chord 的友好名（用于命令右侧的快捷键提示）。 */
  chordHint: (id: string) => string | undefined;
  // —— 动作（由调用方接到 AccountChip / TabManager）——
  setCurrent: (name: string) => void;
  alignSession: (sid: string) => void;
  alignAll: () => void;
  openSettings: () => void;
}

/**
 * 造账号相关命令。规则：
 * - 术语统一「当前工作账号」（U4 起的全 UI 口径；keywords 仍保留"默认"做搜索别名）。
 * - 对齐类命令**只在真的可用时才出现**，不做灰着的死命令：
 *   单会话对齐要求 `activeSid` 确实在 `alignableSids` 里；批量要求列表非空。
 * - 破坏性确认不在这里做——单会话走 `restartWithAccount` 自带确认，批量走 TabManager 的两步确认。
 */
export function buildAccountCommands(input: AccountCommandsInput): AccountCommand[] {
  const { snapshot, alignableSids, activeSid, chordHint } = input;
  const cmds: AccountCommand[] = [];

  if (snapshot) {
    for (const a of snapshot.accounts) {
      if (!isSelectable(a)) continue; // 单一来源，随 isSelectable 演进
      const isCur = snapshot.defaultName === a.name;
      cmds.push({
        id: `acct-default-${a.name}`,
        title: `账号：设 ${a.name} 为当前工作账号${isCur ? "（已是当前）" : ""}`,
        keywords: `account 账号 切换 default 默认 当前工作账号 ${a.name} ${a.email}`,
        // 教学式发现：把「打开账号菜单」的键位露在这里（用户若绑过）。
        hint: chordHint("account.switch-default"),
        run: () => {
          if (!isCur) input.setCurrent(a.name);
        },
      });
    }
  }

  if (activeSid && alignableSids.includes(activeSid)) {
    cmds.push({
      id: "acct-align-active",
      title: "账号：把当前会话对齐到当前工作账号…（会重启该会话）",
      keywords: "account 账号 对齐 align 重启 当前会话 current",
      hint: chordHint("account.align-active"),
      run: () => input.alignSession(activeSid),
    });
  }

  if (alignableSids.length > 0) {
    cmds.push({
      id: "acct-align-all",
      title: `账号：对齐全部不一致的会话（${alignableSids.length}）…`,
      keywords: "account 账号 对齐 align 全部 批量 all 不一致",
      run: () => input.alignAll(),
    });
  }

  cmds.push({
    id: "acct-manage",
    title: "账号：管理…",
    keywords: "account 账号 管理 manage 设置",
    run: () => input.openSettings(),
  });

  return cmds;
}
