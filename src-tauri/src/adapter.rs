//! F-MA:agent 适配层。把 cc-monitor 对「Claude Code 具体形态」的假设(会话目录布局 / 记录解析 /
//! 活性 / resume 命令)收敛到 [`AgentAdapter`] 后面,Claude Code 是**第一个实例**。
//!
//! **第一刀只抽浅耦合点(会话源布局等字面量),不碰记录模型**——`JsonlRecord` 暂当规范模型,拆成
//! per-agent wire + 中立 `CanonicalRecord` 等第二个具体 agent 落地才动(只有一个样本时拆 = 投机,
//! 违 SS-1「别建完整统一格式、留逃生口就够」)。见 `plan/features/MA-multi-agent-adapter.md`。
//!
//! 增量长 trait:每收敛一类假设(布局→解析→活性→resume)才往 trait 加一个方法,避免未接线的死方法。

pub mod claude_code;

use std::path::{Path, PathBuf};

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
    /// 文件名(去扩展名)是否即 session_id(CC = true;读侧不需 `enc(cwd)` 编码)。
    pub sid_from_stem: bool,
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

/// 当前活跃适配器。第一刀写死 Claude Code(ZST,无分配);接第二个 agent 时改成可选/探测。
pub fn active() -> &'static dyn AgentAdapter {
    static CLAUDE: claude_code::ClaudeCodeAdapter = claude_code::ClaudeCodeAdapter;
    &CLAUDE
}

/// F-MA:agent 数据根下的**会话记录**目录(CC = `<root>/projects`)。收敛散落的 `.join("projects")`。
pub fn records_dir(data_root: &Path) -> PathBuf {
    data_root.join(active().layout().sessions_subdir)
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
pub fn session_id_from_path(p: &Path) -> Option<String> {
    if active().layout().sid_from_stem {
        p.file_stem().and_then(|s| s.to_str()).map(String::from)
    } else {
        None
    }
}
