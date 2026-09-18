//! Subagent JSONL 按需加载。
//!
//! Claude Code 的 Task/Agent tool_use 触发的 subagent 不出现在主 session JSONL，
//! 而是独立写到：
//!   `<encoded-cwd>/<parent-session-id>/subagents/agent-<hash>.jsonl`
//! 同目录还有 `agent-<hash>.meta.json`：`{agentType, description}`。
//!
//! 主 watcher 跳过这些文件；本模块提供一个 IPC 命令，前端在用户展开 Task 折叠
//! 卡时调用，按 (parent_jsonl_path, tool_use.description, tool_use.timestamp)
//! 定位并加载对应 subagent JSONL。
//!
//! # `K-R94`（09-12）：**「找」也交给后端了**
//!
//! 改前两条路只共用了「挑」那一半（`pick_closest`）：远端让 daemon 列候选，
//! **而本机自己做了三样** —— 候选枚举（`read_dir` 扫 `*.meta.json`）· 读首行时间戳 ·
//! 读 jsonl。这三样后端侧的 `--list-subagents` / `--read-session` 早就有了。
//!
//! ⇒ 本模块现在只剩**一条**流程，两条路唯一的差别是**谁去跑那条查询**（[`Backend`]）：
//!
//! 1. `--list-subagents <父会话 jsonl>` ⇒ 后端逐行吐 `{path, description, timestamp}`
//! 2. [`choose_subagent`]：按 `description` 精确串等筛 ＋ [`pick_closest`] 按时间戳挑最近
//!    —— **纯函数，不碰文件系统**；两条路喂进来的是同一种东西
//! 3. `--read-session <选中的 jsonl>` ⇒ 原样透传字节，本侧走既有的 [`parse_line`]
//!
//! 这与 `K28`（前端不许自己发明对外行为，一切对外都经后端）/ `K33`（一件事只许有一处实现）
//! 同向：「有哪些候选」是**后端**回答的问题，本侧只负责在候选里挑。
//!
//! ⚠ **两条 transport 之间一条如实登记的差别**（不在本模块能收的范围里）：
//! 远端那条（`run_list_query`）带 **30s 整体超时**与**单行上限**；本机那条
//! （`local_query::run_query`）**两样都没有** —— 那一层刻意把超时留给调用方（见它自己的头注）。
//! subagent 通常是短命的侧任务、文件很小，但一个长跑的 subagent 可能撞上远端那 30s。
//! 真要收，得把本命令改成**流式**（同 `stream_read_remote_session` 那条 channel 路），
//! 那会改它对前端的返回形状 —— 是另一件事，不在本件里顺手做。

use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SubagentLoadResult {
    /// 命中的 jsonl 文件路径（用于前端 debug / 状态栏显示）
    pub path: String,
    /// agentId（从文件名 `agent-<id>.jsonl` 提取）
    pub agent_id: String,
    pub records: Vec<JsonlRecord>,
}

/// **两条路唯一的差别**：谁去跑那条一次性查询。
///
/// 本机 = exec 一次 sidecar 拿 stdout；远端 = 经 ssh exec 同一个二进制。
/// 定框 `C1` 逐字「本地 = 不走 ssh 的远端」——⇒ 子命令、参数、解析、挑选**全是同一份**。
enum Backend {
    Local,
    Remote(Box<crate::ssh_source::RemoteConfig>),
}

impl Backend {
    /// `origin` 缺省 / 空串 = 本机；否则按 label 取那台远端的配置。
    fn for_origin(origin: Option<&str>) -> Result<Self, String> {
        match origin.filter(|o| !o.is_empty()) {
            Some(o) => {
                let cfg = crate::remote_history::require_cfg_by_label(o)?;
                Ok(Backend::Remote(Box::new(cfg)))
            }
            None => Ok(Backend::Local),
        }
    }

    /// 报错文案里的「谁」—— 本机 / 哪台远端。
    fn whose(&self) -> String {
        match self {
            Backend::Local => "本机".to_string(),
            Backend::Remote(cfg) => format!("远端 [{}]", cfg.origin_label()),
        }
    }

