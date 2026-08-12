//! issue #16 P1a：远端历史浏览的 monitor 侧。
//!
//! 每条查询走**独立 SSH 连接**一次性 exec `<daemon_path> --list-projects` 等
//! （方案权衡见 issue #16 计划评论：历史浏览用户驱动低频，握手开销可接受，
//! 完全不碰稳定的流式路径；连接建立复用 `ssh_source::connect_session` 全套
//! 指纹校验/鉴权）。
//!
//! 旧 daemon 兼容：不认参数的旧版会照常进流模式、首行发 hello 帧——这里检测
//! `"kind":"hello"` 即返回明确的"daemon 版本过旧"错误（优雅降级，前端 toast）。
//!
//! 只读铁律（INVARIANT § 1）：本模块只读远端；resume/delete 对远端在前端禁用。
//! INVARIANTS § 25：本路径是一次性读取（非 at-least-once 行流），SessionViewer
//! 每次 load 全新实例，无重投幂等义务。

use crate::history::{HistoryProject, HistorySessionEntry};
use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use crate::ssh_source::{self, RemoteConfig};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// 查询超时：列举类命令整体限时（远端扫盘 + 传输）。
const LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 读单会话：不设整体超时（会话可能大、流式合法耗时），但 (a) 每次 read_line 加
/// 单次超时，防"连接活着却永不来数据"卡死；(b) 总字节上限兜底，防无 EOF / 无换行
/// 的巨型损坏文件吃爆内存。
///
/// ⚠〔audit-0805 F06〕**这里原本还有一句「正常会话毫秒级、远小于上限」——那句今天是假的。**
/// 实测本机最大会话 **270,103,105 字节 / 92,967 行**（就是那次审计对话本身），
/// 已经**越过** 256 MiB 这条线 1,667,649 字节；57 MB 以上的会话有 5 个，不是孤例。
/// ⇒ 上限**会被真实数据打到**，所以「打到之后怎么办」不能是静默。
const READ_LINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const MAX_SESSION_BYTES: u64 = 256 * 1024 * 1024;

/// 超限时给用户的话〔audit-0805 F06，定框 **E4/E5**〕。
///
/// # 它此前是**静默**的
///
/// 读法是 `stream.take(MAX_SESSION_BYTES)` + `if n == 0 { break; }` ——
/// 到限之后 `read_line` 返回 0，与**正常 EOF 完全同形** ⇒ 前端拿到一份「看起来完整」的历史，
/// 而后面的内容**无声消失**。同一份数据走 daemon 的 `--fork-session` 那条路会**硬报错**
/// （`common/fs.rs`），走这条路却什么都不说 —— 这正是定框 **E5** 要消灭的
/// 「同一份数据走不同路得到不同答案」。
///
/// 抽成纯函数是为了让它可判据：外面那圈是真 SSH 流，测不了。
fn session_truncated_message(read_bytes: u64, lines_shown: u32) -> String {
    format!(
        "这个会话超过 {MAX_SESSION_BYTES} 字节上限，只读到前 {read_bytes} 字节（{lines_shown} 行）；\
         后面的内容**没有显示**。完整历史仍在远端那个 jsonl 文件里。"
    )
}

pub(crate) fn require_cfg_by_label(label: &str) -> Result<RemoteConfig, String> {
    crate::load_remote_config_by_label(label)
        .ok_or_else(|| format!("远端 '{label}' 未配置或未启用"))
}

