/**
 * G3b-2（branch-anywhere）：分叉出新会话文件之后 —— **把它起起来**。
 *
 * G3a 回答「参数该是什么」（`fork-launch.ts`），G3b-1 让后端能带账号，
 * 本模块负责**编排**：推断 → 该问就问一次 → 起会话。
 *
 * # 为什么把 `ask` / `startLocal` / `startRemote` 注入进来
 *
 * 这三件都是副作用（弹窗、拉终端、走 ssh），而本模块真正要守的是**判断**：
 * 「什么时候该问」「不知道的时候绝不替用户填」「tmux 名一定要换」。
 * 注入之后这些判断可以被直接测，不必去驱动一个真弹窗 —— 同 `readiness.ts` 注入
 * `statusOf` 的思路。
 *
 * # 「两个都活着」在这里的落点
 *
 * 本模块**只起新的**，对原会话一个字都不碰（不 kill、不 attach、不 resume）。
 * 唯一会伤到它的是 **tmux 名撞车** —— 同名会让 `ccm` 把新会话 attach 进原窗口，
 * 那就变成「一个窗口两条对话轮流出现」，正好毁掉这个功能的意义。所以名字必须换。
 */

import {
  inferForkLaunch,
  slotsNeedingInput,
  forkTmuxName,
  type ForkLaunchFacts,
  type ForkLaunchInput,
} from "./fork-launch";

/** 用户在追问小窗里给的答案。只覆盖 `unknown` 的那几格。 */
export interface ForkChoices {
  /** `null` = 账号 0（不注入 `CLAUDE_CONFIG_DIR`）。 */
  configDir?: string | null;
  /** 起在 tmux 里还是直连。 */
  useTmux?: boolean;
  cwd?: string;
}

export interface ForkStartDeps {
  /** 弹一次追问小窗。返回 `null` = 用户取消（**什么都不起**）。 */
  ask: (facts: ForkLaunchFacts, slots: Array<keyof ForkLaunchFacts>) => Promise<ForkChoices | null>;
  startLocal: (a: {
    sessionId: string;
    cwd: string;
    configDir: string | null;
  }) => Promise<void>;
  startRemote: (a: {
    origin: string;
    sessionId: string;
    cwd: string;
    configDir: string | null;
    tmuxName: string | null;
  }) => Promise<void>;
}

export interface ForkStartInput {
  /** 刚分叉出来的新会话 sid。 */
  newSessionId: string;
  /** 远端 origin；`null` = 本机。 */
  origin: string | null;
  /** 源会话的事实（喂给 `inferForkLaunch`）。 */
  source: ForkLaunchInput;
  /** 源会话所在的 tmux 名（用来取一个**不同**的新名）。 */
  sourceTmuxName?: string | null;
  /** 已被占用的 tmux 名（避让）。 */
  takenTmuxNames?: readonly string[];
}

export type ForkStartOutcome = "started" | "cancelled";

/**
 * 起那条分叉出来的会话。
 *
 * **只在真有 `unknown` 时才问**（`slotsNeedingInput` 为空就直接起）——
 * 否则每分叉一次弹一次窗，这个功能就没人用了。
 */
export async function startForkedSession(
  input: ForkStartInput,
  deps: ForkStartDeps,
): Promise<ForkStartOutcome> {
  const facts = inferForkLaunch(input.source);
  // G6：**本机这条路不进 tmux** —— `resume_history_session` 交给用户自配的拉起器
  //（Windows 是 `wt.exe`，POSIX 是终端模拟器），tmux 与否根本不在它的表达能力里。
  // 所以本机分叉时把 tmux 这格从追问清单里摘掉：**问一个答案会被忽略的问题，
  // 比不问更坏** —— 用户会以为自己选了，而下面的 `startLocal` 压根不看。
  const slots = slotsNeedingInput(facts).filter(
    (s) => !(input.origin === null && s === "tmux"),
  );

  let choices: ForkChoices = {};
  if (slots.length > 0) {
    const answered = await deps.ask(facts, slots);
    if (answered === null) return "cancelled"; // 用户取消 ⇒ 什么都不起
    choices = answered;
  }

  // ★ 每一格都是「知道就用知道的，不知道就用用户答的」。
  //   **没有第三条路** —— 不知道且没答，就不该走到这里（`ask` 返回 null 已经拦掉）。
  const cwd =
    facts.cwd.kind === "known" ? facts.cwd.value : (choices.cwd ?? "");
  const configDir =
    facts.account.kind === "known" ? facts.account.value : (choices.configDir ?? null);
  const useTmux =
    facts.tmux.kind === "known" ? facts.tmux.value : (choices.useTmux ?? false);

  if (input.origin === null) {
    // 本机：G3b-1 给 `resume_history_session` 加的 `configDir` 走这里。
    // 本机路径不管 tmux（那是 PowerShell/POSIX 拉起器自己的事）。
    await deps.startLocal({
      sessionId: input.newSessionId,
      cwd,
      configDir,
    });
    return "started";
  }

  // 远端：tmux 名**必须与原会话不同**，否则 ccm 会 attach 进原窗口。
  const tmuxName = useTmux
    ? forkTmuxName(input.sourceTmuxName ?? cwd ?? "fork", input.takenTmuxNames ?? [])
    : null;
  await deps.startRemote({
    origin: input.origin,
    sessionId: input.newSessionId,
    cwd,
    configDir,
    tmuxName,
  });
  return "started";
}
