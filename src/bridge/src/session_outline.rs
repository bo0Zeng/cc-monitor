//! 〔`设计/10 §2.2b ⑥` · SE1〕**monitor 侧的大纲数据源**：问后端要「你说过的话」清单。
//!
//! | 命令 | 跑什么 | 给谁 |
//! |---|---|---|
//! | [`list_user_inputs`] | `--list-user-inputs --from <offset> <p>` | 大纲（实时 tab 与历史查看器**同一条**）：`IPC-PROTOCOL.md §10.4` |
//!
//! 走 [`crate::subagent::Backend`]（本机 exec 本机后端 / 远端 ssh exec 同一个二进制）——
//! 「这条查询谁去跑」全仓只有那一处分流（与骨架索引 `session_skeleton` 同一个形）。
//!
//! # 判定不在这里
//!
//! 「什么算一条用户输入」只住后端 `observe/user_inputs.rs`。本侧**不解释**记录，只核头尾、
//! 把行搬成 [`UserInputEntry`]。前端 TS 那份判定（`user-input-index.ts`）已删 —— 两份判定
//! 就是 `设计/10 §2.2b ⑥` 逐字禁掉的「各写一遍」。
//!
//! # 🔴 诚实降级
//!
//! 老后端不认这条子命令（现打：stdout 0 字节、退出 2）、本机后端不在、输出被截断 ——
//! 一律回 `available: false` ＋ 原因（**不是错误**）：大纲灰掉、原因挂在开关的提示上。
//! **绝不**把别的输出当清单解析。
//!
//! # 买不到
//!
//! - 远端整体 30s 超时（`run_list_query` 的 `LIST_TIMEOUT`）：弱网上超大会话的全量清单可能撞上
//!   （现打本机 release：97 MB 的会话一趟约 1 s）。增量（`--from`）只读新写的那一截，撞不上。
//! - 远端要等 `BUILD_ID` bump 后判 stale 重装才有这条子命令。

use crate::subagent::Backend;

/// 一条可点的清单项（后端那一行的形状，键名一字不差）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct UserInputEntry {
    pub uuid: String,
    /// 列表上显示的摘要（后端已折叠、已截断）。
    pub excerpt: String,
    /// 记录的 `timestamp`；没有 ⇒ 空串。
    pub timestamp: String,
}

/// 〔SE1 回修〕**要不到清单的种类** —— 前端据它决定「还要不要再要」，**只 match 它，不解析 `reason` 的文字**。
///
/// | 种类 | 是什么 | 前端怎么办 |
/// |---|---|---|
/// | `oldBackend` | 对面回的第一行不是 `user_inputs` 头（老后端 0 字节退出 / 回了别的形状），或查询自己就带出「老后端」（本机退出 2 ＋ `unknown argument` · 远端首行 hello · 长连接不认） | **结构性**：再要一定还是这样 ⇒ 不再要，灰掉说原因 |
/// | `truncated` | 有头，但没尾 / 尾行条数对不上 / 中间一行坏了，或查询自己带出「截断」（远端单行超上限被拒收） | **瞬时**：下一次触发再要 |
/// | `transport` | 查询本身失败的其余情形（起不了本机后端进程、ssh 连不上、超时、后端退出码非 0 且不是 `unknown argument` —— 含「越过 EOF」） | **瞬时**：下一次触发再要 |
///
/// 〔C2〕先前这里登记着两处**分错档**（很老的远端后端回 hello · 本机后端过旧退出 2，都落进 `transport`），
/// 病根是「`subagent::Backend::query` 只回一句话」。那个签名改成带种类（`subagent::QueryFailure`）之后，
/// 两处都在失败发生的那一层当场定成 `oldBackend`；本侧只做「查询的种类 ⇒ 大纲的种类」那一步映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum OutlineFailure {
    OldBackend,
    Truncated,
    Transport,
}

/// 清单的回包。`available == false` 时 `entries` 为空、`failure` 是种类、`reason` 是给人看的原因（**不是错误**）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct UserInputsResult {
    pub available: bool,
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 要不到时的种类（`available == true` 时缺席）。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<OutlineFailure>,
    /// 这份清单从哪个字节起（= 请求的 `from_offset`）。
    #[cfg_attr(test, ts(type = "number"))]
    pub from: u64,
    /// 最后一个完整行的末字节 ＝ **下一次增量该带的 `from_offset`**。
    #[cfg_attr(test, ts(type = "number"))]
    pub end: u64,
    /// 按文件顺序（= 对话顺序）。
    pub entries: Vec<UserInputEntry>,
}

/// 本侧认得出的「拿不到清单」的几种样子。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OutlineUnavailable {
    /// 首行不是 `user_inputs` 头 —— 对面是不认这条子命令的老后端（它 0 字节退出）或回了别的东西。
    OldBackend,
    /// 有头没尾、尾行条数对不上、或中间一行坏了 —— 输出被截断，**不许当全量用**。
    Truncated { got: usize },
}

impl OutlineUnavailable {
    fn kind(&self) -> OutlineFailure {
        match self {
            Self::OldBackend => OutlineFailure::OldBackend,
            Self::Truncated { .. } => OutlineFailure::Truncated,
        }
    }

    fn reason(&self) -> String {
        match self {
            Self::OldBackend => "这台机器上的后端版本旧，还列不出大纲（重装后端之后就有）".into(),
            Self::Truncated { got } => {
                format!("大纲清单传到一半断了（只收到 {got} 条），这次先不显示")
            }
        }
    }
}