/// 旧 daemon 检测：查询命令的输出行不可能含 wire 的 `"kind":"hello"`（查询模式
/// 输出裸 JSON 对象 / 裸 jsonl 行）；出现即说明远端 daemon 不认参数、进了流模式。
fn is_old_daemon_hello(line: &str) -> bool {
    line.contains(r#""kind":"hello""#) || line.contains(r#""kind": "hello""#)
}

const OLD_DAEMON_MSG: &str =
    "远端 daemon 版本过旧（不支持历史查询）——请按 doc/REMOTE-PHASE0-DEPLOY.md 重新构建部署";

/// 跑一条列举类查询，收集全部输出行（带整体超时 + 旧版检测）。
pub(crate) async fn run_list_query(cfg: &RemoteConfig, args: &str) -> Result<Vec<String>, String> {
    let cmd = format!("{} {}", ssh_source::shell_quote(&cfg.daemon_path), args);
    let collect = async {
        let stream = ssh_source::connect_and_exec_cmd(cfg, &cmd).await?;
        let mut reader = BufReader::new(stream);
        let mut lines = Vec::new();
        // ★〔G 审计〕原来是无界 `read_line` —— 与 F10b 修掉的那三处**同一个量**
        // （daemon 出方向单行），只是当时的人群只扫了 `ssh_source.rs`。
        // 外面那层 `LIST_TIMEOUT` 拦不住它：对端 30s 内不吐换行地灌字节，
        // `buf` 就是无界堆分配（daemon 侧同形态实测 RSS 6 MiB → 518 MiB）。
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let text = match ssh_source::read_capped_line(
                &mut reader,
                &mut buf,
                ssh_source::DAEMON_FRAME_LINE_CAP,
            )
            .await
            .map_err(|e| format!("读取远端输出失败: {e}"))?
            {
                ssh_source::CappedLine::Eof => break, // EOF = 命令结束
                // 一次性查询的输出行是 JSON 记录，超上限说明对端不对劲。
                // **拒收+回错**：这条路有调用方接得住错，不像帧读那样只能横向报告。
                ssh_source::CappedLine::TooLong(bytes) => {
                    return Err(format!(
                        "远端输出的单行 {bytes} 字节，超过上限 {} —— 拒收，不拿截断的结果当完整的用",
                        ssh_source::DAEMON_FRAME_LINE_CAP
                    ));
                }
                ssh_source::CappedLine::Line => String::from_utf8_lossy(&buf).into_owned(),
            };
            let line = text.trim();
            if line.is_empty() {
                continue;
            }
            if lines.is_empty() && is_old_daemon_hello(line) {
                return Err(OLD_DAEMON_MSG.to_string());
            }
            lines.push(line.to_string());
        }
        Ok(lines)
    };
    tokio::time::timeout(LIST_TIMEOUT, collect)
        .await
        .map_err(|_| format!("远端查询超时（{}s）: {args}", LIST_TIMEOUT.as_secs()))?
}

