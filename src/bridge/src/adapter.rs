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
/// （SS-1 说好的第二刀触发点）。monitor 先定义；daemon（`src/backend`）与 frontend
/// 各自镜像（双写 parity，同 `turn_detect`/`usage` 现状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    ClaudeCode,
    /// Codex（F1a 起 production 构造：`enabled_kinds`/`kind_of_path` 按会话根 `~/.codex` 派发）。
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
    /// 会话记录子目录(CC = `"projects"`)。
    pub sessions_subdir: &'static str,
    /// 活性 pidfile 子目录(CC = `"sessions"`)。
    pub liveness_subdir: &'static str,
    /// 任务追踪子目录(CC = `"tasks"`),可选。
    pub tasks_subdir: Option<&'static str>,
    /// 会话记录扩展名(CC = `"jsonl"`)。
    pub record_ext: &'static str,
    /// 从记录文件路径取 sid 的策略(CC = `Stem`;Codex = `CodexRollout`)。
    pub sid_strategy: SidStrategy,
    /// 扫描时跳过的路径段(CC = `["subagents"]`,子会话不当独立会话)。
    pub skip_segments: &'static [&'static str],
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

/// F-MA:agent 数据根下的**会话记录**目录(CC = `<root>/projects`)。收敛散落的 `.join("projects")`。
pub fn records_dir(data_root: &Path) -> PathBuf {
    data_root.join(active().layout().sessions_subdir)
}

/// Phase 2 F1a：**按 kind** 的会话记录目录(`<data_root>/<sessions_subdir>`;Claude=projects、Codex=sessions)。
pub fn records_dir_for(kind: AgentKind, data_root: &Path) -> PathBuf {
    data_root.join(for_kind(kind).layout().sessions_subdir)
}

/// Phase 2 F1a：本机**启用的 agent 种类**。Claude 恒启用;Codex 仅当其数据根的会话目录存在
/// (`~/.codex/sessions` 或 `$CODEX_HOME/sessions`)——不装 Codex 的机器上不纳入、零行为变化。
pub fn enabled_kinds() -> Vec<AgentKind> {
    let mut kinds = vec![AgentKind::ClaudeCode];
    let codex = for_kind(AgentKind::Codex);
    if let Some(root) = codex.data_root() {
        if root.join(codex.layout().sessions_subdir).is_dir() {
            kinds.push(AgentKind::Codex);
        }
    }
    kinds
}

/// Phase 2 F1a：所有启用 kind 的 `(kind, 会话记录根目录)`。发现层遍历它、按 kind 用对应 layout
/// 扫 + 解析(`parse_line` for Claude / `codex_record::to_jsonl_record` for Codex)。**显式传 kind**
/// (发现层枚举时即知 kind、无需按路径反解),per-file op 走 `session_id_from_path_with(for_kind(k).layout())`。
pub fn records_roots() -> Vec<(AgentKind, PathBuf)> {
    enabled_kinds()
        .into_iter()
        .filter_map(|k| {
            for_kind(k)
                .data_root()
                .map(|root| (k, records_dir_for(k, &root)))
        })
        .collect()
}

/// Phase 2 F1a：按记录文件路径判其 [`AgentKind`]（在哪个启用 kind 的会话根下）。都不在 → 默认
/// `ClaudeCode`（**零回归**：非 Codex 路径 = 原 Claude 行为；调用方仍会对该 kind 的根做前缀校验）。
pub fn kind_of_path(p: &Path) -> AgentKind {
    for (kind, root) in records_roots() {
        if p.starts_with(&root) {
            return kind;
        }
    }
    AgentKind::ClaudeCode
}

/// F-MA:agent 数据根下的**活性 pidfile** 目录(CC = `<root>/sessions`)。
pub fn liveness_dir(data_root: &Path) -> PathBuf {
    data_root.join(active().layout().liveness_subdir)
}