/// 核头尾、剥出条目。**纯函数**，两条 transport 的输出（逐行、已 trim、已剔空行）都走它。
pub(crate) fn parse_user_inputs_output(
    lines: &[String],
) -> Result<(u64, u64, Vec<UserInputEntry>), OutlineUnavailable> {
    let parse = |l: &String| serde_json::from_str::<serde_json::Value>(l).ok();
    let kind_is =
        |v: &serde_json::Value, k: &str| v.get("kind").and_then(|x| x.as_str()) == Some(k);
    let Some(head) = lines
        .first()
        .and_then(parse)
        .filter(|h| kind_is(h, "user_inputs"))
    else {
        return Err(OutlineUnavailable::OldBackend);
    };
    let from = head.get("from").and_then(|v| v.as_u64()).unwrap_or(0);
    let got = lines.len().saturating_sub(2);
    let tail = lines
        .last()
        .filter(|_| lines.len() >= 2)
        .and_then(parse)
        .filter(|t| kind_is(t, "user_inputs_end"))
        .ok_or(OutlineUnavailable::Truncated { got })?;
    if tail.get("count").and_then(|v| v.as_u64()) != Some(got as u64) {
        return Err(OutlineUnavailable::Truncated { got });
    }
    let end = tail.get("end").and_then(|v| v.as_u64()).unwrap_or(from);
    let mut entries = Vec::with_capacity(got);
    for l in &lines[1..lines.len() - 1] {
        match serde_json::from_str::<UserInputEntry>(l) {
            Ok(e) => entries.push(e),
            Err(_) => return Err(OutlineUnavailable::Truncated { got }),
        }
    }
    Ok((from, end, entries))
}

/// 清单那条的 argv。🔴 **选项写在位置参数前面**（与 `session_skeleton::index_argv` 同一条纪律，有判据钉着）。
///
/// ⚠ 如实记：对**这一条**来说顺序改变不了老后端的行为 —— 它是新子命令，老后端连子命令都不认，
/// 选项在前在后都是 stdout 0 字节、退出 2（现打：主线 p2o 与本机 p2j 两份各一趟）。
/// 钉它是为了全仓只有一种写法：下一个给它加选项的人不用再想「放哪边」。
pub(crate) fn user_inputs_argv(jsonl_path: &str, from_offset: u64) -> Vec<String> {
    vec![
        "--list-user-inputs".into(),
        "--from".into(),
        from_offset.to_string(),
        jsonl_path.into(),
    ]
}

/// 路径的廉价预检（与 `session_skeleton` / `load_subagent` 同一条纪律）：
/// 真正的越权读由后端 `fence_under_projects` 兜底。
fn precheck(jsonl_path: &str) -> Result<(), String> {
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法会话路径: {jsonl_path}"));
    }
    Ok(())
}

/// 一趟查询的结果 → 回包。**纯函数**：分类（[`OutlineFailure`]）只有这一个住址。
///
/// 查询本身失败（`Err`）按**查询自己带出来的种类**分档（[`crate::subagent::QueryFailure`]，逐档映射、不看文字）；
/// 查询成功但输出不对，按 [`parse_user_inputs_output`] 的判定分档。
pub(crate) fn outline_result(
    queried: Result<Vec<String>, crate::subagent::QueryError>,
    from_offset: u64,
) -> UserInputsResult {
    let unavailable = |failure: OutlineFailure, reason: String| UserInputsResult {
        available: false,
        reason: Some(reason),
        failure: Some(failure),
        from: from_offset,
        end: from_offset,
        entries: Vec::new(),
    };
    // 「后端不在 / 查询失败（含越过 EOF：文件被截断重写）」：这一趟没有清单，原因原样带给前端
    // （增量失败时前端从 0 重要一次）。
    let lines = match queried {
        Ok(l) => l,
        Err(e) => return unavailable(outline_kind(e.kind), e.message),
    };
    match parse_user_inputs_output(&lines) {
        Ok((from, end, entries)) => UserInputsResult {
            available: true,
            reason: None,
            failure: None,
            from,
            end,
            entries,
        },
        Err(u) => unavailable(u.kind(), u.reason()),
    }
}

/// 查询的种类 ⇒ 大纲的种类。**穷尽 `match`、零通配**：查询那边多一档，这里当场编不过。
pub(crate) fn outline_kind(k: crate::subagent::QueryFailure) -> OutlineFailure {
    use crate::subagent::QueryFailure;
    match k {
        QueryFailure::OldBackend => OutlineFailure::OldBackend,
        QueryFailure::Truncated => OutlineFailure::Truncated,
        QueryFailure::Transport => OutlineFailure::Transport,
    }
}

/// **「你说过的话」清单**：从字节 `from_offset` 起（冷启动 0 / 增量传上次的 `end`）。
#[tauri::command]
pub async fn list_user_inputs(
    origin: crate::origin::Origin,
    jsonl_path: String,
    from_offset: u64,
) -> Result<UserInputsResult, String> {
    let route = origin.route("list_user_inputs")?;
    precheck(&jsonl_path)?;
    let backend = Backend::for_origin(route)?;
    let argv = user_inputs_argv(&jsonl_path, from_offset);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let res = outline_result(backend.query(&argv).await, from_offset);
    if let Some(f) = res.failure {
        tracing::info!(
            "list_user_inputs({jsonl_path}): {} 没有大纲（{f:?}）：{}",
            backend.whose(),
            res.reason.as_deref().unwrap_or("")
        );
    }
    Ok(res)
}

#[cfg(test)]
#[path = "../../../tests/bridge/session_outline_tests.rs"]
mod tests;
