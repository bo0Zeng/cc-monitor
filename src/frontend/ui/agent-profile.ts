/**
 * 前端侧 agent 画像 —— 值来自后端，这里不写死任何一格。
 *
 * 取值链：
 *
 * ```text
 * src/backend/agents/<名>/resume.rs（注册表 `Adapter.launch`）   ← 唯一的值源
 *   └─（cargo test --lib export_bindings ＝ npm run gen:types）→
 *      src/frontend/ui/generated/agent-profile-table.ts         ← 生成物，不许手改
 *        └─（本文件）→ lookupAgentProfile / listAgents / speakerNameOf / displayNameOf
 * ```
 *
 * **改后端那一份 ⇒ 前端拿到的值跟着变**；改了不重跑生成 ⇒ 门禁第六格 `generated` 红
 * （`git diff --exit-code -- src/frontend/ui/generated/`，跑在 `cargo test --lib` 之后）。
 * 没有 Rust 的那一侧（CI 的 frontend job）由 `agent-profile-parity.vitest.ts`
 * 把「生成物 == 金表」再对一遍（「后端那一份 == 金表」由后端 `agents_tests.rs` 钉）。
 *
 * 三条：
 * 1. 问不到就说问不到 —— [`lookupAgentProfile`] 对表里没有的 agent 回 `{ known: false, message }`，不悄悄回落到 claude 那一份；
 * 2. `null` ≠ 空：`null` 是「这一格没人考据过」，`[]` 才是「考据过、确实是空的」；
 * 3. 本文件不写工具名 / 进程名 / 启动器名 —— `agent-profile-parity.vitest.ts` 扫生产段，后端那张表里的任何一个值写进来就红。
 *    一个会话是哪一家也是后端的事实（会话事实的 `agent` · 历史清单那一行的 `agent`），不设「当前那一家」。
 */
import {
  AGENT_PROFILE_TABLE,
  DEFAULT_AGENT,
  type AgentProfileRow,
} from "./generated/agent-profile-table";
import { copyText } from "./copy-table";

export { DEFAULT_AGENT };
export type { AgentProfileRow };

/** 查画像的结果。**没有第三态**：要么查得到，要么说得出为什么查不到。 */
export type AgentProfileLookup =
  | { readonly known: true; readonly facts: AgentProfileRow }
  | { readonly known: false; readonly message: string };

/** 后端认得的 agent，按后端那张表的顺序。 */
export function listAgents(table: readonly AgentProfileRow[] = AGENT_PROFILE_TABLE): string[] {
  return table.map((row) => row.agent);
}

/**
 * 查一个 agent 的画像。
 *
 * 🔴 查不到**不回落**到任何别的 agent（尤其不是 claude）—— 回一句说得出口的话，
 * 由调用方决定怎么摆。这一条是 `KR93D3` 的本体。
 */
export function lookupAgentProfile(
  agent: string,
  table: readonly AgentProfileRow[] = AGENT_PROFILE_TABLE,
): AgentProfileLookup {
  const facts = table.find((row) => row.agent === agent);
  if (facts) return { known: true, facts };
  const known = listAgents(table);
  const roster = known.length > 0 ? known.join(" / ") : copyText("agentProfile.lookup.none");
  return {
    known: false,
    message: copyText("agentProfile.lookup.unknown", { agent, roster }),
  };
}

/**
 * 考据齐全的画像 —— 每一格都有值（按会话的那一家取）。只有起会话那几格事实：卡型随记录成品带出（`toolCards`），
 * 认 tmux 会话由后端认窗格时判。
 */
export type FullAgentProfile = {
  /**
   * resume/拉起前要 unset 的嵌套会话 env。
   *
   * ⚠ **顺序不是随手排的**：它逐项同序于后端 `agents/claudecode/resume.rs::NESTED_ENV`（唯一住址），
   * 而那个顺序**直接决定了送到远端的那条命令的字节**（`backend/control/payload.rs` 那个载荷内核）。
   * 今天两侧同源（这一格就是从那里来的），顺序天然不会漂。
   */
  nestedEnvVars: string[];
  /** 默认拉起二进制名。 */
  defaultLauncher: string;
  /** 用户的 shell 集成 wrapper（检测不到回退 `defaultLauncher`）；没有则 `null`。 */
  launcherAlias: string | null;
  /**
   * resume 的**调用形态**（flag ／ 子命令）。
   *
   * 类型从生成物借、不在这里写那两个字面量：本文件出现后端那张表的任何一个值都会被 `agent-profile-parity.vitest.ts` 判红。
   */
  resumeKind: AgentProfileRow["resumeKind"];
  /** resume 那个字面量（claude 是 `--resume`）。⚠ 它**不带形态**，形态看 `resumeKind`。 */
  resumeFlag: string;
};

/**
 * 取一份**考据齐全**的画像。
 *
 * 失败**不静默**：agent 不在表里 ⇒ 抛 [`lookupAgentProfile`] 那句话。
 * 「某一格是 `null` ⇒ 抛」那一半随五格词表进了后端（今天没有可空的列表格）。
 * 🔴 **不许在这里塞一个「回落到 claude」的兜底** —— 那正是 `KR93D3` 禁的那件事。
 */
export function fullAgentProfile(
  agent: string,
  table: readonly AgentProfileRow[] = AGENT_PROFILE_TABLE,
): FullAgentProfile {
  const got = lookupAgentProfile(agent, table);
  if (!got.known) throw new Error(got.message);
  const facts = got.facts;
  return {
    nestedEnvVars: [...facts.nestedEnvVars],
    defaultLauncher: facts.defaultLauncher,
    launcherAlias: facts.launcherAlias,
    resumeKind: facts.resumeKind,
    resumeFlag: facts.resumeToken,
  };
}

/** 起这一家时的默认启动器。认不出 ⇒ 抛（同 [`fullAgentProfile`]，不回落到别的哪一家）。 */
export function defaultLauncherOf(agent: string): string {
  return fullAgentProfile(agent).defaultLauncher;
}

/** 这一家有没有账号这一维（选号 · 跟随上次的号只对有的那一家）。认不出 ⇒ 没有。 */
export function agentHasAccounts(agent: string): boolean {
  const got = lookupAgentProfile(agent);
  return got.known && got.facts.hasAccounts;
}

/** 这一家在消息流里叫什么（卡头 · 刻度悬停的短名）。认不出 / 还不知道是哪一家 ⇒ `null`（不顶替成哪一家）。 */
export function speakerNameOf(agent: string | null): string | null {
  if (agent === null) return null;
  const got = lookupAgentProfile(agent);
  return got.known ? got.facts.speakerName : null;
}

/** 这一家的产品全称（「{名} 会话还不能选账号」这类话里用）。认不出 ⇒ `null`。 */
export function displayNameOf(agent: string | null): string | null {
  if (agent === null) return null;
  const got = lookupAgentProfile(agent);
  return got.known ? got.facts.displayName : null;
}
