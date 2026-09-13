/**
 * **前端侧 agent 画像 —— 值来自后端。**
 *
 * # `K-R93`（09-12）：这里从此不写死任何一格
 *
 * 立件时本文件是一份**手写常量**（`AGENT_PROFILE`），而后端 `src-tauri/src/adapter.rs`
 * 另有一份（claude ＋ codex）—— 同一件事两份实现，`K-R54` 表**第 11 行**。
 * 🔴 而且前端这一份**只认 claude** ⇒ 接上后端的同一刻，codex 那一格也补上了
 * （**那不是回归，是把一格漏的补上**）。
 *
 * 取值链，一句话：
 *
 * ```text
 * src-tauri/src/adapter.rs::agent_profile_facts     ← 唯一的值源
 *   └─（cargo test --lib export_bindings ＝ npm run gen:types）→
 *      src/generated/agent-profile-table.ts         ← 生成物，不许手改
 *        └─（本文件）→ AGENT_PROFILE / lookupAgentProfile / listAgents
 * ```
 *
 * **改后端那一份 ⇒ 前端拿到的值跟着变**；改了不重跑生成 ⇒ 门禁第六格 `generated` 红
 * （`git diff --exit-code -- src/generated/`，跑在 `cargo test --lib` 之后）。
 * 没有 Rust 的那一侧（CI 的 frontend job）由 `agent-profile-parity.vitest.ts`
 * 把「生成物 == `adapter.rs` 源」再对一遍。
 *
 * # 三条不许犯的
 *
 * 1. **问不到就说问不到**（`KR93D3`，`K-R92` 那一形的预防）—— [`lookupAgentProfile`]
 *    对表里没有的 agent 回 `{ known: false, message }`，**不许悄悄回落到 claude 那一份**：
 *    那就是「一个值装了两件事」。
 * 2. **`null` ≠ 空** —— 生成物里的 `null` 是「这一格今天没人考据过」（codex 的五格就是），
 *    `[]` 才是「考据过、确实是空的」。[`fullAgentProfile`] 碰到 `null` **当场抛**，
 *    不悄悄给一个空 `Set`。
 * 3. **不许在本文件里再写一份工具名 / 进程名 / 启动器名** —— `agent-profile-parity.vitest.ts`
 *    有一条判据扫本文件的生产段，把后端那张表里的**任何一个值**写死进来就红
 *    （`KR93D1` 第三刀）。这也是 `AGENT_PROFILE` 用 `ACTIVE_AGENT` 而不是字面量 `"claude"`
 *    取那一行的原因：**谁是当前 agent 也是后端的事实**。
 *
 * ★ **第二刀仍然没做**（原头注那句话原样留着）：本件**不拆记录模型、不动 `renderMessage`
 * 的 `switch(rec.type)` 分发** —— 件文件 `§0b` 逐字挡住了，那是等第二个 wire 样本看清
 * 真·共性之后的事（SS-1）。
 */
import {
  ACTIVE_AGENT,
  AGENT_PROFILE_TABLE,
  type AgentProfileRow,
} from "./generated/agent-profile-table";

export { ACTIVE_AGENT };
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
  const roster = known.length > 0 ? known.join(" / ") : "（一个都没有）";
  return {
    known: false,
    message: `问不到 agent「${agent}」的画像：后端那张表里没有它（表里有：${roster}）。`,
  };
}

/** 考据齐全的画像 —— 每一格都有值。`AGENT_PROFILE` 就是当前 agent 那一份。 */
export type FullAgentProfile = {
  /** 子 agent 工具（展开 = 子会话）。 */
  agentTools: Set<string>;
  /** 交互工具（agent 在等用户决定）。 */
  interactiveTools: Set<string>;
  /** 写类工具（行级 diff）。 */
  diffTools: Set<string>;
  /** 结果默认按 markdown 渲染的工具。 */
  mdTools: Set<string>;
  /** tmux 前台命令算该 agent 的会话。 */
  livenessProcessNames: Set<string>;
  /**
   * resume/拉起前要 unset 的嵌套会话 env。
   *
   * ⚠ **顺序不是随手排的**：它逐项同序于后端 `adapter/claude_code.rs::CLAUDE_NESTED_ENV`，
   * 而那个顺序**直接决定了送到远端的那条命令的字节**（`payload::usage_probe_payload`）。
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
   * ⚠ 类型刻意**从生成物借**而不是在这里写一遍那两个字面量：本文件里出现后端那张表的
   * 任何一个值都会被 `agent-profile-parity.vitest.ts` 判红（`KR93D1` 第三刀），
   * 而「借类型」与「抄一份值」是两件事。
   */
  resumeKind: AgentProfileRow["resumeKind"];
  /** resume 那个字面量（claude 是 `--resume`）。⚠ 它**不带形态**，形态看 `resumeKind`。 */
  resumeFlag: string;
};

/**
 * 取一份**考据齐全**的画像。
 *
 * 两种失败**都不静默**：agent 不在表里 ⇒ 抛 [`lookupAgentProfile`] 那句话；
 * 某一格是 `null`（没人考据过）⇒ 抛，并点名是哪一格。
 * 🔴 **不许在这里塞一个「回落到 claude」的兜底** —— 那正是 `KR93D3` 禁的那件事。
 */
export function fullAgentProfile(
  agent: string,
  table: readonly AgentProfileRow[] = AGENT_PROFILE_TABLE,
): FullAgentProfile {
  const got = lookupAgentProfile(agent, table);
  if (!got.known) throw new Error(got.message);
  const facts = got.facts;
  const researched = (cell: string[] | null, key: string): Set<string> => {
    if (cell === null) {
      throw new Error(
        `后端那张表里「${agent}」的 ${key} 是 null＝这一格今天没人考据过 ——` +
          " 不许拿别的 agent 那一份顶上（KR93D3）。",
      );
    }
    return new Set(cell);
  };
  return {
    agentTools: researched(facts.agentTools, "agentTools"),
    interactiveTools: researched(facts.interactiveTools, "interactiveTools"),
    diffTools: researched(facts.diffTools, "diffTools"),
    mdTools: researched(facts.mdTools, "mdTools"),
    livenessProcessNames: researched(facts.livenessProcessNames, "livenessProcessNames"),
    nestedEnvVars: [...facts.nestedEnvVars],
    defaultLauncher: facts.defaultLauncher,
    launcherAlias: facts.launcherAlias,
    resumeKind: facts.resumeKind,
    resumeFlag: facts.resumeToken,
  };
}

/**
 * **当前 agent 那一份画像** —— 本仓今天所有 `AGENT_PROFILE.*` 的消费者走的都是它。
 *
 * ⚠ 名字刻意留着（件文件 `§0b`：判据认的是**值从哪来**，不是名字）。
 * 是哪一个 agent 由后端 `adapter::active()` 说了算（生成物里的 `ACTIVE_AGENT`），
 * 不是这里挑的；表里没有它 ⇒ **模块加载当场抛**，不静默给一份空画像。
 */
export const AGENT_PROFILE = fullAgentProfile(ACTIVE_AGENT);
