// 〔`A3` 第二波〕换号重启的**本机那一跳**：kill 之后，用选中的账号把同一个 sid 在本机 resume 起来。
//
// # 为什么单开一个文件
//
// `account-restart.ts` 的编排（预检 → 二次确认 → [compact] → 优雅退出 → kill → resume → 记账）
// 两侧**逐步相同**；差别只在最后「resume」那一跳怎么起：
//
// | | 远端 | 本机 |
// |---|---|---|
// | 起法 | `runRemoteResumeTmux`（渲一条 ssh 命令、开终端，失败回退剪贴板） | `resume_history_session`（本机后端直接起，**没有剪贴板那条回退**） |
// | 账号怎么交 | `LaunchModifiers.configDir / accountName` | 载荷上的 `account`（具名：`configDir` ＋ `name`，键名来自生成物） |
// | 模型偏好 | 交（`modelOverride`） | **交不了** —— `history.rs` 本机那条的载荷里没有模型这一格（如实登记，不假装） |
//
// ⇒ 编排不分叉，只在第⑤步按 `origin` 选这一跳；本文件就是那一跳本身。
//
// # 账号从哪来 —— **用户在菜单里点的那一个**，不走 `localLaunchAccountSync`
//
// `localLaunchAccountSync` 取的是「这条会话上次用的号 / 当前号」（主路 resume 的语义）；
// 换号重启的正题恰恰是**换成另一个**，取它就把用户的选择丢了。⇒ 与 `fork-flow.ts` 同类：
// 用户显式选的那一格，自己组载荷。**名字与目录一起交**（`K-R53`：后端那条 ccm 路只认名字）。
import { commands } from "./ipc/commands";
import { LOCAL_LAUNCH_ACCOUNT_WIRE } from "./generated/launch-render-facts";
import type { LocalLaunchAccountWire } from "./accounts";
import { showActionFailureToast } from "./error-toast";

/** 用户点的那个具名账号 → 本机载荷上的 `account`。键名与判别值都来自生成物（`K-R95`），不写字面量。 */
export function namedLocalAccountWire(configDir: string, name: string): LocalLaunchAccountWire {
  return {
    [LOCAL_LAUNCH_ACCOUNT_WIRE.tag]: LOCAL_LAUNCH_ACCOUNT_WIRE.named,
    [LOCAL_LAUNCH_ACCOUNT_WIRE.configDir]: configDir,
    [LOCAL_LAUNCH_ACCOUNT_WIRE.name]: name,
  };
}

export interface LocalRestartResume {
  sessionId: string;
  cwd: string;
  /** 设置里「本机 resume 命令」（空串 = 没配 ⇒ 交 `null`，由后端用默认）。 */
  launcher: string;
  /** 旧会话所在的 tmux 名 —— 此时已被 kill，原名让出来了，照远端那条一样复用它。 */
  tmuxName: string;
  configDir: string;
  accountName: string;
}

/**
 * 在本机用选中的账号 resume 同一个 sid。
 *
 * @returns 真起来了才 `true`。失败时本函数**自己**弹一条带原因的提示，然后回 `false`；
 *          调用方（`restartWithAccount`）据此不记账、并说清「旧会话已结束、新会话没起来」。
 */
export async function runLocalRestartResume(req: LocalRestartResume): Promise<boolean> {
  try {
    await commands.resume_history_session({
      sessionId: req.sessionId,
      cwd: req.cwd,
      launcher: req.launcher.trim() === "" ? null : req.launcher,
      tmuxName: req.tmuxName,
      account: namedLocalAccountWire(req.configDir, req.accountName),
    });
    return true;
  } catch (e) {
    showActionFailureToast("本机没能用新账号拉起会话", String(e), {
      level: "error",
      durationMs: 10000,
    });
    return false;
  }
}