    /// 跑一条一次性查询。`argv[0]` 是子命令，其余是它的参数。
    ///
    /// 出的是**逐行、已 trim、已剔空行**的输出 —— 两条路形状一致
    /// （远端那条由 `run_list_query` 保证，本机这条在 [`run_local_query`] 里对齐）。
    async fn query(&self, argv: &[&str]) -> Result<Vec<String>, String> {
        match self {
            Backend::Local => run_local_query(argv),
            Backend::Remote(cfg) => {
                // 自由文本（路径）逐个过 `shell_quote`；子命令本身是字面量。
                let mut args = argv[0].to_string();
                for a in &argv[1..] {
                    args.push(' ');
                    args.push_str(&crate::ssh_source::shell_quote(a));
                }
                crate::remote_history::run_list_query(cfg, &args).await
            }
        }
    }
}

/// 本机那条 transport：exec 一次 sidecar 拿 stdout。
///
/// ⚠ 定框 §5：**「后端不在」与「查询失败」不许压成同一句话** ——
/// 前者该提示用户装/起后端，后者该把原因原样端出来。
fn run_local_query(argv: &[&str]) -> Result<Vec<String>, String> {
    use crate::backend::observe::local_query::{run_query, QueryOutcome};
    match run_query(
        env!("CCM_TARGET_TRIPLE"),
        argv,
        &*crate::spawn_managed::local_backend_one_shot_query(),
    ) {
        QueryOutcome::Ok(stdout) => Ok(nonempty_lines(&stdout)),
        QueryOutcome::NoBackend(reason) => Err(format!("本机后端不在：{reason}")),
        QueryOutcome::Failed { code, stderr } => {
            let sub = argv[0];
            let msg = stderr.trim();
            Err(format!("本机后端 {sub} 查询失败（退出码 {code:?}）：{msg}"))
        }
    }
}

/// 与 `run_list_query` 的出参形状对齐：逐行、trim 过、空行剔掉。
fn nonempty_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

#[tauri::command]
pub async fn load_subagent(
    parent_jsonl_path: String,
    description: String,
    tool_use_timestamp: String,
    origin: Option<String>,
) -> Result<SubagentLoadResult, String> {
    let backend = Backend::for_origin(origin.as_deref())?;
    // 深度防御（与 `stream_read_remote_session` 同一条纪律）：路径来自前端，本侧先做廉价校验；
    // 真正的越权读由后端的 `fence_under_projects` 兜底。
    // ⚠ `K-R94` 起这道校验**两条路都过** —— 改前只有远端那条有，而「同一个入参、两种把关」
    // 正是 `KR94D3` 说的那种两条路不一致。
    if parent_jsonl_path.contains("..") || !parent_jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法父会话路径: {parent_jsonl_path}"));
    }

    let list_argv = ["--list-subagents", parent_jsonl_path.as_str()];
    let listing = backend.query(&list_argv).await?;
    let Some(picked) = choose_subagent(&listing, &description, &tool_use_timestamp) else {
        let whose = backend.whose();
        return Err(format!(
            "{whose} 上没有 description={description:?} 的 subagent"
        ));
    };
    let picked_str = picked.to_string_lossy().into_owned();
    let agent_id = extract_agent_id(&picked).unwrap_or_default();

    // 内容走**既有的** `--read-session`（原样透传字节，本侧走既有解析）：subagent jsonl 就在
    // `<records 根>/<slug>/<sid>/subagents/` 下，那条围栏本来就放行它，不用新造读口。
    let read_argv = ["--read-session", picked_str.as_str()];
    let raw = backend.query(&read_argv).await?;
    let mut records = Vec::new();
    for line in &raw {
        match parse_line(line) {
            Ok(Some(rec)) => records.push(rec),
            Ok(None) => {}
            Err(e) => tracing::warn!("subagent parse skip: {e}"),
        }
    }

    Ok(SubagentLoadResult {
        path: picked_str,
        agent_id,
        records,
    })
}