/// 远端全文搜索 fan-out（issue #28）：对所有已配置远端各 exec 一次 `<daemon> --search`，
/// 把每行 camelCase `SessionHits` JSON 反序列化、补 `origin = 该台 label`。无远端 → 空；
/// 逐台失败 warn + 跳过（不拖垮其余台）。复用 `run_list_query`（连接/超时/旧 daemon 检测）。
///
/// `scope` 透传原始字符串（"user"/"assistant"/其它=不限）；只对 daemon 认的两值下发。
pub async fn search_remote_all(
    query: &str,
    include_tools: bool,
    scope: Option<&str>,
    after_ms: i64,
    limit: usize,
) -> Vec<crate::search::SessionHits> {
    let cfgs = crate::load_remote_configs();
    if cfgs.is_empty() {
        return Vec::new();
    }
    // 参数对所有台一致（不含 cfg），构建一次。经 shell_quote 防注入；daemon 侧再做 projects/ 白名单校验。
    let mut args = format!("--search {}", ssh_source::shell_quote(query));
    if include_tools {
        args.push_str(" --include-tools");
    }
    if let Some(s) = scope {
        if s == "user" || s == "assistant" {
            args.push_str(" --scope ");
            args.push_str(s);
        }
    }
    if after_ms > 0 {
        args.push_str(&format!(" --after-ms {after_ms}"));
    }
    args.push_str(&format!(" --limit {limit}"));

    // R9：并发 fan-out——各台查询独立、无序要求，join_all 同时查所有台（墙钟从 Σ 降到 max）。
    // 借用 cfg/args 即可（join_all 在当前任务并发 poll，不需 'static/Send）。逐台错误仍隔离。
    let results =
        futures::future::join_all(cfgs.iter().map(|cfg| run_list_query(cfg, &args))).await;
    let mut out = Vec::new();
    for (cfg, res) in cfgs.iter().zip(results) {
        let origin = cfg.origin_label();
        match res {
            Ok(lines) => {
                for line in lines {
                    match serde_json::from_str::<crate::search::SessionHits>(&line) {
                        Ok(mut sh) => {
                            sh.origin = Some(origin.clone());
                            out.push(sh);
                        }
                        Err(e) => {
                            tracing::warn!("远端 [{origin}] --search 行解析失败（跳过）: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("远端 [{origin}] --search 失败（跳过该台）: {e}");
            }
        }
    }
    out
}

/// F88a-remote（#52）：远端用量聚合 fan-out。对所有**非 daemonless** 已配置远端各 exec 一次
/// `<daemon> --usage`（daemon 在远端 CPU 服务端按 requestId 逐字段 MAX 聚合，避免拉整库回本地），
/// 把每行 camelCase `SessionUsageRow` JSON 反序列化、补 `origin = 该台 label`。无远端 → 空；
/// **daemonless 台跳过**（无 daemon 服务端聚合，其 usage 该批不出）；逐台失败 warn+跳过（不拖垮其余台）。
/// 复用 `run_list_query`（连接/超时/旧 daemon hello 检测——旧 daemon 不认 `--usage` → 优雅降级空）。
/// **口径与本地 `usage::accumulate_usage` 一字对齐**（daemon `usage_query.rs` 移植，改口径须同步两处）。
#[tauri::command]
pub async fn aggregate_remote_usage_all() -> Vec<crate::usage::SessionUsageRow> {
    let cfgs: Vec<RemoteConfig> = crate::load_remote_configs()
        .into_iter()
        .filter(|c| !c.daemonless) // daemonless 主机无 daemon → 无 --usage 服务端聚合
        .collect();
    if cfgs.is_empty() {
        return Vec::new();
    }
    // 并发 fan-out（各台独立、无序），墙钟从 Σ 降到 max；逐台错误隔离。
    let results =
        futures::future::join_all(cfgs.iter().map(|cfg| run_list_query(cfg, "--usage"))).await;
    let mut out = Vec::new();
    for (cfg, res) in cfgs.iter().zip(results) {
        let origin = cfg.origin_label();
        match res {
            Ok(lines) => {
                for line in lines {
                    match serde_json::from_str::<crate::usage::SessionUsageRow>(&line) {
                        Ok(mut row) => {
                            row.origin = Some(origin.clone());
                            out.push(row);
                        }
                        Err(e) => {
                            tracing::warn!("远端 [{origin}] --usage 行解析失败（跳过）: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("远端 [{origin}] --usage 失败（跳过该台）: {e}");
            }
        }
    }
    out
}

/// F76（#46）：远端来源列表结果 = 项目 + **失败台清单**。
///
/// 后端 fan-out 语义是「任一台成功即 `Ok`，失败台 warn+跳过」——前端单看项目列表无从区分
/// 「某台失败缺项」与「某台真的无项目」。带上 `failed_hosts` 让前端判断「部分失败」：部分失败
/// 时**不冻结 TTL 缓存**（下次 open 重试失败台），避免瞬断台的项目在缓存里消失整个 TTL 窗口。
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct RemoteProjectsResult {
    pub projects: Vec<HistoryProject>,
    /// 本次 fan-out 中查询失败（已跳过）的台的 origin label。空 = 全部台成功。
    pub failed_hosts: Vec<String>,
}

/// 远端项目列表（多机 #30：fan-out 所有已配置远端）。无远端 → 空列表（前端无感合并）；
/// 单台查询失败 → warn + 跳过该台（不拖垮其余台）。各 project 带 `origin = 该台 label`。
#[tauri::command]
pub async fn list_remote_history_projects() -> Result<RemoteProjectsResult, String> {
    let cfgs = crate::load_remote_configs();
    if cfgs.is_empty() {
        return Ok(RemoteProjectsResult {
            projects: Vec::new(),
            failed_hosts: Vec::new(),
        });
    }
    let mut projects = Vec::new();
    let mut any_ok = false;
    let mut last_err = String::new();
    let mut failed_hosts: Vec<String> = Vec::new();
    // R9：并发 fan-out 所有台（各台独立、无序要求），墙钟从 Σ(各台) 降到 max(各台)。逐台错误仍隔离。
    let results = futures::future::join_all(
        cfgs.iter()
            .map(|cfg| run_list_query(cfg, "--list-projects")),
    )
    .await;
    for (cfg, res) in cfgs.iter().zip(results) {
        let lines = match res {
            Ok(l) => {
                any_ok = true;
                l
            }
            Err(e) => {
                // 逐台失败不拖垮整体：该台 warn + 跳过，其余台照常返回。记进 failed_hosts
                // 让前端识别「部分失败」（F76：部分失败不冻结缓存、下次 open 重试该台）。
                tracing::warn!(
                    "远端 [{}] --list-projects 失败（跳过该台）: {e}",
                    cfg.origin_label()
                );
                failed_hosts.push(cfg.origin_label());
                last_err = e;
                continue;
            }
        };
        for line in lines {
            let v: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("remote --list-projects 行解析失败（跳过）: {e}: {line}");
                    continue;
                }
            };
            let dir_name = v["dirName"].as_str().unwrap_or_default().to_string();
            if dir_name.is_empty() {
                continue;
            }
            let project_path = v["projectPath"].as_str().unwrap_or_default().to_string();
            // 对齐本地口径：projectName = cwd 最后一段；提取不到 cwd 时回退编码目录名
            let project_name = if project_path.is_empty() {
                dir_name.clone()
            } else {
                project_path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&dir_name)
                    .to_string()
            };
            projects.push(HistoryProject {
                project_path,
                project_name,
                // 远端的"懒加载 key"= 远端编码目录名（前端原样传回 stream_remote_history_sessions）
                project_dir: dir_name,
                session_count: v["sessionCount"].as_u64().unwrap_or(0) as u32,
                // P1a：远端不合并本地元数据计数（列表级开销不值得），条目级照常合并
                starred_count: 0,
                hidden_count: 0,
                last_activity: v["lastActivityMs"].as_i64().unwrap_or(0),
                // 活跃远端会话已有 [host] live Tab，历史组不重复标 live
                has_live: false,
                origin: Some(cfg.origin_label()),
            });
        }
    }
    // 配了远端但**全部**台查询都失败 → 返回 Err（前端可 toast），避免与"无远端配置"的
    // 空列表（cfgs.is_empty 早返）混淆，让用户能区分"没配"和"配了但连不上"。
    if !any_ok {
        return Err(format!(
            "所有远端历史查询失败（{} 台），最后一个错误: {last_err}",
            cfgs.len()
        ));
    }
    tracing::info!(
        "list_remote_history_projects: {} projects from {} host(s), {} failed",
        projects.len(),
        cfgs.len(),
        failed_hosts.len()
    );
    Ok(RemoteProjectsResult {
        projects,
        failed_hosts,
    })
}

/// 远端某项目的历史会话列表（流式 Channel，对齐本地 stream_history_sessions_in_project）。
/// `project_dir` = 远端编码目录名（list_remote_history_projects 给出的 projectDir）。
#[tauri::command]
pub async fn stream_remote_history_sessions(
    project_dir: String,
    origin: String,
    on_entry: tauri::ipc::Channel<HistorySessionEntry>,
) -> Result<u32, String> {
    let cfg = require_cfg_by_label(&origin)?;
    // 防穿越：目录名不允许含分隔符（daemon 侧同样校验，双层防御）
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("非法项目目录名: {project_dir}"));
    }
    let args = format!("--list-sessions {}", ssh_source::shell_quote(&project_dir));
    let lines = run_list_query(&cfg, &args).await?;
    // 条目级元数据（star/rename/hide）按 session_id 存本地，远端会话同样适用
    let metadata = crate::history::load_metadata().unwrap_or_default();
    let mut total = 0u32;
    for line in lines {
        let v: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("remote --list-sessions 行解析失败（跳过）: {e}");
                continue;
            }
        };
        let session_id = v["sessionId"].as_str().unwrap_or_default().to_string();
        if session_id.is_empty() {
            continue;
        }
        let cwd = v["cwd"].as_str().unwrap_or_default().to_string();
        let project_name = cwd
            .rsplit(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&project_dir)
            .to_string();
        let meta = metadata
            .entries
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let entry = HistorySessionEntry {
            session_id,
            project_path: cwd,
            project_name,
            ai_title: v["aiTitle"].as_str().map(String::from),
            // Batch11-F32：p1h daemon 附 isBg；旧 daemon 缺字段 → false 安全降级
            is_bg: v["isBg"].as_bool().unwrap_or(false),
            first_user_excerpt: v["firstUserExcerpt"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            started_at: v["startedAtMs"].as_i64().unwrap_or(0),
            updated_at: v["updatedAtMs"].as_i64().unwrap_or(0),
            jsonl_path: v["jsonlPath"].as_str().unwrap_or_default().to_string(),
            is_live: false,
            message_count_approx: v["messageCountApprox"].as_u64().unwrap_or(0) as u32,
            starred: meta.starred,
            custom_title: meta.custom_title,
            hidden: meta.hidden,
            // P1a：daemon 不提取 fork 关系，远端会话在 fork 树上呈平铺
            forked_from_session_id: None,
            forked_from_message_uuid: None,
            origin: Some(cfg.origin_label()),
        };
        if on_entry.send(entry).is_err() {
            tracing::info!("stream_remote_history_sessions: 前端取消");
            return Ok(total);
        }
        total += 1;
    }
    tracing::info!("stream_remote_history_sessions({project_dir}): {total} sessions");
    Ok(total)
}

/// 流式读取远端单个会话（对齐本地 stream_read_session_jsonl 的 chunk 口径：
/// 每 100 条一发，payload 带 origin=Some(host)，SessionViewer 零改动复用）。
#[tauri::command]
pub async fn stream_read_remote_session(
    jsonl_path: String,
    origin: String,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    let cfg = require_cfg_by_label(&origin)?;
    // 深度防御（与 stream_remote_history_sessions 的 project_dir 校验对称）：jsonl_path 来自
    // 前端，monitor 侧先做廉价校验（拒 `..` + 强制 .jsonl 后缀）。真正的越权读由 daemon 侧
    // canonicalize + projects/ 前缀 + symlink 逃逸校验兜底，这里补齐不对称的防御缺口。
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法会话路径: {jsonl_path}"));
    }
    let started = std::time::Instant::now();
    // 与本地 history.rs 的 file_stem 口径一致：剥**一个** ".jsonl" 后缀（strip_suffix
    // 是字面后缀，不是 trim_end_matches 的字符集语义）。
    let file_name = jsonl_path.rsplit(['/', '\\']).next().unwrap_or("");
    let session_id = file_name
        .strip_suffix(".jsonl")
        .unwrap_or(file_name)
        .to_string();
    let args = format!("--read-session {}", ssh_source::shell_quote(&jsonl_path));
    let cmd = format!("{} {}", ssh_source::shell_quote(&cfg.daemon_path), args);
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    // F06：`+ 1` 是为了**能分辨「到限」与「正好读完」** —— 只 take(MAX) 的话，
    // 到限时 read_line 返回 0，与正常 EOF 完全同形，于是静默截断。
    let mut reader = BufReader::new(stream.take(MAX_SESSION_BYTES + 1));
    let mut read_bytes: u64 = 0;
    let mut buf = String::new();
    let mut line_buf: Vec<u8> = Vec::new();
    let mut cwd_seen: Option<String> = None;
    let mut chunk: Vec<crate::bridge::JsonlLinePayload> = Vec::with_capacity(CHUNK_SIZE);
    let mut total = 0u32;
    let mut next_seq: u64 = 0;
    let mut first_line = true;
    loop {
        // ★〔G 审计〕原来是无界 `read_line`。下面那条 `MAX_SESSION_BYTES` 是**总量**且
        // **读完再判** —— 一条 10 GiB 的行会在 `read_line` 返回**之前**就把内存吃光，
        // 那条总量检查根本轮不到跑。这正是 daemon 侧 `inbound.rs` 头注逐字警告的
        // 「上限必须在**读的时候**生效，不能读完再判」，而当时那次实测是 RSS 6 MiB → 518 MiB。
        // ⇒ 补一层**单行**上限（与 daemon 出方向单行同量），总量那条保持不动。
        let n = match tokio::time::timeout(
            READ_LINE_TIMEOUT,
            ssh_source::read_capped_line(
                &mut reader,
                &mut line_buf,
                ssh_source::DAEMON_FRAME_LINE_CAP,
            ),
        )
        .await
        .map_err(|_| "读取远端会话超时（单次读取卡住）".to_string())?
        .map_err(|e| format!("读取远端会话失败: {e}"))?
        {
            ssh_source::CappedLine::Eof => break,
            // 超单行上限与超总量**同一档处置**（截断+说清）：都是「这份会话读不完整了，
            // 而且明说为什么」。措辞分开，因为用户要采取的动作不同。
            ssh_source::CappedLine::TooLong(bytes) => {
                return Err(format!(
                    "远端会话里有一行 {bytes} 字节，超过单行上限 {} —— 已停止读取。\
                     这不是会话太大（那会报另一句），是**单条记录**异常巨大，多半该直接看源文件。",
                    ssh_source::DAEMON_FRAME_LINE_CAP
                ));
            }
            ssh_source::CappedLine::Line => {
                buf.clear();
                buf.push_str(&String::from_utf8_lossy(&line_buf));
                buf.len()
            }
        };
        read_bytes += n as u64;
        if read_bytes > MAX_SESSION_BYTES {
            // F06：**不许静默截断**。同一份数据走 daemon 的 `--fork-session` 会硬报错，
            // 走这条路却假装读完了 —— 定框 E5 要的是「同一份数据走不同路得到同一个答案」。
            return Err(session_truncated_message(read_bytes, total));
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        if first_line {
            first_line = false;
            if is_old_daemon_hello(trimmed) {
                return Err(OLD_DAEMON_MSG.to_string());
            }
        }
        // 与本地 stream_read_session_jsonl 同口径：parse + displayable 过滤 + per-file seq
        let rec = match parse_line(trimmed) {
            Ok(Some(r)) if r.is_displayable() => r,
            _ => continue,
        };
        if let JsonlRecord::User { cwd, .. } = &rec {
            if cwd_seen.is_none() {
                cwd_seen = cwd.clone();
            }
        }
        let seq = next_seq;
        next_seq += 1;
        chunk.push(crate::bridge::JsonlLinePayload {
            session_id: session_id.clone(),
            cwd: cwd_seen.clone(),
            path: jsonl_path.clone(),
            seq,
            origin: Some(cfg.origin_label()),
            message: rec,
        });
        total += 1;
        if chunk.len() >= CHUNK_SIZE {
            let full = std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK_SIZE));
            if on_chunk.send(full).is_err() {
                tracing::info!("stream_read_remote_session({session_id}): 前端取消于 {total} 条");
                return Ok(total);
            }
        }
    }
    if !chunk.is_empty() {
        let _ = on_chunk.send(chunk);
    }
    tracing::info!(
        "stream_read_remote_session({session_id}): {total} records in {}ms",
        started.elapsed().as_millis()
    );
    Ok(total)
}

