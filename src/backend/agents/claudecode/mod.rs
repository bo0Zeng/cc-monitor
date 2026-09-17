//! Claude Code 适配层 —— daemon 里 Claude 专属知识的唯一住址（`S3`）。
//!
//! | 子模块 | 装什么 |
//! |---|---|
//! | [`paths`] | 配置目录怎么解析（环境变量名 + `.claude`）· `projects/` 与 `sessions/` 两个子目录 |
//! | [`records`] | 会话记录的后缀与命名（`<sid>.jsonl`） |
//! | [`liveness`] | 判活时"这个 cmdline 看起来像不像 Claude" |
//! | [`accounts`] | `.claude.json` 的信任判定（`projects[cwd].hasTrustDialogAccepted`） |
//! | [`resume`] | resume 的命令形状与会话名前缀（与 [`super::codex::resume`] 对称） |
//!
//! # ⚠ 搬进来的是**知识**，不是**机器**
//!
//! `S3` 的 Bx 实测推翻了"读盘族 = 两千行"这个印象：`history_query`(923) /
//! `search_query`(676) / `watcher`(4885) 里的 `jsonl` 绝大多数是**变量名与函数名**
//!（`jsonl_path` / `read_jsonl` / `newest_jsonl`）—— 那是**通用的流式读取机器**，与 agent 无关。
//! 真知识只有**带引号的那几处**（后缀判定 6 · 目录名 4 · 身份/账号 ~12 · 判活与 resume ~5）。
//! ⇒ 本层收的是那 ~30 处；机器留在原地，通过本层的一次函数调用拿知识。
//! 这正是 `D1` 那句「通用骨架 ~12000 行与 agent 无关」在代码上的兑现。
//!
//! # ⚠ 本层**不**代表 Claude 那半已经分干净
//!
//! 〔`S4b` 08-14 订正〕`claude_dir` 那个**参数名已经清了**（生产段 64 行 → 3 行，
//! 剩下的 3 处全是冻结的 wire 字段名）。
//! 而 `watcher`/`accounts_query`/`history_query` **仍然进不了** `S1` 的 `CORE_FILES` ——
//! `S4b` 实测：改完名之后这三个文件在 `S1` 六根针下还剩 **6 / 5 / 5** 处（共 16），其中
//! **12 处是 `crate::agents::claudecode::…` 这个适配层地址本身**（`history_query` 那 5 处**全是**），
//! 另 4 处是散文（`watcher` 两句 warn 里的 `claude` 一词、`accounts_query` 两处
//! `sessions/` 文案）。⇒ 卡点不再是参数名，是**通用层直接写死了一个 agent 的名字** ——
//! 那是 `L2`（接口）的题，归 `S6`。清单在 `agent_locality_guard::ADAPTER_CALL_SITES`。

pub(crate) mod accounts;
pub(crate) mod liveness;
pub(crate) mod paths;
pub(crate) mod records;
pub(crate) mod resume;

/// 本 agent 在 wire 上的 **`agent_kind` 值**〔`S5`〕。
///
/// ⚠ **是 `claude` 不是 `claudecode`** —— 模块名与 wire 值域是两件事，别顺手对齐：
/// 值域由既有契约定死（`ResumeSpec.agentKind` 逐字「缺/`""`/`"claude"`=claude」·
/// `session_added.agent_kind` 对 Claude **省略** ⇒ 缺 = claude），改它 = 改跨仓契约。
///
/// 它住在**适配层**而不是通用层，是为了让 `agents::REGISTRY` 那张注册表里
/// 一个 agent 名的字面量都没有（`D3`：agent 维度只许出现在值里 —— 而这个值的**来源**
/// 也该是那个 agent 自己）。
pub(crate) const AGENT_KIND: &str = "claude";

/// 本 agent 在这台机器上的 home 目录 —— **只答"它该在哪"，不答"在不在"**〔`S5`〕。
///
/// 「在不在」的判准是通用层的机器（`agents::visible_homes`），不是每家自己定一套 ——
/// 那正是 `S3` 立的分界（知识住适配层、机器留通用层）在发现这件事上的兑现。
///
/// 恒 `Some`：[`paths::resolve_home`] 有默认值（`$HOME/.claude`，再退 cwd 下的 `.claude`），
/// 解析不出来这种事对 Claude 不存在。签名仍取 `Option` 是为了与
/// [`super::codex::home`] 同形（那家真的可能答不出来）。
pub(crate) fn home() -> Option<std::path::PathBuf> {
    Some(paths::resolve_home())
}
