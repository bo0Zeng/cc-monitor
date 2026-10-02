/**
 * 「起一个会话」的意图（纯类型叶子模块，零 import）。
 *
 * 一个动作 ＋ 一个容器 ＋ 若干正交修饰（账号 · 模型 · 身份标记 · 启动期令牌）。它被摊成线上请求交给那台后端，
 * 渲出来的永远是一行 `ccm …`（`remote-launch-run.ts::buildCliRenderRequest` → 帧命令 `launch-render-cli`）；
 * 环境、中转地址、身份标记由那台机器上的 `ccm` 自己做，界面一句 shell 都不拼。
 */

/** 起会话就是起会话，attach 就是 attach，不要 `or`。`send-into` = 往已有的空 tmux 会话里就地 resume。 */
export type TmuxMode = "create" | "send-into" | "attach-only";

export type LaunchContainer = { kind: "none" } | { kind: "tmux"; name: string; mode: TmuxMode };

export type LaunchAction =
  | { kind: "new" }
  | { kind: "resume"; sid: string }
  | { kind: "attach"; name: string };

/** 账号只有两态：`account`（带配置目录，说得出名字时带名字）· `base`（账号 0）。远端没有「继承」那一态。 */
export type LaunchAccount =
  | { kind: "account"; name?: string; configDir: string }
  | { kind: "base" };

/**
 * 正交修饰的传递载体（解析前的原始形态：调用方手上的一个目录 / 一个名字 / 一个模型串 / 一个令牌）。
 * 命名字段而不是位置参数：`configDir` 与 `accountName` 同类型相邻，传错顺序 tsc 抓不到。
 */
export interface LaunchModifiers {
  /** 账号目录。 */
  configDir?: string;
  /** 与 `configDir` 成对：说得出名字 ⇒ `--account <名>`；只有目录 ⇒ `--account-dir <目录>`。 */
  accountName?: string;
  /** 该账号配置的默认模型偏好（本机 `config.json`）。 */
  modelOverride?: string;
}

/** 调用方已解析好的具体意图。 */
export interface LaunchContext {
  action: LaunchAction;
  container: LaunchContainer;
  cwd: string | null;
  account: LaunchAccount;
  launcherOverride: string | undefined;
  /** 身份标记：建出来的 tmux 会话打上这个 sid（只有 tmux 建会话 resume 那一形设）。 */
  ccmSid: string | undefined;
  modelOverride?: string;
}