/// 删除一个远端历史会话的 jsonl（issue 未拆，F11）。**只读铁律豁免（SS-G）**：用户
/// 显式删除 + 前端二次确认 + `sftp::remove_remote_file` 双重路径守卫。删除后清本地元数据。
#[tauri::command]
pub async fn delete_remote_history_session(
    origin: String,
    jsonl_path: String,
) -> Result<(), String> {
    let cfg = require_cfg_by_label(&origin)?;
    crate::sftp::remove_remote_file(&cfg, &jsonl_path).await?;
    // 清本地元数据（注解按 sid = jsonl 文件名 stem）。
    if let Some(sid) = jsonl_stem(&jsonl_path) {
        crate::history::remove_metadata_entry(&sid);
    }
    Ok(())
}

/// 远端 POSIX 路径取文件名 stem（去目录、去 `.jsonl`）。非 jsonl / 无文件名 → None。
fn jsonl_stem(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    name.strip_suffix(".jsonl").map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_stem_basics() {
        assert_eq!(
            jsonl_stem("/home/pi/.claude/projects/p/abc-123.jsonl").as_deref(),
            Some("abc-123")
        );
        assert_eq!(jsonl_stem("abc.jsonl").as_deref(), Some("abc"));
        assert_eq!(jsonl_stem("/x/y/note.txt"), None);
        assert_eq!(jsonl_stem(""), None);
    }

    #[test]
    fn old_daemon_hello_detected() {
        assert!(is_old_daemon_hello(
            r#"{"kind":"hello","v":1,"build_id":"phase0-proto","host_arch":"aarch64","claude_dir":"/home/pi/.claude"}"#
        ));
        // 查询模式的正常输出不含 kind
        assert!(!is_old_daemon_hello(
            r#"{"dirName":"-home-pi-proj","projectPath":"/home/pi/proj","sessionCount":3,"lastActivityMs":1}"#
        ));
        // jsonl 正文里聊到 hello 不该误判（必须是 kind 字段形态）
        assert!(!is_old_daemon_hello(
            r#"{"type":"user","message":{"content":"say hello"}}"#
        ));
    }

    #[test]
    fn shell_quote_via_ssh_source() {
        assert_eq!(
            crate::ssh_source::shell_quote("/a/b c.jsonl"),
            "'/a/b c.jsonl'"
        );
        assert_eq!(crate::ssh_source::shell_quote("a'b"), r"'a'\''b'");
    }
}