/// F-MA:agent 数据根下的**任务追踪**目录(CC = `<root>/tasks`);该 agent 无此概念则 `None`。
pub fn tasks_dir(data_root: &Path) -> Option<PathBuf> {
    active().layout().tasks_subdir.map(|s| data_root.join(s))
}

/// F-MA:路径扩展名是不是该 agent 的会话记录扩展(CC = `jsonl`)。
pub fn has_record_ext(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some(active().layout().record_ext)
}

/// F-MA:路径是否落在跳过段下(CC = `subagents`,子会话不当独立会话)。大小写不敏感。
pub fn is_skipped_path(p: &Path) -> bool {
    let skip = active().layout().skip_segments;
    p.components()
        .any(|c| skip.iter().any(|s| c.as_os_str().eq_ignore_ascii_case(s)))
}

/// F-MA:一个路径是不是该 agent 的**顶层会话记录文件**(扩展名对 + 不在跳过段下)。
pub fn is_record_file(p: &Path) -> bool {
    has_record_ext(p) && !is_skipped_path(p)
}

/// F-MA:从记录文件路径取 session_id(CC = `file_stem`)。约定不成立则 `None`。
/// **F1 仍走 `active()`（=Claude，零回归）**；`_with` 供 per-kind 测 + 后续 multi-kind 派发。
pub fn session_id_from_path(p: &Path) -> Option<String> {
    session_id_from_path_with(active().layout(), p)
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
    // （对齐 daemon `codex::codex_sid_from_path` 已加固的 .get 写法，消两端 parity 发散）。
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
// 收进 trait（顺带把 daemon 侧那份判活词表也接上，`daemon-api` F11）是下一刀的事。
//
// # `None` 与 `Some(&[])` 不是一回事
//
// `None` = **这一格今天没人考据过**；`Some(&[])` = 考据过、确实是空的。
// 把这两个值合并就是 `K-R92` 那一形（「一个值装了两件事」），`KR93D3` 明令禁止。

/// 盘上**所有**的 agent 种类 —— 与 [`enabled_kinds`]（本机装了哪几个）**不是同一个问题**。
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
/// 今天权威方在这里 —— 但 **daemon 那一侧仍是各写各的**（`agents/claudecode/liveness.rs` 的内联
/// 字面量），两侧仍靠那条对拍咬着。收成一份归 `daemon-api` F11，本件没做。
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
mod tests {
    use super::*;

    /// Phase 2 F1a：records_dir_for 按 kind 派生正确子目录（Claude=projects / Codex=sessions）。
    #[test]
    fn records_dir_for_per_kind() {
        let root = Path::new("/home/u/.claude");
        assert_eq!(
            records_dir_for(AgentKind::ClaudeCode, root),
            Path::new("/home/u/.claude/projects")
        );
        let croot = Path::new("/home/u/.codex");
        assert_eq!(
            records_dir_for(AgentKind::Codex, croot),
            Path::new("/home/u/.codex/sessions")
        );
    }

    /// enabled_kinds 恒含 Claude（零回归）；Codex 仅当 ~/.codex/sessions 存在时纳入（machine-dependent，
    /// 此处只锁 Claude 恒在 + records_roots 有对应 projects 根，Codex 分支由装了 Codex 的机器真机验证）。
    #[test]
    fn enabled_kinds_always_includes_claude() {
        assert!(enabled_kinds().contains(&AgentKind::ClaudeCode));
        let roots = records_roots();
        let claude = roots.iter().find(|(k, _)| *k == AgentKind::ClaudeCode);
        assert!(
            claude
                .map(|(_, d)| d.ends_with("projects"))
                .unwrap_or(false),
            "Claude 根应以 projects 结尾"
        );
    }

    /// kind_of_path：不在任何启用 kind 根下的路径 → 默认 `ClaudeCode`（**零回归**：非 Codex 路径
    /// 走原 Claude 行为；调用方仍会对该根做前缀校验挡非法路径）。真根下派发由真机集成验证。
    #[test]
    fn kind_of_path_defaults_to_claude_for_unrooted() {
        assert_eq!(
            kind_of_path(Path::new("/tmp/nowhere/x.jsonl")),
            AgentKind::ClaudeCode
        );
    }

    // ── `K-R93`：把前端那份画像**生成出去** ─────────────────────────────────
    //
    // `npm run gen:types` 逐字就是 `cd src/bridge && cargo test --lib export_bindings`
    // ⇒ 本条会被它跑到；门禁第六格 `generated` 随后判「已提交的那份与 Rust 源一不一致」。
    // ⇒ **改了上面那几张表而不重跑生成 ⇒ 门禁红**，这就是 `KR93D1` 要的那条牙。

    /// 生成物的头。「Do not edit this file manually」那句话是给
    /// `tests/generated-boundary-guard.vitest.ts` 那条判据看的，别改措辞。
    ///
    /// 🔴 **本段与下面 `TABLE_HEADER_TAIL` 刻意分成两个字面量，别合回去。**
    /// `guard_core::test_module_ranges` 用「**列 0 的右大括号**」判测试模块到哪儿收尾
    /// （它刻意不解析字符串 / 原始字符串，理由写在那个函数自己的头注里）。
    /// 这段 TS 里 `};`（`AgentProfileRow` 那个类型的收尾）一旦落在本 `.rs` 文件的列 0，
    /// **测试模块就在那里被切断**，后面的测试代码全被当成生产段。
    /// 〔实打，09-12 本件第一趟门禁：合成一个字面量 ⇒ `cargo` 当场**四条**红 ——
    /// `structural_scan.rs`（剥完仍残留测试属性）· `write_site_registry.rs`（把生成器那句
    /// `fs::write` 当成未申报的写盘落点）· `agent_dispatch_registry.rs` 两条（把测试段里的
    /// agent 名数成了生产耦合点）。**四条都不是假红，是那一刀真的把模块切断了。**
    /// 四条判据的逐字名字与读数住 `tests/evidence/K-R93-deathvalue.md`〕
    const TABLE_HEADER: &str = r#"// 本文件由 `src/bridge/src/adapter.rs` 的 `export_bindings_agent_profile_table` 生成
// （`npm run gen:types`）。Do not edit this file manually.
//
// `K-R93`：**前端那份 agent 画像的值来自后端**（`adapter.rs::agent_profile_facts`），
// 不再是 `src/agent-profile.ts` 里自己写死的一份常量 —— 那一份只认 claude，
// 接上后端的同一刻把 codex 那一格也补上了。
//
// ⚠ **`null` ≠ 空**：`null` = 这一格今天没人考据过（后端 `None`），**不许拿 claude 那份顶上**
// （`KR93D3`）；`[]` 才是「考据过、确实是空的」。

export type AgentProfileRow = {
  /** 这张表的键（= `agent-profile-golden.tsv` 第一列，也是 `ccm --agent` 收的那个名字）。 */
  agent: string;
  /** 后端适配器 id（`AgentAdapter::id()`）。 */
  adapterId: string;
  defaultLauncher: string;
  launcherAlias: string | null;
  resumeKind: "flag" | "subcommand";
  resumeToken: string;
  nestedEnvVars: string[];
  agentTools: string[] | null;
  interactiveTools: string[] | null;
  diffTools: string[] | null;
  mdTools: string[] | null;
  livenessProcessNames: string[] | null;
"#;

    /// 接着上面那一段 —— **第一行就是那个收尾的 `};`**（见上面为什么不能合并）。
    const TABLE_HEADER_TAIL: &str = r#"};

export const AGENT_PROFILE_TABLE: readonly AgentProfileRow[] = [
"#;

    /// `ACTIVE_AGENT` 那一格的头注（后端 `active()` 今天是谁，不是前端自己挑的）。
    const ACTIVE_HEADER: &str = r#"
/** 后端 `adapter::active()` 今天派发给谁 —— `AGENT_PROFILE` 就是它那一份。 */
"#;

    /// 一个 TS 串字面量。**这个生成器不做转义** —— 真出现要转义的字符就当场炸，不产坏 TS。
    fn ts_str(s: &str) -> String {
        assert!(
            !s.contains('"') && !s.contains('\\'),
            "画像里出现了要转义的字符：{s:?}"
        );
        format!("\"{s}\"")
    }

    fn ts_list(v: &[&str]) -> String {
        let items: Vec<String> = v.iter().map(|s| ts_str(s)).collect();
        format!("[{}]", items.join(", "))
    }

    /// `None` ⇒ `null`（**「没人考据过」，不是空数组**）。
    fn ts_opt_list(v: Option<&[&str]>) -> String {
        v.map(ts_list).unwrap_or_else(|| "null".to_string())
    }

    fn ts_opt_str(v: Option<&str>) -> String {
        v.map(ts_str).unwrap_or_else(|| "null".to_string())
    }

    fn row_fields(f: &AgentProfileFacts) -> Vec<(&'static str, String)> {
        let inter = ts_opt_list(f.interactive_tools);
        let live = ts_opt_list(f.liveness_process_names);
        vec![
            ("agent", ts_str(f.agent)),
            ("adapterId", ts_str(f.adapter_id)),
            ("defaultLauncher", ts_str(f.default_launcher)),
            ("launcherAlias", ts_opt_str(f.launcher_alias)),
            ("resumeKind", ts_str(f.resume_kind)),
            ("resumeToken", ts_str(f.resume_token)),
            ("nestedEnvVars", ts_list(f.nested_env)),
            ("agentTools", ts_opt_list(f.agent_tools)),
            ("interactiveTools", inter),
            ("diffTools", ts_opt_list(f.diff_tools)),
            ("mdTools", ts_opt_list(f.md_tools)),
            ("livenessProcessNames", live),
        ]
    }

    fn render_row(f: &AgentProfileFacts) -> String {
        let mut s = String::new();
        s.push_str("  {\n");
        for (key, value) in row_fields(f) {
            s.push_str("    ");
            s.push_str(key);
            s.push_str(": ");
            s.push_str(&value);
            s.push_str(",\n");
        }
        s.push_str("  },\n");
        s
    }

    /// 后端 [`active`] 今天是哪一个 agent（按 [`AgentAdapter::id`] 回查表键）。
    fn active_agent_key() -> &'static str {
        let id = active().id();
        for kind in ALL_AGENT_KINDS {
            let f = agent_profile_facts(kind);
            if f.adapter_id == id {
                return f.agent;
            }
        }
        panic!("`active()` 的 id `{id}` 不在 ALL_AGENT_KINDS 里 —— 表漏了一个 agent");
    }

    fn render_agent_profile_table() -> String {
        let mut s = String::from(TABLE_HEADER);
        s.push_str(TABLE_HEADER_TAIL);
        for kind in ALL_AGENT_KINDS {
            s.push_str(&render_row(&agent_profile_facts(kind)));
        }
        s.push_str("];\n");
        s.push_str(ACTIVE_HEADER);
        s.push_str("export const ACTIVE_AGENT: string = ");
        s.push_str(&ts_str(active_agent_key()));
        s.push_str(";\n");
        s
    }

    /// `K-R93`：生成 `src/generated/agent-profile-table.ts`。
    #[test]
    fn export_bindings_agent_profile_table() {
        let repo = crate::guard_support::repo_root();
        let out = repo.join("src/generated/agent-profile-table.ts");
        let body = render_agent_profile_table();
        std::fs::write(&out, body).unwrap_or_else(|e| panic!("写不进 {}：{e}", out.display()));
    }
}
