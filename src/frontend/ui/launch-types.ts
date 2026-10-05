/**
 * 「起一个会话」的意图（纯类型叶子模块，零 import）。
 *
 * 一个动作 ＋ 一个容器 ＋ 若干正交修饰（账号 · 模型 · 身份标记 · 启动期令牌）。它被摊成线上请求交给那台后端，
 * 渲出来的永远是一行 `ccm …`（`launch-cli-wire.ts::buildCliRenderRequest` → 帧命令 `launch-render-cli`）；
 * 环境、中转地址、身份标记由那台机器上的 `ccm` 自己做，界面一句 shell 都不拼。
 */

/** 起会话就是起会话，attach 就是 attach，不要 `or`。`send-into` = 往已有的空 tmux 会话里就地 resume。 */
export type TmuxMode = "create" | "send-into" | "attach-only";

export type LaunchContainer = { kind: "none" } | { kind: "tmux"; name: string; mode: TmuxMode };

export type LaunchAction =
  | { kind: "new" }
  | { kind: "resume"; sid: string }
  | { kind: "attach"; name: string };

/** 起会话那一格「用哪个号」（线上 `AccountAsk`：跟随 · 账号 0 · 用户点名）—— 判定在那台后端。 */
export type { AccountAsk as LaunchAccount } from "./generated/AccountAsk";
import type { AccountAsk } from "./generated/AccountAsk";

/**
 * 正交修饰（调用方手上的原值）：要哪个号 · 那台的模型偏好表（用户设置的原值，`{号: 模型}`）。
 * 号缺席 ＝ 跟随（同那台后端的缺省）。
 */
export interface LaunchModifiers {
  account?: AccountAsk;
  models?: Record<string, string>;
  /** 开终端之前问一句（收那台判出来的号的目录；账号 0 / 不指定 ⇒ `undefined`）；回 `false` ⇒ 不起（它自己已经说过了）。 */
  preflight?: (configDir: string | undefined) => Promise<boolean>;
}

/** 调用方已解析好的具体意图。 */
export interface LaunchContext {
  /** 这个会话是哪一家（线上的 kind）：怎么 resume、能不能选号都按它，由那台后端渲。 */
  agent: string;
  action: LaunchAction;
  container: LaunchContainer;
  cwd: string | null;
  account: AccountAsk;
  /** 那台的模型偏好表（原值）。 */
  models: Record<string, string>;
  launcherOverride: string | undefined;
  /** 身份标记：建出来的 tmux 会话打上这个 sid（只有 tmux 建会话 resume 那一形设）。 */
  ccmSid: string | undefined;
}
