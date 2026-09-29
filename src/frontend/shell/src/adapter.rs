//! F-MA:agent 适配层。把 cc-monitor 对「Claude Code 具体形态」的假设(会话目录布局 / 记录解析 /
//! 活性 / resume 命令)收敛到 [`AgentAdapter`] 后面,Claude Code 是**第一个实例**。
//!
//! **第一刀只抽浅耦合点(会话源布局等字面量),不碰记录模型**——`JsonlRecord` 暂当规范模型,拆成
//! per-agent wire + 中立 `CanonicalRecord` 等第二个具体 agent 落地才动(只有一个样本时拆 = 投机,
//! 违 SS-1「别建完整统一格式、留逃生口就够」)。见 `plan/features/MA-multi-agent-adapter.md`。
//!
//! 增量长 trait:每收敛一类假设(布局→解析→活性→resume)才往 trait 加一个方法,避免未接线的死方法。

pub mod claude_code;
pub mod codex;

use std::path::PathBuf;

/// Phase 2（Codex 泛化）：受支持的 agent 种类。Claude Code 是第一个、Codex 是「第二个样本」
/// （SS-1 说好的第二刀触发点）。monitor 先定义；backend（`src/backend`）与 frontend
/// 各自镜像（双写 parity，同 `turn_detect`/`usage` 现状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    ClaudeCode,
    /// Codex（〔MOD〕生产段今天只在画像表那一格用它 —— 读正文按文件名派发那一处随记录解释进了后端）。
    #[cfg_attr(not(test), allow(dead_code))]
    Codex,
}

// 〔MOD · `设计/90 §3` 判据 3〕「会话源布局」（`SessionLayout` · 取 sid 的策略 `SidStrategy`〔散文墓碑〕）删了：它最后的读者是
//   monitor 读正文那条分页器（按文件名判是哪一家、取 sid），那件事随记录解释进了后端（`agents::record_face_of`）。

/// 一个 agent CLI 的适配器。第一个实例 = [`claude_code::ClaudeCodeAdapter`]。
///
/// 只加**已接线**的方法(增量);记录解析(委托 `parser::parse_line`,SS-16 缝不动)、活性投影、
/// resume 命令等后续 Step 逐一并入。
pub trait AgentAdapter: Send + Sync {
    /// 稳定 id(如 `"claude-code"`)。
    fn id(&self) -> &'static str;
    /// agent 数据根目录(CC = `resolve_claude_dir` 的三级回退)。
    fn data_root(&self) -> Option<PathBuf>;
    /// resume/拉起前要从进程环境清洗掉的**嵌套会话** env(否则 agent 自认嵌套子会话、不写记录)。
    /// CC = `CLAUDECODE` / `CLAUDE_CODE_*`(spec §5);其它 agent 各不相同,无则空。
    fn nested_env_to_scrub(&self) -> &'static [&'static str];
    /// resume 一个已存在会话的命令 flag(CC = `--resume`);别的 agent 可能是 `--continue`/`resume` 等。
    fn resume_flag(&self) -> &'static str;
    /// 默认拉起二进制名(CC = `claude`)。
    fn default_launcher(&self) -> &'static str;
    /// 默认拉起的**别名/wrapper**(CC = `cc`,用户的 shell 集成 wrapper);优先它、检测不到才回退
    /// `default_launcher`。无别名返 `None`。
    fn launcher_alias(&self) -> Option<&'static str>;
}

/// Phase 2：按 [`AgentKind`] 取适配器(ZST static,无分配)。
pub fn for_kind(kind: AgentKind) -> &'static dyn AgentAdapter {
    static CLAUDE: claude_code::ClaudeCodeAdapter = claude_code::ClaudeCodeAdapter;
    static CODEX: codex::CodexAdapter = codex::CodexAdapter;
    match kind {
        AgentKind::ClaudeCode => &CLAUDE,
        AgentKind::Codex => &CODEX,
    }
}

/// 当前活跃适配器。**F1 仍默认 Claude Code**(零回归——所有现有 caller 走 active() 行为不变);
/// 后续 slice 起按会话根(`~/.claude` vs `~/.codex`)per-kind 派发,届时发现层改传 kind、不再走全局。
pub fn active() -> &'static dyn AgentAdapter {
    for_kind(AgentKind::ClaudeCode)
}