/// 从**后端给的候选清单**里挑一个。
///
/// ★★ **纯函数：不碰文件系统，也不知道自己在为哪条路服务。**
/// 入参是 `--list-subagents` 的输出行（每行 `{path, description, timestamp}`）。
/// ⇒「候选集由后端决定」在这里是**类型上的**事实：这个函数没有别的地方能变出候选来。
///
/// ★ **筛选留在本侧**，不在 daemon（`C1`：挑选逻辑只准有一份；daemon 侧那半由
/// `the_daemon_never_matches_or_ranks_subagents` 钉住它只列、不挑）。
fn choose_subagent(
    listing: &[String],
    description: &str,
    tool_use_timestamp: &str,
) -> Option<PathBuf> {
    let mut metas: Vec<(PathBuf, Option<String>)> = Vec::new();
    for line in listing {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if v.get("description").and_then(|d| d.as_str()) != Some(description) {
            continue;
        }
        let Some(path) = v.get("path").and_then(|p| p.as_str()) else {
            continue;
        };
        // 时间戳拿不到就是 `None` —— 后端给 `null` 与**根本没这个键**落到同一档。
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .map(str::to_string);
        metas.push((PathBuf::from(path), ts));
    }
    if metas.is_empty() {
        return None;
    }
    Some(pick_closest(metas, tool_use_timestamp))
}

/// 多个 description 相同的候选 → 取首行 timestamp 与 `tool_use_timestamp` 差距最小的。
///
/// ★★ **P7c-1：它吃 `(路径, 时间戳)` 对，不再自己去读文件。**
///
/// 原来它拿 `Vec<PathBuf>` 并在内部读首行时间戳 —— 那样**只有本机能用**
/// （远端的文件不在本机文件系统上）。改成吃对之后，本机与远端喂给它的是**同一种东西**，
/// 挑选逻辑因此只有一份 —— 定框 `C1` 要的正是这个。
///
/// ⚠ `K-R94` **刻意没动它一个字**（射程 `§0b`）：本件收的是「找」那一半，
/// 已经收好的这一半再动一次就是把它拆开。今天它只剩**一个**调用点（[`choose_subagent`]）。
///
/// **缺时间戳那一档的处置逐字写在这里**：拿不到（`None`），或 `tool_use_timestamp`
/// 自己解析不出来 ⇒ 排序键取 `i64::MAX`；而 `sort_by_key` 是**稳定**排序 ⇒
/// 全缺时保持后端给的次序、取第一条，部分缺时**有时间戳的一定排在前面**。
/// 它**不报错** —— 两条路同走这一档（`KR94D3` ②）。
fn pick_closest(
    mut candidates: Vec<(PathBuf, Option<String>)>,
    tool_use_timestamp: &str,
) -> PathBuf {
    if candidates.len() == 1 {
        return candidates.remove(0).0;
    }
    let target = crate::utils::parse_iso8601_ms(tool_use_timestamp);
    candidates.sort_by_key(|(_, ts)| {
        let first_ts = ts.as_deref().and_then(crate::utils::parse_iso8601_ms);
        match (target, first_ts) {
            (Some(t), Some(f)) => (f - t).abs(),
            _ => i64::MAX,
        }
    });
    candidates.into_iter().next().unwrap().0
}

// P3 归并：parse_ts_ms 已搬到 utils::parse_iso8601_ms（多处复用）。
// 跨月 / 跨年 / 闰年单调性由 utils::days_from_civil 保证（utils 自带回归测试）。

