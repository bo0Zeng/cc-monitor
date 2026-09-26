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

use std::path::{Path, PathBuf};

/// Phase 2（Codex 泛化）：受支持的 agent 种类。Claude Code 是第一个、Codex 是「第二个样本」
/// （SS-1 说好的第二刀触发点）。monitor 先定义；backend（`src/backend`）与 frontend
/// 各自镜像（双写 parity，同 `turn_detect`/`usage` 现状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    ClaudeCode,
    /// Codex（〔LOC1b〕今天由 `kind_of_record_name` 按文件名形态派发）。
    Codex,
}

/// 从记录文件路径取 session_id 的策略（per-kind）。取代原 `sid_from_stem: bool`——Codex 的
/// `rollout-<ts>-<uuid>.jsonl` 文件名 stem **不等于** sid（sid 是末尾 UUID），bool 表达不了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidStrategy {
    /// 文件名 stem 即 sid（CC：`<sid>.jsonl`）。
    Stem,
    /// Codex `rollout-<YYYY-MM-DDThh-mm-ss>-<uuid>.jsonl` → 末 36 字符 UUID。
    CodexRollout,
}

/// 文件型 agent 的会话源布局(目录 / 命名约定)。把散落的「知道 CC 目录结构」字面量收这里,
/// 消除会话发现层(live / history / search / remote 四链)对具体子目录名的硬编码。
pub struct SessionLayout {
    // 〔LOC1b · 第四波 4D〕「会话记录子目录」「活性 pidfile 子目录」「会话记录扩展名」三格删了：它们的读者只有
    //   monitor 自己读本机会话 / 判活那几个函数（已删，见下面那块墓碑）；那几件事今天问本机后端，目录布局归后端 `agents/`。
    /// 任务追踪子目录(CC = `"tasks"`),可选。
    pub tasks_subdir: Option<&'static str>,
    /// 从记录文件路径取 sid 的策略(CC = `Stem`;Codex = `CodexRollout`)。
    pub sid_strategy: SidStrategy,
    // 〔CF1 · 2026-09-24〕「扫描时跳过的路径段」那一格（CC = `subagents`）随 monitor 自己那套 jsonl watcher 删了 ——
    //   它唯一的读者就是那条 watcher 的「是不是顶层会话记录文件」判定；本机会话内容改走本机后端的 `line` 帧之后，
    //   同一条规矩由后端那一份（`observe/watcher.rs` 的 subagent 路径判定）管。
}

/// 一个 agent CLI 的适配器。第一个实例 = [`claude_code::ClaudeCodeAdapter`]。
///
/// 只加**已接线**的方法(增量);记录解析(委托 `parser::parse_line`,SS-16 缝不动)、活性投影、
/// resume 命令等后续 Step 逐一并入。
pub trait AgentAdapter: Send + Sync {
    /// 稳定 id(如 `"claude-code"`)。
    fn id(&self) -> &'static str;
    /// agent 数据根目录(CC = `resolve_claude_dir` 的三级回退)。
    fn data_root(&self) -> Option<PathBuf>;
    /// 会话源布局。
    fn layout(&self) -> &SessionLayout;
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

/// 〔LOC1b · 4D〕按记录文件的**名字形态**判 [`AgentKind`]：`rollout-<ts>-<uuid>.jsonl` ⇒ Codex，其余 ⇒ Claude。
///
/// 从前另有一个按本机根前缀判的 `kind_of_path`〔散文墓碑〕；本函数不看本机有没有那一家的根 —— 冷读本机远端合成一条之后，远端的路径也要判得对
/// （那台机器的 Codex 根在哪，本机不知道）。形态口径与 [`session_id_from_path_with`] 的 `CodexRollout` 同一个函数。
pub fn kind_of_record_name(p: &Path) -> AgentKind {
    if codex_sid_from_rollout(p).is_some() {
        AgentKind::Codex
    } else {
        AgentKind::ClaudeCode
    }
}

/// F-MA:agent 数据根下的**任务追踪**目录(CC = `<root>/tasks`);该 agent 无此概念则 `None`。
pub fn tasks_dir(data_root: &Path) -> Option<PathBuf> {
    active().layout().tasks_subdir.map(|s| data_root.join(s))
}

/// Phase 2：按 layout 的 [`SidStrategy`] 取 sid（供 per-kind 派发/测）。
pub fn session_id_from_path_with(layout: &SessionLayout, p: &Path) -> Option<String> {
    match layout.sid_strategy {
        SidStrategy::Stem => p.file_stem().and_then(|s| s.to_str()).map(String::from),
        SidStrategy::CodexRollout => codex_sid_from_rollout(p),
    }
}

/// Codex `rollout-<YYYY-MM-DDThh-mm-ss>-<uuid>.jsonl` → 末尾 36 字符 UUID（= ThreadId =
/// session_meta.id）。时间戳内也含 `-`，故不能按 `-` 切；取 stem 末 36 字符并校验 UUID 形。
/// 非 rollout 前缀 / 过短 / 末段非 UUID → `None`（不臆造）。（`.jsonl.zst` 冷会话见 F2。）
fn codex_sid_from_rollout(p: &Path) -> Option<String> {
    let stem = p.file_stem().and_then(|s| s.to_str())?;
    let rest = stem.strip_prefix("rollout-")?;
    if rest.len() < 36 {
        return None;
    }
    // 末 36 用 `.get()`（非字节切片）→ 非字符边界（畸形多字节名）安全返 None、不 panic。
    // Phase G 审计修：原 `&rest[..]` 会在含多字节字符的畸形文件名上 panic、挂掉整个历史/用量扫描
    // （对齐 backend `codex::codex_sid_from_path` 已加固的 .get 写法，消两端 parity 发散）。
    let uuid = rest.get(rest.len() - 36..)?;
    is_uuid(uuid).then(|| uuid.to_string())
}

/// UUID 形校验：`8-4-4-4-12` 十六进制 + 固定位 `-`（第 8/13/18/23 字符）。
fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

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
static CLAUDE_AGENT_TOOLS: &[&str] = &["Agent", "Task"];
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
#[path = "../../../tests/bridge/adapter_tests.rs"]
mod tests;