#[cfg(test)]
mod f06_tests {
    use super::*;

    /// ★ **超限必须说话，而且要说清「少了什么」**〔audit-0805 F06，定框 E4/E5〕。
    ///
    /// 此前是 `take(MAX)` + `if n == 0 { break; }` —— 到限与正常 EOF **完全同形**，
    /// 前端拿到一份「看起来完整」的历史而后面的内容无声消失。
    /// 而同一份数据走 daemon 的 `--fork-session` 会**硬报错**（`common/fs.rs`）：
    /// **同一份数据走两条路得到两个答案**，正是 E5 要消灭的。
    #[test]
    fn the_truncation_message_says_what_is_missing_and_where_it_still_is() {
        let m = session_truncated_message(MAX_SESSION_BYTES + 1, 92_967);
        assert!(m.contains("上限"), "要说清是撞了上限：{m}");
        assert!(
            m.contains("92967") || m.contains("92_967"),
            "★ 要报出**已显示多少行** —— 用户得知道自己看到的是哪一截：{m}"
        );
        assert!(
            m.contains("没有显示"),
            "★ 要明说后面的内容没显示 —— 不说这句就等于还是在静默：{m}"
        );
        assert!(
            m.contains("仍在远端"),
            "★ 要告诉用户完整历史还在（这条与丢帧不同：数据没丢，是没读完）：{m}"
        );
    }