fn extract_agent_id(jsonl_path: &Path) -> Option<String> {
    let stem = jsonl_path.file_stem()?.to_str()?;
    // 文件名形如 agent-<hash>
    stem.strip_prefix("agent-").map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{choose_subagent, extract_agent_id};
    use crate::backend::observe::local_query::{run_query, QueryOutcome};
    use std::path::PathBuf;

    /// 造一行后端 `--list-subagents` 的输出。
    fn listed(path: &str, description: &str, timestamp: Option<&str>) -> String {
        let ts = match timestamp {
            Some(t) => serde_json::Value::String(t.to_string()),
            None => serde_json::Value::Null,
        };
        let v = serde_json::json!({
            "path": path,
            "description": description,
            "timestamp": ts,
        });
        v.to_string()
    }

    /// ★★★ `KR94D1` 的**行为**判据：**候选是后端给的，不是本机自己从盘上枚举的。**
    ///
    /// # 为什么不是「查代码里还有没有 `read_dir`」
    ///
    /// 那是判**写法** —— 换个名字（换一个目录遍历库 / 包一层 / 先把路径存进一个不叫那个
    /// 名字的变量；⚠ 这里**刻意不逐字写出那个库名** —— `scanning_guard_registry` 认的
    /// 就是那几个字面量，写出来它会把本文件的测试段当成「又一处裸遍历」）
    /// 就绕过去了，而绕过去之后**候选仍然来自本机的盘**。本条判的是**性质**，
    /// 靠的是把两边摆到对立面：
    ///
    /// | 盘上 | 后端 | 期望 |
    /// |---|---|---|
    /// | **有**两个货真价实的候选（真文件） | **不在**（开发树没有 sidecar） | **必须报「后端不在」** |
    ///
    /// ⇒ 只要它还从盘上枚举，就会**成功**返回其中一个 ⇒ 本条当场红。
    /// 这正是 `KR94D1` 第 ③ 刀（「本机退回自己 `read_dir` ⇒ 必须红」）的可执行形态。
    ///
    /// ⚠ 它**不证明** happy path 对（那要真 sidecar，属 e2e）。只杀「悄悄读本机盘」这一类。
    #[tokio::test]
    async fn the_candidate_set_comes_from_the_backend_not_from_this_machines_disk() {
        // 前提自检：本测试环境**必须**没有 sidecar，否则下面那条断言会走 happy path 而空转。
        let probe = run_query(
            env!("CCM_TARGET_TRIPLE"),
            &["--list-subagents"],
            &*crate::spawn_managed::local_backend_one_shot_query(),
        );
        assert!(
            matches!(probe, QueryOutcome::NoBackend(_)),
            "测试环境里居然找得到 sidecar —— 本条的前提不成立，下面那条断言会空转。\n\
             （若哪天单测环境真带 sidecar，本条要改成显式指一个不存在的 target triple）"
        );

        // 盘上摆两个**货真价实**的候选：meta 描述精确匹配、jsonl 首行有时间戳。
        // 改前那条路会从这里挑出一个来。
        let tmp = std::env::temp_dir().join(format!("ccm-k-r94-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let sub = tmp.join("parent").join("subagents");
        std::fs::create_dir_all(&sub).expect("建临时 subagents 目录");
        for (id, ts) in [
            ("aaa", "2026-09-12T10:00:00.000Z"),
            ("bbb", "2026-09-12T11:00:00.000Z"),
        ] {
            std::fs::write(
                sub.join(format!("agent-{id}.meta.json")),
                r#"{"agentType":"general","description":"找它"}"#,
            )
            .expect("写 meta");
            std::fs::write(
                sub.join(format!("agent-{id}.jsonl")),
                format!("{{\"type\":\"user\",\"timestamp\":\"{ts}\"}}\n"),
            )
            .expect("写 jsonl");
        }
        let parent = tmp.join("parent.jsonl");
        std::fs::write(&parent, "").expect("写父会话");

        let got = super::load_subagent(
            parent.to_string_lossy().into_owned(),
            "找它".to_string(),
            "2026-09-12T10:30:00.000Z".to_string(),
            None,
        )
        .await;
        let _ = std::fs::remove_dir_all(&tmp);

        let err = got.expect_err(
            "盘上摆着两个候选、而本机后端不在，它却给出了结果 ——\n\
             ⇒ 候选是它**自己从盘上枚举**出来的，不是后端给的（`KR94D1` 第 ③ 刀）。",
        );
        assert!(
            err.contains("本机后端不在"),
            "报错没说清是「后端不在」（定框 §5：它与「查询失败」对用户是两种处境）：{err}"
        );
    }

    /// ★★ `KR94D1` 第 ①② 刀：**后端给的候选变了，结果就得跟着变。**
    ///
    /// 五组读数摆在一起才说得清「跟」是什么意思：换一份候选 ⇒ 换一个答案；
    /// 一条都不给 ⇒ **没得挑**（不许从别处变出一个来）；描述不匹配 ⇒ 同样没得挑。
    #[test]
    fn changing_what_the_backend_lists_changes_what_gets_picked() {
        let at = "2026-09-12T10:30:00.000Z";

        // ① 后端给 a / b，a 更近 ⇒ 挑 a
        let l1 = vec![
            listed("/r/agent-a.jsonl", "找它", Some("2026-09-12T10:29:00.000Z")),
            listed("/r/agent-b.jsonl", "找它", Some("2026-09-12T12:00:00.000Z")),
        ];
        assert_eq!(
            choose_subagent(&l1, "找它", at),
            Some(PathBuf::from("/r/agent-a.jsonl"))
        );

        // ② 后端换了一份候选（a/b 都不在了）⇒ 结果必须跟着换成 c
        let l2 = vec![listed(
            "/r/agent-c.jsonl",
            "找它",
            Some("2026-09-12T10:31:00.000Z"),
        )];
        assert_eq!(
            choose_subagent(&l2, "找它", at),
            Some(PathBuf::from("/r/agent-c.jsonl")),
            "后端换了候选而结果没跟 —— 那候选就不是后端决定的"
        );

        // ③ 后端一条都不给 ⇒ 没得挑
        assert_eq!(
            choose_subagent(&[], "找它", at),
            None,
            "后端零候选却挑出了东西 —— 那个东西只可能来自别处"
        );

        // ④ description 不匹配的不许混进来（筛选留在本侧，daemon 不挑）
        let l3 = vec![listed(
            "/r/agent-d.jsonl",
            "别的",
            Some("2026-09-12T10:30:00.000Z"),
        )];
        assert_eq!(choose_subagent(&l3, "找它", at), None);

        // ⑤ 后端给的行不是 JSON / 少字段 ⇒ 跳过那一行，不整条崩
        let l4 = vec![
            "这不是 json".to_string(),
            r#"{"description":"找它"}"#.to_string(),
            listed("/r/agent-e.jsonl", "找它", Some(at)),
        ];
        assert_eq!(
            choose_subagent(&l4, "找它", at),
            Some(PathBuf::from("/r/agent-e.jsonl"))
        );
    }

    /// ★★ `KR94D2`（**纪律 ⑱ 的预防**）：本件在删一段路，
    /// **不许把已经共用好的 `pick_closest` 连带砍了，也不许长出第二个挑选实现。**
    ///
    /// 改前是「定义 ＋ 两个调用点 = 3 处」（本机一条、远端一条各调一次）。
    /// `K-R94` 把两条路收成一条 ⇒ 现在是**定义 ＋ 一个调用点 = 2 处**。
    /// ⚠ 这个数**变小是因为调用点合并了，不是因为实现被砍了** ——
    /// 所以下面同时钉住「定义仍恰好一处」与「唯一的入口是 `choose_subagent`」。
    #[test]
    fn the_only_picker_is_still_pick_closest_and_there_is_only_one_of_it() {
        let prod = guard_core::production_code(include_str!("subagent.rs"));
        guard_core::assert_no_test_code("subagent.rs", &prod);

        assert_eq!(
            prod.matches("fn pick_closest(").count(),
            1,
            "`pick_closest` 的**定义**不再恰好一处 —— 要么被砍了（纪律 ⑱），要么长出了第二份"
        );
        assert_eq!(
            prod.matches("pick_closest(").count(),
            2,
            "`pick_closest` 的定义 ＋ 唯一调用点 = 2 处。\n\
             变多 = 有人另起了一条挑选路；变少 = 挑选那一半被连带砍了（纪律 ⑱）。"
        );
        assert_eq!(
            prod.matches("choose_subagent(").count(),
            2,
            "「筛 ＋ 挑」的入口 `choose_subagent` 的定义 ＋ 唯一调用点 = 2 处 ——\n\
             两条路必须共用它；多一个调用点就意味着有人给某一条路开了小灶。"
        );

        // 它不许再自己去读文件 —— 那样只有本机能用，远端那条会被逼着复制一份（`C1` 排除）。
        // ⚠ 窗口要**按函数边界**截，不能拍一个字节数（第一版取 900 字节，越过了函数末尾）。
        let at = prod.find("fn pick_closest(").expect("找不到 pick_closest");
        let rest = &prod[at + "fn pick_closest(".len()..];
        let end = rest.find("\nfn ").unwrap_or(rest.len());
        let body = &rest[..end];
        // ⚠ 「枚举目录」那个 needle **运行时拼**：写成字面量的话，
        //   `scanning_guard_registry` 认的就是那四个字面量，会把本文件的测试段
        //   当成「又一处裸遍历目录的判据」而当场红（09-12 实打过一次）。
        let w = format!("read_{}(", "dir");
        for banned in [w.as_str(), "File::open(", "read_to_string(", "BufReader"] {
            assert!(
                !body.contains(banned),
                "`pick_closest` 里出现了 {banned:?} —— 它又自己去读文件了，远端那条就用不了它"
            );
        }
        // ★★ **名字之外的那一半**：光数名字挡不住「换个名字复制一份」——
        //    `pick_closest_v2(` 里**不含** `pick_closest(` 这个子串（09-12 实打确认）。
        //    ⇒ 再钉**排序/比较那几样原语**：它们在生产段里出现几次、而且必须**全在**
        //    `pick_closest` 体内。复制一份挑选实现，一定会把其中某个数顶上去。
        let rankers = [
            ("sort_by_key(", 1usize),
            ("parse_iso8601_ms", 2),
            (".abs()", 1),
        ];
        for (needle, want) in rankers {
            let got = prod.matches(needle).count();
            assert_eq!(
                got, want,
                "生产段里 {needle:?} 出现了 {got} 次（期望 {want}）—— 多半是有人复制了一份挑选实现"
            );
            assert_eq!(
                body.matches(needle).count(),
                want,
                "{needle:?} 跑到 `pick_closest` 体外去了 —— 排序不再只有一处"
            );
        }
        // 同一条性质的另一半：**筛 ＋ 挑**那一段整体不许碰文件系统。
        let ca = prod
            .find("fn choose_subagent(")
            .expect("找不到 choose_subagent");
        let crest = &prod[ca + "fn choose_subagent(".len()..];
        let cend = crest.find("\nfn ").unwrap_or(crest.len());
        let cbody = &crest[..cend];
        for banned in [w.as_str(), "File::open(", "read_to_string(", "is_dir("] {
            assert!(
                !cbody.contains(banned),
                "`choose_subagent` 里出现了 {banned:?} —— 它开始自己找候选了，\
                 那就不再是「候选由后端决定」"
            );
        }
        // 筛选那一半同理：description 的**精确串等**只许有一处，且在 `choose_subagent` 体内。
        assert_eq!(prod.matches("Some(description)").count(), 1);
        assert_eq!(cbody.matches("Some(description)").count(), 1);
    }

    /// ★★ `KR94D3` 第 ① 刀：**候选顺序不同，仍挑出同一个**（`pick_closest` 不该看顺序）。
    ///
    /// 「挑出同一个」是**全称**命题 ⇒ 3 个候选的 **6 种排列逐个跑**，不挑一个样本了事。
    #[test]
    fn the_order_the_backend_lists_them_in_does_not_change_the_pick() {
        let at = "2026-09-12T10:30:00.000Z";
        let all = [
            listed("/r/agent-a.jsonl", "找它", Some("2026-09-12T10:29:00.000Z")),
            listed("/r/agent-b.jsonl", "找它", Some("2026-09-12T10:40:00.000Z")),
            listed("/r/agent-c.jsonl", "找它", Some("2026-09-12T09:00:00.000Z")),
        ];
        let perms = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        for (i, p) in perms.iter().enumerate() {
            let listing: Vec<String> = p.iter().map(|&k| all[k].clone()).collect();
            assert_eq!(
                choose_subagent(&listing, "找它", at),
                Some(PathBuf::from("/r/agent-a.jsonl")),
                "第 {i} 种排列挑出了别的 —— `pick_closest` 看了顺序"
            );
        }
    }

    /// ★★ `KR94D3` 第 ② 刀：**时间戳缺失那一档，两条路的处置必须一致。**
    ///
    /// 「两条路」在 `K-R94` 之后是同一段代码 ⇒ 真正要钉的是**缺失的两种形状**落到同一档：
    /// 后端拿不到时给 `"timestamp": null`（它**不猜**），而本机那条改前是
    /// 「首行读不出时间戳」—— 在后端出口上表现为**根本没有这个键**。
    /// 两种形状必须同处置，而且**都不报错**（不许一条报错、一条静默取第一个）。
    #[test]
    fn both_shapes_of_a_missing_timestamp_land_in_the_same_tier() {
        let at = "2026-09-12T10:30:00.000Z";
        let null_shape = r#"{"path":"/r/agent-x.jsonl","description":"找它","timestamp":null}"#;
        let absent_shape = r#"{"path":"/r/agent-x.jsonl","description":"找它"}"#;

        // ① 单条：两种形状同结果，而且都是 `Some`（**不报错**、不静默丢掉）。
        let via_null = choose_subagent(&[null_shape.to_string()], "找它", at);
        let via_absent = choose_subagent(&[absent_shape.to_string()], "找它", at);
        assert_eq!(via_null, via_absent, "`null` 与「缺键」被分到了两档");
        assert_eq!(via_null, Some(PathBuf::from("/r/agent-x.jsonl")));

        // ② 多条全缺：处置是「保持后端给的次序、取第一条」——两种形状必须给同一个答案。
        let all_null = vec![
            listed("/r/agent-1.jsonl", "找它", None),
            listed("/r/agent-2.jsonl", "找它", None),
        ];
        let all_absent = vec![
            r#"{"path":"/r/agent-1.jsonl","description":"找它"}"#.to_string(),
            r#"{"path":"/r/agent-2.jsonl","description":"找它"}"#.to_string(),
        ];
        assert_eq!(
            choose_subagent(&all_null, "找它", at),
            choose_subagent(&all_absent, "找它", at)
        );
        assert_eq!(
            choose_subagent(&all_null, "找它", at),
            Some(PathBuf::from("/r/agent-1.jsonl")),
            "全缺时间戳时的处置变了 —— 它是「取后端给的第一条」，不是报错、也不是随机"
        );

        // ③ 部分缺：**有时间戳的一定赢**，而且与缺失是哪种形状无关。
        let shapes = [
            listed("/r/agent-nots.jsonl", "找它", None),
            r#"{"path":"/r/agent-nots.jsonl","description":"找它"}"#.to_string(),
        ];
        for missing in shapes {
            let has = listed(
                "/r/agent-has.jsonl",
                "找它",
                Some("2026-09-12T10:31:00.000Z"),
            );
            let mixed = vec![missing, has];
            assert_eq!(
                choose_subagent(&mixed, "找它", at),
                Some(PathBuf::from("/r/agent-has.jsonl")),
                "缺时间戳的那条排到了有时间戳的前面"
            );
        }

        // ④ `tool_use_timestamp` 自己解析不出来：同样落这一档，**不报错**。
        let l = vec![
            listed("/r/agent-1.jsonl", "找它", Some("2026-09-12T10:00:00.000Z")),
            listed("/r/agent-2.jsonl", "找它", Some("2026-09-12T11:00:00.000Z")),
        ];
        assert_eq!(
            choose_subagent(&l, "找它", "根本不是时间戳"),
            Some(PathBuf::from("/r/agent-1.jsonl"))
        );
    }

    /// ★ **两条路都去问后端，问的是同一对既有子命令**（不新造读口）。
    ///
    /// 〔散文墓碑〕改前这条只钉远端那半（`the_remote_path_actually_asks_the_daemon`）——
    /// 那时本机那条根本不问后端。`K-R94` 之后它钉的是**两条**。
    #[test]
    fn both_paths_ask_the_backend_and_reuse_the_existing_subcommands() {
        let prod = guard_core::production_code(include_str!("subagent.rs"));
        // 子命令各只出现一次 = 两条路共用同一处调用点（不是各写各的）。
        assert_eq!(
            prod.matches("\"--list-subagents\"").count(),
            1,
            "`--list-subagents` 不再是「一处、两条路共用」—— 要么没接上，要么有人复制了一份"
        );
        assert_eq!(
            prod.matches("\"--read-session\"").count(),
            1,
            "内容读口要**复用既有的** `--read-session`，而且只许有一处"
        );
        // 本机那条真的 exec 本机后端（`C1`：本地 = 不走 ssh 的远端）。
        assert!(
            prod.contains("local_query::{run_query, QueryOutcome}"),
            "本机那条没有走 `backend::observe::local_query` —— 它又在自己读盘了"
        );
        // 远端那条仍走既有的 ssh 传输，没有另起炉灶。
        assert!(
            prod.contains("run_list_query(cfg, &args)"),
            "远端那条没走既有的 `run_list_query`"
        );
        // 本机/远端的分流仍只有一处，且只看 origin。
        assert_eq!(
            prod.matches("fn for_origin(").count(),
            1,
            "本机/远端的分流点不再恰好一处"
        );
    }

    /// `agent-<hash>.jsonl` → `<hash>`。两条路共用（路径形状由后端给，解析在本侧）。
    #[test]
    fn agent_id_comes_off_the_file_name() {
        let got = extract_agent_id(&PathBuf::from("/r/agent-9f3.jsonl"));
        assert_eq!(got.as_deref(), Some("9f3"));
        // 不是 `agent-` 开头 ⇒ 没有 id（调用方 `unwrap_or_default()` 成空串）。
        assert_eq!(extract_agent_id(&PathBuf::from("/r/other.jsonl")), None);
    }
}