// 〔LOC1b · 第四波 4D〕这里原来还有八个「替本机读盘找根 / 判记录文件」的函数（`records_dir` · `records_dir_for` · `enabled_kinds` ·
//   `records_roots` · `kind_of_path` · `liveness_dir` · `has_record_ext` · `session_id_from_path`，都〔散文墓碑〕）：它们唯一的调用方是
//   monitor 自己读本机会话 / 判活 / 建索引的那几份实现，而冷读 · 判活 · 搜索都改问本机后端了（本机远端同一条路）⇒ 零调用方，删。
//   按 agent 找记录目录 / 判记录文件的活今天住后端的适配层（`src/backend/agents/`）。

// 〔MOD〕按文件名形态判是哪一家（`kind_of_record_name`）· 取 sid（`session_id_from_path_with` · `codex_sid_from_rollout` · `is_uuid`）〔散文墓碑〕
//   〔散文墓碑〕删：唯一调用方（读正文的分页器）随记录解释进了后端，同一件事住后端 `agents/codex/parse.rs::codex_sid_from_path`。

// ── `K-R93`（09-12）：**前端那份 agent 画像的取数口** ────────────────────────────
//
// # 它治的是什么
//
// `K-R54` 表第 11 行：同一张 agent 适配表盘上有**两份** —— 后端这一份（claude ＋ codex）
// 与前端 `src/agent-profile.ts` 的 `AGENT_PROFILE`（🔴 **只有 claude**）。
// 前端那份从此不再自己写死：值由下面这个取数口给出，经生成物
// `src/generated/agent-profile-table.ts`（本文件的 `export_bindings_agent_profile_table`
// 生成，`npm run gen:types` 重跑）送到 TS 那一侧。
// ⇒ **删掉前端那一份的同一刻，codex 那一格也补上了**（不是回归，是把一格漏的补上）。
//
// # 为什么这几张表住在这里，而不是各自的 adapter 模块里
//
// `AgentAdapter` trait 今天没有「工具名 / 判活进程名」这几个方法，加进去要动
// `adapter/claude_code.rs` 与 `adapter/codex.rs` —— 而 `K-R93` 的写区只给了本文件这一格
//（件文件 `§2` 逐字「只加取数口，不动分派」）。⇒ **先住这里，住址写明，不假装它是终点**：
// 收进 trait（顺带把后端侧那份判活词表也接上，`backend-api` F11）是下一刀的事。
//
// # `None` 与 `Some(&[])` 不是一回事
//
// `None` = **这一格今天没人考据过**；`Some(&[])` = 考据过、确实是空的。
// 把这两个值合并就是 `K-R92` 那一形（「一个值装了两件事」），`KR93D3` 明令禁止。

/// 盘上**所有**的 agent 种类 —— 与「本机装了哪几个」（从前的 `enabled_kinds`〔散文墓碑〕）**不是同一个问题**。
/// 加一个 `AgentKind` 而忘了这里 ⇒ [`agent_profile_facts`] 的 `match` 编译不过。
pub const ALL_AGENT_KINDS: [AgentKind; 2] = [AgentKind::ClaudeCode, AgentKind::Codex];

/// 一个 agent 的**画像**：前端那份 `AGENT_PROFILE` 今天用到的每一格，加上「它是谁」。
///
/// 五个 `Option` 字段的 `None` 读作**「这一格今天没人考据过」**，不是「空的」。
#[allow(dead_code)] // 消费方在 TS 那一侧（生成物）；Rust 这侧只有生成器与判据读它 —— 不假装它在别处在用。
pub struct AgentProfileFacts {
    /// 这张表的键（= `agent-profile-golden.tsv` 第一列，也是 `ccm --agent` 收的那个名字）。
    pub agent: &'static str,
    /// 后端适配器 id（[`AgentAdapter::id`]）。
    pub adapter_id: &'static str,
    pub default_launcher: &'static str,
    pub launcher_alias: Option<&'static str>,
    /// resume 的**调用形态**：`flag`（`claude --resume <sid>`）/ `subcommand`（`codex resume <sid>`）。
    pub resume_kind: &'static str,
    pub resume_token: &'static str,
    pub nested_env: &'static [&'static str],
    pub agent_tools: Option<&'static [&'static str]>,
    pub interactive_tools: Option<&'static [&'static str]>,
    pub diff_tools: Option<&'static [&'static str]>,
    pub md_tools: Option<&'static [&'static str]>,
    pub liveness_process_names: Option<&'static [&'static str]>,
}