    /// ★ **上限检查还接在路径上，不只是「登记过」**〔audit-0805 §5 1g，08-06 补〕。
    ///
    /// # 先量后写：1g 那句话对，但它给的理由只挡住一半
    ///
    /// §5 1g 逐字写着「把检测拆成 `if false` 全仓没有任何判据会红」。08-06 真做了这次变异
    /// （把下面那个条件换成 `if false {`）：monitor 全量 **passed=989 failed=0** ——
    /// 那句话在**行为层**成立。
    ///
    /// 但它给的**理由**是「住在吃真 SSH 流的 async 函数里，红线内造不出 >256 MiB 的远端流」。
    /// 这次变异与流有多大**毫无关系** —— 那个理由挡住的只是「撞到上限时运行起来真的会 Err」，
    /// 挡不住「判断被整个摘掉」。**阻塞四问 ①「挡住的是整件还是一部分」又一次命中。**
    ///
    /// 顺带说清另一件容易误读的事：`byte_cap_registry` 里**登记了**这处上限，
    /// 但它管的是「上限有没有被登记 + 语义有没有声明」，**不是「检查会不会触发」** ——
    /// 「有个登记表覆盖着」不等于「这条路上有人守着」。
    ///
    /// # 钉什么、不钉什么
    ///
    /// 钉**源码形态的三段链**，每一段单独被摘掉，静默截断都会回来：
    ///
    /// | 段 | 摘掉它会怎样 |
    /// |---|---|
    /// | `take(…+ 1)` 里的 **`+ 1`** | 到限时 `read_line` 返回 0，与正常 EOF **完全同形** ⇒ 无声截断 |
    /// | 那个 `read_bytes >` 条件 | 判断没了，读到 cap 就当读完了 |
    /// | 那一支的 `return Err(…)` | 换成 `break` 就是「读完了」，前端拿到一份看起来完整的历史 |
    ///
    /// **不钉**「撞到上限时运行起来真的会 Err」—— 那要把读循环从这个吃真 SSH 流的
    /// async fn 里抽出来（连 `tauri::ipc::Channel` 那个出口一起抽象）。本轮不做，
    /// §5 1g **保留**，但范围缩小到行为层那一半。
    ///
    /// # 对照组是自带的，不是另写一条
    ///
    /// 本判据的 needle 在**未剥测试段**的源码里有两处：生产一处 + 本测试的字面量一处。
    /// 下面第一条断言的就是「剥完之后它变少了」—— 于是「有人把 `production_source` 拿掉」
    /// 会当场红，而不是让本条静默地读到自己（F23 那一族，本区已犯过三次）。
    #[test]
    fn the_cap_check_is_still_wired_not_just_declared() {
        let raw = include_str!("remote_history.rs");
        let prod = guard_core::production_source(raw);

        const COND: &str = "if read_bytes > MAX_SESSION_BYTES {";
        assert!(
            raw.matches(COND).count() > prod.matches(COND).count(),
            "★ 对照组：剥掉测试段之后这个 needle 应当变少（本测试自己的字面量被剥走了）。\n\
             没变少 ⇒ `production_source` 没在起作用，下面三条就是在**读自己**、恒绿。"
        );

        // ① `+ 1`：它是「到限」与「正好读完」唯一的区分手段。
        guard_core::pin_line(
            &prod,
            "let mut reader = BufReader::new(stream.take(MAX_SESSION_BYTES + 1));",
        )
        .expect(
            "★ `take(MAX_SESSION_BYTES + 1)` 这一行不在了（或写法变了）。\n\
             只 take(MAX) 的话，到限时 read_line 返回 0，与正常 EOF **完全同形** ——\n\
             下面两条即使都在，也再没有任何东西能分辨「读完了」和「读到上限」。",
        );

        // ② 条件本身：这一条正是 08-06 那次 `if false` 变异摘掉的东西。
        let at = guard_core::pin_line(&prod, COND).expect(
            "★ 上限判断不在生产段里了。08-06 实测：把它换成 `if false {`，\n\
             monitor 全量 989 条**一条都不会红** —— 本条就是为那个洞补的。",
        );

        // ③ 那一支必须**报错**，不能是 break/continue：后者等于「读完了」。
        let body = prod
            .lines()
            .skip(at + 1)
            .find(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("//")
            })
            .unwrap_or("");
        assert!(
            body.trim()
                .starts_with("return Err(session_truncated_message("),
            "★ 撞上限那一支的第一句是 `{}`，不是 `return Err(session_truncated_message(…))`。\n\
             换成 break/continue 就是「读完了」：前端拿到一份**看起来完整**的历史，\n\
             而同一份数据走 daemon 的 `--fork-session` 会硬报错 —— 同一份数据两条路两个答案，\n\
             正是定框 E5 要消灭的那种不一致。",
            body.trim()
        );
    }
}
