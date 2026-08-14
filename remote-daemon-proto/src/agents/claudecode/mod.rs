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