/// 子 agent 工具（展开 = 子会话）。〔`K-R93` 从 `src/agent-profile.ts` 搬来，值逐字未改〕
/// 〔DUP2 · J19〕值住共享 crate `agent_tools_core`（后端会话事实的 agent 列表用同一份；两半编译期不许互咬 ⇒ 共享 crate）。
static CLAUDE_AGENT_TOOLS: &[&str] = &agent_tools_core::CLAUDE_AGENT_TOOLS;
/// 交互工具（agent 在等用户决定）。〔同上〕
static CLAUDE_INTERACTIVE_TOOLS: &[&str] = &["AskUserQuestion", "ExitPlanMode"];
/// 写类工具（行级 diff）。〔同上〕
static CLAUDE_DIFF_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit"];
/// 结果默认按 markdown 渲染的工具。〔同上〕
static CLAUDE_MD_TOOLS: &[&str] = &["Read", "Grep", "WebFetch", "NotebookRead", "TodoWrite"];
/// tmux 前台命令算该 agent 的会话（CC 是 Node CLI，视启动路径也可能报解释器）。〔同上〕
///
/// ⚠ 这一格从前**没有权威方**（`tests/liveness-process-names-parity.vitest.ts` 的头注逐字说过
/// 「`agent-profile-golden.tsv` 只有 4 个 key，不含这一项；`AgentAdapter` trait 也没有这个方法」）。
/// 今天权威方在这里 —— 但 **backend 那一侧仍是各写各的**（`agents/claudecode/liveness.rs` 的内联
/// 字面量），两侧仍靠那条对拍咬着。收成一份归 `backend-api` F11，本件没做。
static CLAUDE_LIVENESS_PROCESS_NAMES: &[&str] = &["claude", "node"];

/// resume 的调用形态 —— 与 `backend/control/agent_profile_parity.rs` 那条**同一条推法**：
/// 以 `--` 开头 = flag，否则 = 子命令。
fn resume_kind_of(token: &str) -> &'static str {
    if token.starts_with("--") {
        "flag"
    } else {
        "subcommand"
    }
}

/// `K-R93`：按 kind 取那份画像（**只取数，不参与派发**）。
#[allow(dead_code)] // 同上：消费方在 TS 那一侧。
pub fn agent_profile_facts(kind: AgentKind) -> AgentProfileFacts {
    let a = for_kind(kind);
    let agent = match kind {
        AgentKind::ClaudeCode => "claude",
        AgentKind::Codex => "codex",
    };
    let facts = AgentProfileFacts {
        agent,
        adapter_id: a.id(),
        default_launcher: a.default_launcher(),
        launcher_alias: a.launcher_alias(),
        resume_kind: resume_kind_of(a.resume_flag()),
        resume_token: a.resume_flag(),
        nested_env: a.nested_env_to_scrub(),
        // 下面五格：claude 那份在下面填上；**codex 那五格今天没人考据过**（不是空的）。
        agent_tools: None,
        interactive_tools: None,
        diff_tools: None,
        md_tools: None,
        liveness_process_names: None,
    };
    match kind {
        AgentKind::ClaudeCode => AgentProfileFacts {
            agent_tools: Some(CLAUDE_AGENT_TOOLS),
            interactive_tools: Some(CLAUDE_INTERACTIVE_TOOLS),
            diff_tools: Some(CLAUDE_DIFF_TOOLS),
            md_tools: Some(CLAUDE_MD_TOOLS),
            liveness_process_names: Some(CLAUDE_LIVENESS_PROCESS_NAMES),
            ..facts
        },
        // ⚠ Codex 的工具名 / 判活进程名**本仓今天没有考据过的读数**（`codex_record.rs` 的真机样本里
        // 只出现过 `shell` 一个名字，那不足以当一张表）⇒ 五格留 `None`＝「不知道」。
        // 编一份出来，或者拿 claude 那份顶上，都是 `KR93D3` 禁的那件事。
        AgentKind::Codex => facts,
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/adapter_tests.rs"]
mod tests;
