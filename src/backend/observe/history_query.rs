//! issue #16 P1a：一次性历史查询模式。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec 本二进制并带参数：
//!
//! - `--list-projects`                → 列举 `<claude_dir>/projects/` 下各项目
//! - `--list-sessions <project_dir>`  → 列举某项目目录下的历史会话（带元数据）
//! - `--read-session <jsonl_path>`    → 原样透传该 jsonl 文件内容（monitor 侧解析）
//! - `--read-session-from-offset <jsonl_path> <offset>` → 从字节 `offset`（0-based）透传
//!   [offset, EOF]，= aterm `tail -c +(offset+1)`（offset 续拉/重连恢复）
//!
//! 输出协议：`--list-*` 每行一个 JSON 对象（**不是** wire::Frame——查询模式与流式
//! 协议互不混用，旧后端不认参数会照常进流模式发 hello，monitor 以"首行是
//! hello 帧"识别旧版并优雅降级）；`--read-session` 输出原始文件字节。
//! 错误：stderr 写原因 + 退出码 2。成功退出码 0。
//!
//! 安全：所有路径参数严格限制在 `<claude_dir>/projects/` 之内（canonicalize 后
//! 前缀校验，防 `../` 穿越）；project_dir 参数不允许含路径分隔符。
//! 只读铁律（cc-monitor 不写远端）在此同样成立：本模块只 read_dir / read。

// U2/U3：这两个原来在本文件里各有一份逐字相同的副本。去向**不同**：
// `projects_root` 跨 observe/control 两层 ⇒ `common/`；`mtime_ms` 两个调用点同属 observe
// ⇒ U3 按 `common/` 自己的「≥2 层」门槛搬回 `observe/`。
use crate::agents::claudecode::paths::projects_root;
use crate::observe::fence::Fence;
use crate::observe::fs::mtime_ms;
use copy_core::copy_text;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 查询模式入口。返回进程退出码。
pub fn run(agent_home: &Path, args: &[String]) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("--list-projects") => {
            return match list_projects_to(agent_home, &mut std::io::stdout().lock()) {
                Ok(()) => 0,
                Err((Some(code), said)) => coded_failure(code, &said),
                Err((None, e)) => query_failed(&e),
            }
        }
        Some("--list-sessions") => match args.get(1) {
            Some(dir) => list_sessions(agent_home, dir),
            None => Err("--list-sessions requires <project_dir> argument".into()),
        },
        Some("--read-session-tail") => match (args.get(1), args.get(2)) {
            (Some(p), Some(n)) => match n.parse::<usize>() {
                Ok(n) => read_session_tail(agent_home, p, n),
                Err(_) => Err("--read-session-tail <jsonl_path> <N>: N must be a number".into()),
            },
            _ => Err("--read-session-tail requires <jsonl_path> <N> arguments".into()),
        },
        Some("--read-session") => match args.get(1) {
            Some(p) => read_session(agent_home, p),
            None => Err("--read-session requires <jsonl_path> argument".into()),
        },
        Some("--read-session-from-offset") => match parse_from_offset_args(&args[1..]) {
            Ok((opts, pos)) => match (pos.first(), pos.get(1)) {
                (Some(p), Some(o)) => match o.parse::<u64>() {
                    Ok(o) if opts.index => session_index(agent_home, p, o, opts.until),
                    Ok(o) => read_session_from_offset(agent_home, p, o, opts.until),
                    Err(_) => Err(
                        "--read-session-from-offset <jsonl_path> <offset>: offset must be a number"
                            .into(),
                    ),
                },
                _ => Err(
                    "--read-session-from-offset requires <jsonl_path> <offset> arguments".into(),
                ),
            },
            Err(e) => Err(e),
        },
        // 〔`设计/10 §2.2b ⑥` · SE1〕大纲的数据源：「你说过的话」清单（判定住 `observe::user_inputs`）。
        Some("--list-user-inputs") => match parse_user_inputs_args(&args[1..]) {
            Ok((from, p)) => list_user_inputs(agent_home, p, from),
            Err(e) => Err(e),
        },
        // 〔SE2 · `设计/10 §6 步 6`〕会话内查找（口径与 `--search` 同一份，内核住 `observe::search_query`）。
        Some("--find-in-session") => match parse_find_args(&args[1..]) {
            Ok(a) => find_in_session(agent_home, &a),
            Err(e) => Err(e),
        },
        Some(other) => Err(format!("unknown argument: {other}")),
        None => Err("no query argument".into()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => query_failed(&e),
    }
}

/// `--list-projects`：每个项目目录一行 JSON：
/// `{"dirName","projectPath","sessionCount","lastActivityMs","sessionIds"}`
/// projectPath 从该项目**最新** jsonl 的头部记录提取 cwd（对齐本地口径：真实工作
/// 目录，而非编码过的目录名）；提取不到则空字符串，monitor 侧回退显示 dirName。
///
/// # `sessionIds`（`K-R83` 09-12）：这一行**带得出下游要算的那三个数**
///
/// monitor 侧的项目行还要 `starredCount` / `hiddenCount` / `hasLive` 三个数，
/// 而这三个数的真相源在 monitor 那侧（本机 metadata / `SessionMap`）**全部按会话 sid 索引**
/// —— 缺的从来不是「谁来数」，是「这个项目下有哪几个 sid」。
/// ⇒ 本行把那份清单带上，下游一次就算得出，**不用每个项目再来一次 `--list-sessions`**
/// （那是 N 次进程 spawn，而项目列表是用户常开的界面 —— 失效方向逐字记在
/// monitor 的 `local_read_surface_registry.rs` 那条退役条件里）。
///
/// **代价如实记**：`sessionIds` 与 `sessionCount` 同源同一趟 `read_dir`，
/// **零额外 I/O**；涨的只有输出字节（每会话 ~38 B）。monitor 侧单行上限是 64 MiB
/// （`ssh_source::BACKEND_FRAME_LINE_CAP`），要撞上它得一个项目下约 170 万个会话。
///
/// ⚠ **它与 `sessionCount` 恒等长，这是契约的一部分** —— 下游据此判「空清单」是
/// 「真的没有会话」还是「这一行坏了」（`sessionCount > 0` 而清单空 ⇒ 后者，不许当成 0）。
/// 〔WF2〕老 CLI 那一面照旧**出声**（rc=2）：它是远端 / 一次性问者的契约，零行会被读成「这家没有会话」
/// （`agents/fake` 那条 S6-Z3 钉着）；但那一形带上结构化的码 [`NO_RECORD_TREE`]（`{code, message}` 信封），
/// 问的那台后端认码 ⇒ 画「这台还没有会话记录」而不是「没加载上」。其余失败无码（`Err((None, 原因))`）。
pub(crate) fn list_projects_to(
    agent_home: &Path,
    out: &mut dyn Write,
) -> Result<(), (Option<&'static str>, String)> {
    if list_projects_into(agent_home, out).map_err(|e| (None, e))? {
        return Ok(());
    }
    Err((
        Some(NO_RECORD_TREE),
        copy_text(
            "beHistory.records.none",
            &[("path", &projects_root(agent_home).display().to_string())],
        ),
    ))
}

/// 〔WF2 · WIN3 读数 H〕`--list-projects` 在「记录树根不在」时信封里的码（生产方住这里，认码的是 `history_join` 远端那一支）。
pub(crate) const NO_RECORD_TREE: &str = "no_record_tree";

/// 带码的那一行：CLI 错误信封（`{code, message}`，与 `cli_control::emit_err` 同一对键；那边读信封的是 `remote_ask::settle_pulled`）。
/// 不调 `emit_err`：观测层不往控制层伸手（`layering_guard`）。
fn coded_failure(code: &str, said: &str) -> i32 {
    eprintln!("{}", serde_json::json!({ "code": code, "message": said }));
    2
}

/// 一次性查询失败的那一行（无码的旧形）。
fn query_failed(e: &str) -> i32 {
    eprintln!("cc-monitor-backend query error: {e}");
    2
}

/// `--list-projects` 的本体，出口是参数 ——〔`C1` · 2026-09-24〕帧面那条（`history-projects`）
/// 与 CLI 这条**跑的是同一个函数**，只是 `out` 一个是 stdout、一个是内存里那份应答。
///
/// 〔WF2 · WIN3 读数 H〕回「记录树根在不在」：不在 ⇒ `Ok(false)`、一行不写（这台还没起过会话 —— 判定只在这一处）；
/// 怎么说由两个宿主各自定：帧面当零个项目（界面照空态「还没有会话记录」画，别的机器照常）· CLI 照旧出声。
pub(crate) fn list_projects_into(agent_home: &Path, out: &mut dyn Write) -> Result<bool, String> {
    let root = projects_root(agent_home);
    let entries = match std::fs::read_dir(&root) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(unreadable_dir(&root, &e)),
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let Some(line) = project_row(&dir, dir_name) else {
            continue; // 空目录（全删过/只剩本机后端）不展示
        };
        writeln!(out, "{line}").map_err(|e| format!("stdout write failed: {e}"))?;
    }
    Ok(true)
}

/// 〔WF2 · WIN3 读数 H〕会话记录目录读不了 ⇒ 给人看的那一句（按错误的**种类**说；系统原话只进日志 —— `设计/91` 不露实现词）。
fn unreadable_dir(dir: &Path, e: &std::io::Error) -> String {
    tracing::warn!("history_query: 读不了 {}：{e}", dir.display());
    let path = dir.display().to_string();
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        copy_text("beHistory.dir.denied", &[("path", &path)])
    } else {
        copy_text("beHistory.dir.unreadable", &[("path", &path)])
    }
}

/// 一个项目目录 → `--list-projects` 的那一行；目录下没有会话记录 ⇒ `None`（不展示）。
///
/// # 为什么它是**一个函数**而不是 `list_projects` 里的一段
///
/// `list_projects` 的出口是 `stdout`，红线内测不了；而 `K-R83` 的三条判据要判的是
/// **这一行带了什么**，不是「stdout 上出现了什么」。同样的分法在本文件里已有先例：
/// `analyze_session` 也是把「算出那一行」与「把它印出去」分开的。
fn project_row(dir: &Path, dir_name: String) -> Option<serde_json::Value> {
    let mut session_count = 0u32;
    let mut last_activity_ms = 0i64;
    let mut newest_jsonl: Option<(i64, PathBuf)> = None;
    // `K-R83`：sid 的取法与 `--list-sessions` 那条逐字同源（`analyze_session` 也是
    // `file_stem`）—— 两条路给同一个会话的 id 必须是同一个字符串，否则下游按 sid
    // 去查 metadata 会**查不着而看起来像「没有星标」**，又是一次「不知道」装成 0。
    let mut session_ids: Vec<String> = Vec::new();
    if let Ok(files) = std::fs::read_dir(dir) {
        for f in files.flatten() {
            let p = f.path();
            if !p.is_file() || !crate::agents::claudecode::records::is_session_file(&p) {
                continue;
            }
            // ⚠ 计数与 sid **共用同一个守卫**：不是「先数了再看取不取得到 sid」。
            // 分开写的话，取不到 stem 的那一格会让 `sessionCount` 与清单长度错开，
            // 而下游正是拿这两者对拍来分辨「真的没有」与「这一行坏了」。
            let Some(sid) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            session_count += 1;
            session_ids.push(sid);
            let mtime = mtime_ms(&p);
            if mtime > last_activity_ms {
                last_activity_ms = mtime;
            }
            if newest_jsonl.as_ref().is_none_or(|(m, _)| mtime > *m) {
                newest_jsonl = Some((mtime, p));
            }
        }
    }
    if session_count == 0 {
        return None; // 空目录（全删过/只剩本机后端）不展示
    }
    let project_path = newest_jsonl
        .and_then(|(_, p)| extract_cwd_from_head(&p))
        .unwrap_or_default();
    // 排序**不是**为了好看：`read_dir` 的顺序是文件系统给的，两趟未必一样，
    // 而下游要拿这份清单做对拍与缓存 key —— 不稳定的顺序会让「同一份数据」看起来变了。
    session_ids.sort_unstable();
    Some(serde_json::json!({
            "dirName": dir_name,
            "projectPath": project_path,
            "sessionCount": session_count,
            "lastActivityMs": last_activity_ms,
            "sessionIds": session_ids,
    }))
}

/// `--list-sessions <project_dir>`：该项目每个 jsonl 一行 JSON：
/// `{"sessionId","jsonlPath","startedAtMs","updatedAtMs","messageCountApprox",
///   "firstUserExcerpt","aiTitle","cwd"}`
/// 元数据在远端 CPU 上扫整个文件提取（对齐本地 analyze 口径的精简版）。
fn list_sessions(agent_home: &Path, project_dir: &str) -> Result<(), String> {
    list_sessions_into(agent_home, project_dir, &mut std::io::stdout().lock())
}

/// `--list-sessions` 的本体，出口是参数（同 [`list_projects_into`]：帧面与 CLI 同一个函数）。
pub(crate) fn list_sessions_into(
    agent_home: &Path,
    project_dir: &str,
    out: &mut dyn Write,
) -> Result<(), String> {
    // project_dir 是目录名而非路径：拒绝任何分隔符 / 上跳
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("invalid project dir name: {project_dir}"));
    }
    // 与 read_session 对齐（也兑现本文件头部"canonicalize 后前缀校验"的承诺）：名字
    // 合法但 projects/ 下若有指向外部的 symlink 目录，read_dir 会跟随逃逸出 projects/
    // ——canonicalize 解析 symlink 后做前缀校验挡住。
    // 〔audit-0805 08-06〕**改调共享围栏**（E3）：此前这里是一份内联副本，
    // 注释写着「与 `read_session` 对齐」—— 靠手工对齐的两份迟早会漂。
    let dir = fence_under_projects(agent_home, Path::new(project_dir))?;
    let entries = std::fs::read_dir(&dir).map_err(|e| unreadable_dir(&dir, &e))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() || !crate::agents::claudecode::records::is_session_file(&p) {
            continue;
        }
        let meta = analyze_session(&p);
        writeln!(out, "{meta}").map_err(|e| format!("stdout write failed: {e}"))?;
    }
    Ok(())
}

// **围栏住 `observe/fence.rs`**〔TL3 · 审计 F 🔴-6 · `设计/15 §4.2` / `§5.3 C5`〕：这里原来是具名围栏
// `fence_under_projects` 的本体（「全文件唯一的一处 `canonicalize` + 前缀校验」，audit-0805 08-06 定框 E3 从
// `list_sessions` 的内联副本收成一份）。E3 只收到了**本文件**，`search_query` 里还有一份内联的（头注逐字「复刻 history_query」）
// ⇒ 判定本体（解开根 · 解开目标 · 前缀比）搬去 observe 内部唯一的家 `observe/fence.rs::Fence`；这里只剩「以 `projects/` 为根」
//   那一行（根是哪一个属 Claude 的目录布局，留在认得它的这一侧），三条按路径读的路照旧调它，报错原话逐字不变。
// 〔合并 LOC1b〕LOC1b 在这里把本体提成了「根是参数」的一形（为各家合成历史面给的记录根）—— 那一形就是
// `observe/fence.rs::Fence::at(根)?.admit(候选)`（报错取根目录名，与 LOC1b 那一版逐字同形），下面 [`validate_session_path_among`] 改调它。

/// `<agent_home>/projects/` 这道围栏放行一个候选路径（本体在 [`Fence`]；`candidate` 相对按根拼、绝对直用）。
fn fence_under_projects(agent_home: &Path, candidate: &Path) -> Result<std::path::PathBuf, String> {
    Fence::at(&projects_root(agent_home))?.admit(candidate)
}

/// P7c-1：列一个父会话的 **subagent 候选**。
///
/// # ★ 它**完全不做匹配**，这是全部要点
///
/// 最容易的写法是把 monitor 的 `load_subagent` 整套搬过来（推目录 → 按 description 精确匹配
/// → 按时间戳挑最近 → 读）。**那会长出第二套语义** —— 定框 `C1` 逐字排除，
/// 而本轮已经在那个 `or` 上数出**四份**实现。
///
/// ⇒ 这里只做后端独有的那件事：**列候选**。筛选与挑选留在 monitor，
/// 与本机那条路**共用同一份** `pick_closest`。
/// 由 `the_backend_never_matches_or_ranks_subagents` 钉住（生产段零 `description ==`、零时间戳比较）。
///
/// 围栏**复用既有的** `fence_under_projects` —— subagent 目录本来就在
/// `<claude_dir>/projects/<slug>/<sid>/subagents/` 里（实测），不用放宽任何东西。
///
/// 出：每行一个 `{"path","description","timestamp"}`（description/timestamp 拿不到就给 null，
/// **不猜**）。错：exit 2 + stderr `{code,message}`，与 `--resolve` 同形。
pub fn list_subagents(agent_home: &Path, args: &[String]) -> i32 {
    let Some(parent) = args.get(1) else {
        eprintln!(
            "{}",
            serde_json::json!({"code":"invalid_args","message":"用法: --list-subagents <父会话 jsonl 路径>"})
        );
        return 2;
    };
    match list_subagents_into(agent_home, parent, &mut std::io::stdout().lock()) {
        Ok(()) => 0,
        Err((code, message)) => {
            eprintln!("{}", serde_json::json!({"code":code,"message":message}));
            2
        }
    }
}

/// 〔MOD · 原 monitor `subagent·rs` 的 `choose_subagent` · `pick_closest`〔散文墓碑〕〕从 [`list_subagents_into`] 的候选行里挑**一个**：
/// `description` 精确串等筛，再按首行时间戳与 `tool_use_timestamp` 差距最小挑。**纯函数，不碰文件系统**。
///
/// 挑的规则只有这一份（`C1`）：原先它住 monitor、后端只列不挑；「找」与「挑」一起进了后端之后，界面只问一次。
/// 缺时间戳那一档：拿不到（`None`）或 `tool_use_timestamp` 自己解析不出 ⇒ 排序键取 `i64::MAX`；稳定排序 ⇒
/// 全缺时保持列出来的次序、取第一条，部分缺时有时间戳的排在前面。**不报错**。
pub(crate) fn pick_subagent(
    listing: &[&str],
    description: &str,
    tool_use_timestamp: &str,
) -> Option<std::path::PathBuf> {
    use crate::observe::search_query::parse_iso8601_ms;
    let mut metas: Vec<(std::path::PathBuf, Option<String>)> = Vec::new();
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
        // 时间戳拿不到就是 `None` —— `null` 与**根本没这个键**落到同一档。
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .map(str::to_string);
        metas.push((std::path::PathBuf::from(path), ts));
    }
    if metas.len() <= 1 {
        return metas.pop().map(|(p, _)| p);
    }
    let target = parse_iso8601_ms(tool_use_timestamp);
    metas.sort_by_key(
        |(_, ts)| match (target, ts.as_deref().and_then(parse_iso8601_ms)) {
            (Some(t), Some(f)) => (f - t).abs(),
            _ => i64::MAX,
        },
    );
    metas.into_iter().next().map(|(p, _)| p)
}

/// [`list_subagents`] 的本体，出口是参数（帧面 `history-subagents` 与 CLI 同一个函数）。
/// 错误是 `(code, message)`，CLI 那层把它原样印成 stderr 那一行 JSON（字节与改前相同）。
pub(crate) fn list_subagents_into(
    agent_home: &Path,
    parent: &str,
    out: &mut dyn Write,
) -> Result<(), (&'static str, String)> {
    let parent_path =
        fence_under_projects(agent_home, Path::new(parent)).map_err(|e| ("path_refused", e))?;
    // 目录推法与 monitor 侧逐字同形：`<父 jsonl 去后缀>/subagents`。
    let Some(dir) = parent_path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|stem| parent_path.parent().map(|d| d.join(stem).join("subagents")))
    else {
        return Err((
            "bad_parent",
            crate::common::contract::malformed(
                "cannot derive the subagents directory from the parent path",
            ),
        ));
    };
    // 目录不在 = 这个会话没有 subagent，**不是错**：回空、exit 0。
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in rd.flatten() {
        let meta_path = entry.path();
        let Some(name) = meta_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".meta.json") else {
            continue;
        };
        let jsonl =
            meta_path.with_file_name(crate::agents::claudecode::records::session_file_name(stem));
        if !jsonl.is_file() {
            continue;
        }
        let description = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("description")?.as_str().map(str::to_string));
        // 只读**首行** —— subagent jsonl 可能很大，为一个时间戳整读是白费。
        let timestamp = std::fs::File::open(&jsonl)
            .ok()
            .and_then(|f| {
                use std::io::BufRead;
                let mut line = String::new();
                std::io::BufReader::new(f).read_line(&mut line).ok()?;
                Some(line)
            })
            .and_then(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
            .and_then(|v| v.get("timestamp")?.as_str().map(str::to_string));
        writeln!(
            out,
            "{}",
            serde_json::json!({
                "path": jsonl.to_string_lossy(),
                "description": description,
                "timestamp": timestamp,
            })
        )
        .map_err(|e| ("write_failed", format!("write failed: {e}")))?;
    }
    Ok(())
}

/// 按路径读一份会话之前的围栏：Claude 的 `projects/` ∪ 注册表里各家合成历史面给的记录根（〔LOC1b · 4D〕）。
fn validate_session_path(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    validate_session_path_among(agent_home, &crate::agents::history_roots(), jsonl_path)
}

/// [`validate_session_path`] 的本体，「另外认哪几个根」是参数（判据喂临时目录，不去动进程环境）。
///
/// 〔LOC1b · 4D〕历史浏览器本机远端都会列出 Codex 会话（C4d 起后端合成），而本机冷读也改走后端之后，
/// 这道围栏只认 `projects/` ⇒ 列得出、打不开。根由适配层给（`HistoryFace.root`），这里不写死路径。
/// 另外那几个根**只收绝对路径**（相对路径的意思只在 `projects/` 下有定义）；
/// 都不在 ⇒ 回 `projects/` 那一句拒绝（它是今天所有调用方认得的那一句）。
fn validate_session_path_among(
    agent_home: &Path,
    extra_roots: &[std::path::PathBuf],
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    let candidate = Path::new(jsonl_path);
    let target = match fence_under_projects(agent_home, candidate) {
        Ok(t) => t,
        Err(refused) if candidate.is_absolute() => extra_roots
            .iter()
            .find_map(|r| Fence::at(r).and_then(|f| f.admit(candidate)).ok())
            .ok_or(refused)?,
        Err(refused) => return Err(refused),
    };
    if !crate::agents::claudecode::records::is_session_file(&target) {
        return Err("refusing to read non-jsonl file".into());
    }
    Ok(target)
}

/// `--read-session <jsonl_path>`：路径校验后原样透传文件内容（〔TL3〕这两行原先挂在围栏头上，随围栏搬家挪回它说的那个函数）。
/// 透传而非逐行解析：monitor 侧本就有完整的 parse_line 管线，backend 不重复造。
fn read_session(agent_home: &Path, jsonl_path: &str) -> Result<(), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    std::io::copy(&mut f, &mut out).map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// backend-02（Phase 1 offset 续拉）：`--read-session-from-offset <path> <offset>`——
/// seek 到字节 `offset`（0-based）后原样透传 [offset, EOF]，**语义逐字节 = aterm
/// `tail -c +(offset+1)`**（`TailTransport.kt:33` + `SkeletonScan.windowContentCommand`）。
/// `offset` = 客户端从 Line 帧 `byte_offset` 持久化的续点（重连/断线后带上）。
/// 截断/重写（远端 size < offset）**不在此判**——同 aterm 由客户端另经 size 查检测后
/// 决策 reset（`offsetByPath`），此处 seek 过 EOF → 读空 → 透传空，安全无副作用。
/// 透传而非逐行：monitor 侧 parse_line 管线已全，backend 不重复造（同 `read_session`）。
///
/// 〔`设计/10` 骨架 · 子步 1〕加了两个**选项**（不是新子命令 —— 见 [`FromOffsetOpts`] 的头注）：
/// `--until <end>` 把透传收成半开区间 `[offset, end)`；`--index` 不透传字节，改出
/// **骨架索引**（[`session_index`]）。两个都不带时字节一个不变（老调用方零回归）。
fn read_session_from_offset(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
) -> Result<(), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    stream_from_offset(&mut f, offset, until, &mut out)
        .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 生产 seek+copy 内核（`read_session_from_offset` 与单测共用）——seek 到字节 `offset`（0-based）
/// 透传 [offset, EOF] = `tail -c +(offset+1)`；offset > 文件长 → seek 过 EOF、copy 空（不 panic）。
/// **审计 quality/correctness**：抽出泛型 `W` 让单测直接对 `Vec<u8>` 驱动**发货的 seek 路径本身**、
/// 断言真实续拉字节，闭「只测 `slice_from_offset` 助手、生产 seek 路径无字节断言」的 dup-drift。
///
/// `until = Some(end)`：只透传 `[offset, end)`（`end ≤ offset` ⇒ 空）。骨架按需物化
/// 「这一段正文」走它 —— 两端都取自索引里的行边界，所以切出来的恰好是整行。
/// ⚠ **它不替调用方对齐行边界**：传一个行中间的数，切出来的就是半行（与 `offset` 同一口径）。
fn stream_from_offset<W: std::io::Write>(
    f: &mut std::fs::File,
    offset: u64,
    until: Option<u64>,
    out: &mut W,
) -> std::io::Result<u64> {
    use std::io::{Read, Seek};
    f.seek(std::io::SeekFrom::Start(offset))?;
    match until {
        None => std::io::copy(f, out),
        Some(end) => std::io::copy(&mut f.take(end.saturating_sub(offset)), out),
    }
}

/// `--read-session-from-offset` 的两个**选项**：`--index`（出骨架索引，不出字节）·
/// `--until <end>`（右端收口）。
///
/// # 🔴 为什么是选项，不是一条新子命令 `--session-index`
///
/// 加子命令 ⇒ `build_id_guard` 的指纹变 ⇒ 必须 bump `BUILD_ID`（远端才会判 stale 重装）。
/// 本轮（10 路并行）**明令不许 bump**。选项不进指纹 —— 这正是那条护栏头注自陈的盲区
/// 「子命令集没变但行为变了它不管」。⇒ **这一刀在已部署的老后端上是休眠的**，
/// 直到下一次有人 bump；**老后端上的行为已设计成可认出来**：
/// 老后端不认 `--index`/`--until`（它只读 `args[1..=2]`，多余参数不看）⇒ 照旧透传字节
/// ⇒ 首行不是 `{"kind":"session_index",…}` ⇒ monitor 据此判「对面不会出索引」并诚实降级
/// （`--until` 同理：多拿到的尾巴由 monitor 自己按 `end` 截掉，结果仍然对，只是多传了字节）。
///
/// # 语义上它也**就是**「从偏移读」
///
/// 索引从 `offset` 起算 ⇒ 冷启动传 0 拿全量；续传传上次的 `end` 拿增量（`设计/10 §5 B`：
/// 「续传令牌只能用字节偏移」）。与透传字节是同一个读、两种出法。
///
/// ⚠ 未知的 `--选项` 与多余的位置参数都**报错**（不静默忽略）：老 monitor 从不带，新 monitor 只带这两个；
/// 多出来的一定是写错了，而静默忽略会让它拿到一份形状不对的输出还以为成功。
///
/// # 🔴 选项可以写在位置参数**前面** —— 而 monitor 就该这么写（让老后端**快速失败**）
///
/// 老后端只看 `args[1]`/`args[2]`：把 `--index` 写在路径**后面**，老后端会把**整份会话**透传回来
/// （弱网上几十 MB，只为了让 monitor 看一眼首行认出「它不会」）。写在**前面**，老后端拿路径当 offset
/// 解析 ⇒ `offset must be a number` ⇒ **零字节、退出 2**。新后端两种位置都认。
/// （现打：基线 `3662e17` 的 release 后端对一份 50 955 695 字节的会话 —— 选项在后 stdout 50 955 695 字节 /
/// 退出 0；选项在前 stdout 0 字节 / 退出 2。）
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FromOffsetOpts {
    pub(crate) index: bool,
    pub(crate) until: Option<u64>,
}

pub(crate) fn parse_from_offset_args(
    rest: &[String],
) -> Result<(FromOffsetOpts, Vec<&String>), String> {
    let mut opts = FromOffsetOpts::default();
    let mut pos: Vec<&String> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--index" => opts.index = true,
            "--until" => {
                let v = it
                    .next()
                    .ok_or("--until requires <end> (byte offset)")?
                    .parse::<u64>()
                    .map_err(|_| "--until <end>: end must be a number")?;
                opts.until = Some(v);
            }
            other if other.starts_with("--") => {
                return Err(format!(
                    "--read-session-from-offset: unknown option {other}"
                ))
            }
            _ => pos.push(a),
        }
    }
    if pos.len() > 2 {
        return Err(format!(
            "--read-session-from-offset takes <jsonl_path> <offset>, got {} positional arguments",
            pos.len()
        ));
    }
    Ok((opts, pos))
}

/// `--list-user-inputs [--from <offset>] <jsonl_path>` 的 argv：一个位置参数 ＋ 一个可选的 `--from`。
///
/// 选项在位置参数前后都认；客户端写在前面（与骨架索引那一形同一条纪律；〔C4b〕monitor 不再经 argv 发它，
/// 界面经帧命令 `history-user-inputs` 问）。未知的 `--选项`、多余的位置参数都**报错**，
/// 不静默忽略 —— 静默忽略会让调用方拿到一份形状不对的输出还以为成功了。
pub(crate) fn parse_user_inputs_args(rest: &[String]) -> Result<(u64, &String), String> {
    let mut from: u64 = 0;
    let mut pos: Vec<&String> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--from" => {
                from = it
                    .next()
                    .ok_or("--from requires <offset> (byte offset)")?
                    .parse::<u64>()
                    .map_err(|_| "--from <offset>: offset must be a number")?;
            }
            other if other.starts_with("--") => {
                return Err(format!("--list-user-inputs: unknown option {other}"))
            }
            _ => pos.push(a),
        }
    }
    match pos.as_slice() {
        [p] => Ok((from, *p)),
        _ => Err(format!(
            "--list-user-inputs takes exactly one <jsonl_path>, got {} positional arguments",
            pos.len()
        )),
    }
}

/// `--list-user-inputs`：从字节 `from` 起（冷启动 0 / 增量传上次的 `end`）出「你说过的话」清单。
/// 形状与口径见 [`crate::observe::user_inputs`] 的头注。
///
/// `from` 超过文件长度 ⇒ **报错**（文件被截断或重写过 —— 调用方手上的 `end` 已经不指向这份文件），
/// 不回一份空清单假装「没有新的」。路径守卫与 `--read-session` 同一套。
fn list_user_inputs(agent_home: &Path, jsonl_path: &str, from: u64) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    list_user_inputs_into(agent_home, jsonl_path, from, &mut out)?;
    out.flush().map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// `--list-user-inputs` 的本体，出口是参数 ——〔SR1a · 2026-09-24〕帧面那条（`history-user-inputs`）
/// 与 CLI 这条**跑的是同一个函数**，只是 `out` 一个是 stdout、一个是内存里那份应答（同 `list_projects_into`）。
pub(crate) fn list_user_inputs_into(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
    mut out: &mut dyn Write,
) -> Result<(), String> {
    crate::observe::user_inputs::write_user_inputs(
        open_user_inputs_at(agent_home, jsonl_path, from)?,
        from,
        &mut out,
    )
    .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 〔C4b · 第四波 4B〕大纲清单的打开口（围栏 ＋「起点越过文件尾 ⇒ 报错」＋ 定位）—— CLI 臂与帧面臂共用。
pub(crate) fn open_user_inputs_at(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
) -> Result<std::io::BufReader<std::fs::File>, String> {
    use std::io::{Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let len = f.metadata().map_err(|e| format!("stat failed: {e}"))?.len();
    if from > len {
        return Err(format!(
            "--from {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    f.seek(SeekFrom::Start(from))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// 〔STC〕`history-facts` 的续点：从 `from` 接着读之前先核两件事，任一不成立 ⇒ 报错（调用方从 0 重要一份）：
/// ① `from` 不越过文件尾（越过 = 截断 / 重写）；② `from > 0` 时文件第 `from-1` 字节是 `\n`
/// （续点恒是某个完整行的末字节 —— 不在行边界上 = 被重写过，接着读会从半行起、把后面的事实算歪）。
/// 挡不住的一形：重写成更长、而旧续点恰好也落在新内容的行边界上（与大纲 `设计/10 §7` 第 4 条同一个口子）。
pub(crate) fn open_facts_at(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
) -> Result<std::io::BufReader<std::fs::File>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let len = f.metadata().map_err(|e| format!("stat failed: {e}"))?.len();
    if from > len {
        return Err(format!(
            "resume point {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    if from > 0 {
        f.seek(SeekFrom::Start(from - 1))
            .map_err(|e| format!("seek failed: {e}"))?;
        let mut last = [0u8; 1];
        f.read_exact(&mut last)
            .map_err(|e| format!("read failed: {e}"))?;
        if last[0] != b'\n' {
            return Err(format!(
                "resume point {from} is not at a line boundary: file was rewritten"
            ));
        }
    }
    f.seek(SeekFrom::Start(from))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// `--find-in-session` 的 argv（〔SE2〕）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FindArgs<'a> {
    pub(crate) path: &'a String,
    pub(crate) query: &'a String,
    pub(crate) include_tools: bool,
    pub(crate) limit: usize,
}

/// `--find-in-session [--include-tools] [--limit <n>] --query <q> <jsonl_path>`。
///
/// 🔴 **查询串是 `--query` 的值，不是位置参数**：用户要找的就可能是 `--force` 这种以 `--` 起头的词，
/// 作为位置参数它会被当成一个写错的选项。选项的值原样取下一个 token，不看它长什么样。
/// 选项在位置参数前后都认；客户端写在前面（〔C4b〕monitor 不再经 argv 发它，界面经帧命令 `history-find` 问）。
/// 未知的 `--选项`、缺 `--query`、位置参数不是恰好一个 ⇒ **报错**（不静默忽略 —— `--search` 那种
/// 「未知选项容错忽略」正是本命令不做成它的一个选项的理由之一，见 `IPC-PROTOCOL.md §10.5`）。
/// `--limit` 超出封顶按封顶算（`FIND_MAX_LIMIT`）；0 ⇒ 只数不列。
pub(crate) fn parse_find_args(rest: &[String]) -> Result<FindArgs<'_>, String> {
    use crate::observe::search_query::{FIND_DEFAULT_LIMIT, FIND_MAX_LIMIT};
    let mut query: Option<&String> = None;
    let mut include_tools = false;
    let mut limit = FIND_DEFAULT_LIMIT;
    let mut pos: Vec<&String> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--query" => query = Some(it.next().ok_or("--query requires <text>")?),
            "--include-tools" => include_tools = true,
            "--limit" => {
                limit = it
                    .next()
                    .ok_or("--limit requires <n>")?
                    .parse::<usize>()
                    .map_err(|_| "--limit <n>: n must be a number")?
                    .min(FIND_MAX_LIMIT);
            }
            other if other.starts_with("--") => {
                return Err(format!("--find-in-session: unknown option {other}"))
            }
            _ => pos.push(a),
        }
    }
    let query = query.ok_or("--find-in-session requires --query <text>")?;
    match pos.as_slice() {
        [p] => Ok(FindArgs {
            path: p,
            query,
            include_tools,
            limit,
        }),
        _ => Err(format!(
            "--find-in-session takes exactly one <jsonl_path>, got {} positional arguments",
            pos.len()
        )),
    }
}

/// `--find-in-session`：在**一份**会话里找（路径守卫与 `--read-session` 同一套）。形状见
/// [`crate::observe::search_query::write_session_find`] 的头注。
fn find_in_session(agent_home: &Path, a: &FindArgs<'_>) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    find_in_session_into(
        agent_home,
        a.path,
        a.query,
        a.include_tools,
        a.limit,
        &mut out,
    )?;
    out.flush().map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 会话内查找的本体，出口是参数 ——〔SR1a × SE2〕帧面那条（`history-find`）与 CLI 这条
/// （`--find-in-session`）**跑的是同一个函数**（同 `list_projects_into`）。
pub(crate) fn find_in_session_into(
    agent_home: &Path,
    jsonl_path: &str,
    query: &str,
    include_tools: bool,
    limit: usize,
    mut out: &mut dyn Write,
) -> Result<(), String> {
    crate::observe::search_query::write_session_find(
        open_session_at(agent_home, jsonl_path, 0)?,
        query,
        include_tools,
        limit,
        &mut out,
    )
    .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// `--read-session-from-offset <path> <offset> --index [--until <end>]`：**骨架索引**。
///
/// 出三段（逐行 JSON）：
/// 1. 头 `{"kind":"session_index","v":1,"from":<offset>}` —— **首行就能认出**「对面会出索引」；
/// 2. 每个**可计行**一行 [`IndexRow`]（口径 = [`line_counts`]，与 watcher/monitor 的 seq 空间一字一致：
///    第 k 行就是 seq `base + k`，`base` = `offset` 之前的可计行数，由调用方持有）；
/// 3. 尾 `{"kind":"session_index_end","count":N,"end":E}` —— `end` = 最后一个**完整行**的末字节
///    （torn 残尾不计，F14 口径）＝ 下一次续传该带的 `offset`。**没有尾行 ⇒ 输出被截断了**，
///    调用方不许把前面那些行当成全量。
///
/// 不含正文（`设计/10 §5 A` 已定：骨架不带正文）。每行约 100 字节 × 条数。
fn session_index(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    session_index_into(agent_home, jsonl_path, offset, until, &mut out)?;
    out.flush().map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 骨架索引的本体，出口是参数 ——〔SR1a · 2026-09-24〕帧面那条（`history-index`）
/// 与 CLI 这条（`--read-session-from-offset --index`）**跑的是同一个函数**（同 `list_projects_into`）。
pub(crate) fn session_index_into(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
    mut out: &mut dyn Write,
) -> Result<(), String> {
    write_session_index(
        open_session_at(agent_home, jsonl_path, offset)?,
        offset,
        until,
        &mut out,
    )
    .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 〔GAP1〕过了围栏的会话文件路径（`history-find` 先拿它问常驻索引）。
pub(crate) fn session_path_at(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    validate_session_path(agent_home, jsonl_path)
}

/// 〔C4b · 第四波 4B〕过围栏、打开、定位到 `offset` —— 骨架索引与会话内查找的 CLI 臂和帧面臂共用这一处
/// （帧面那一臂出成品，不经 `out`，见 `read_face.rs`）。
pub(crate) fn open_session_at(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
) -> Result<std::io::BufReader<std::fs::File>, String> {
    use std::io::{Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// [`session_index`] 的内核：读 `r`（已定位在 `from`）逐行出索引。**纯 I/O 泛型**，单测直接喂字节。
///
/// `until`：只收**起点** `< until` 的行（起点在界内的那一行整行收，不劈半行）。
/// 返回写出的行数（不含头尾）。
pub(crate) fn write_session_index<R: std::io::BufRead, W: std::io::Write>(
    r: R,
    from: u64,
    until: Option<u64>,
    out: &mut W,
) -> std::io::Result<u64> {
    writeln!(
        out,
        "{{\"kind\":\"session_index\",\"v\":1,\"from\":{from}}}"
    )?;
    let (count, end) = scan_session_index(r, from, until, |row| {
        serde_json::to_writer(&mut *out, row)?;
        out.write_all(b"\n")
    })?;
    writeln!(
        out,
        "{{\"kind\":\"session_index_end\",\"count\":{count},\"end\":{end}}}"
    )?;
    Ok(count)
}

/// 〔C4b · 第四波 4B〕[`write_session_index`] 的中段：每个可计行一条 [`IndexRow`] 交给 `on_row`，回 `(count, end)`。
/// CLI 那一臂（写头尾三段）与帧面那一臂（`read_face.rs` 的 `history-index`，装成成品 `{from, end, rows}`）
/// 跑的是**同一个**扫描；「这一行占不占 seq」仍只住 [`line_counts`]。
pub(crate) fn scan_session_index<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    until: Option<u64>,
    mut on_row: impl FnMut(&IndexRow) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let mut pos = from;
    let mut end = from;
    let mut count: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        if until.is_some_and(|u| pos >= u) {
            break;
        }
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break; // EOF / torn 残尾不计（F14 口径）
        }
        let start = pos;
        pos += read as u64;
        end = pos;
        let body = &buf[..buf.len() - 1];
        if !line_counts(body) {
            continue; // 空行不占 seq（判定只有 `line_counts` 一个住址）
        }
        on_row(&index_row(body, start, read as u64))?;
        count += 1;
    }
    Ok((count, end))
}

/// 骨架索引的一行：**位置 ＋ 身份 ＋ 宽度无关料**（`设计/10 §1` 第一格 · `§2.5b 路 D`）。
///
/// 键名刻意短（每条记录一行，大会话上万行）；**缺省即零 / 假**，零值不序列化。
///
/// # 宽度无关料是什么、**不是**什么
///
/// 是「这条记录渲染出来**会有多少东西**」的计数 —— 与列宽无关，所以用户拉窗口不会让它失效；
/// 前端拿它乘以当前列宽下的字宽/行高做**第一级粗估**（`height-estimate.ts::estimateFromFacts`）。
/// **不是**高度：高度依赖列宽，后端不知道列宽（`§2.5b` 那条「决定性的理由」）。
///
/// ⚠ **口径是近似的，而且近似得有方向**：它不知道前端哪些记录最终不建卡（`stripInternalNoise`、
/// ESC 折叠、slash/compact 细条…），只按记录的**原料**数。前端据 `t` / `mt` / `fd` 分档，
/// 分不准的那几档落到偏保守的常数（`§2.5b`：「粗估用偏保守的常数，精算后往下修」）。
#[derive(Debug, Default, serde::Serialize, PartialEq, Eq)]
pub(crate) struct IndexRow {
    /// 行起点字节偏移（绝对，0-based）—— 按需取正文就是 `[o, o+n)`。
    pub(crate) o: u64,
    /// 行字节长（**含**结尾 `\n`）。
    pub(crate) n: u64,
    /// 记录 `type`；解析不出（非 JSON / 没有 type）⇒ 省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) t: Option<String>,
    /// `uuid`（前端 `uuidToIdx` —— 跳转与对账的锚）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) u: Option<String>,
    /// `isSidechain == true`。
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) sc: bool,
    /// `isMeta == true`（skill 注入 / 命令回显 —— 前端不建用户卡）。
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) mt: bool,
    /// 正文字符数（**代码块之外**；Unicode 标量计，不含换行）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) ch: u32,
    /// 其中 CJK/全宽字符数（口径 = `height-estimate.ts::fallbackTextHeight` 的 `> 0x2E80`）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cj: u32,
    /// 正文**非空**硬行数（代码块之外）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) pl: u32,
    /// 围栏代码块数（```` ``` ```` / `~~~` 开合一对算一个；没合上的算到文末）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cb: u32,
    /// 代码块内总行数。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cl: u32,
    /// 折叠单元数：`tool_use` / `tool_result` / `thinking` / `redacted_thinking` / `image` 块。
    /// 它们渲染成一行 summary（或并进工具组），与正文长短无关。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) fd: u32,
    /// 〔SE2〕这一行是一条**用户输入**（大纲的一项）⇒ 它的摘要；不是 ⇒ 省略。
    ///
    /// 判定只住 [`crate::observe::user_inputs::user_input_of`]（与 `--list-user-inputs` **同一个函数**），
    /// 摘要同 `excerpt`、uuid 就是本行的 `u`。有了它，首屏的「索引」与「大纲清单」合成一趟读：
    /// 前端见到索引里**有** `x` ⇒ 对面是会出它的后端、每一条用户输入都带着 ⇒ 不再单独要清单；
    /// 一个 `x` 都没有 ⇒ 分不清「老后端」还是「真的零条」⇒ 照旧要一份（形状登记 `IPC-PROTOCOL.md §10.3`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) x: Option<String>,
    /// 〔SE2〕同上那一行的 `timestamp`（空串 ⇒ 省略；清单那边的空串 == 这里缺席）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ts: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// 一行 → [`IndexRow`]。**纯函数**，不碰文件系统。
///
/// 解析失败（半截 / 非 JSON）**不丢这一行** —— 它仍占一个 seq，只是没有料（`t` 省略）；
/// 丢了它，后面每一行的 seq 都会错一位（那是「计数对不上而且不会报错」的那一形）。
pub(crate) fn index_row(line: &[u8], offset: u64, len: u64) -> IndexRow {
    let mut row = IndexRow {
        o: offset,
        n: len,
        ..IndexRow::default()
    };
    let text = String::from_utf8_lossy(line);
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return row;
    };
    row.t = v.get("type").and_then(|t| t.as_str()).map(str::to_string);
    row.u = v.get("uuid").and_then(|u| u.as_str()).map(str::to_string);
    row.sc = v.get("isSidechain").and_then(|b| b.as_bool()) == Some(true);
    row.mt = v.get("isMeta").and_then(|b| b.as_bool()) == Some(true);
    // 〔SE2〕大纲那一项（判定只住 `user_inputs`；这里只搬字段）
    if let Some(ui) = crate::observe::user_inputs::user_input_of(&v) {
        row.x = Some(ui.excerpt);
        row.ts = Some(ui.timestamp).filter(|t| !t.is_empty());
    }
    // 正文在哪：user/assistant 在 `message.content`（字符串或块数组）；system 在顶层 `content`。
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .or_else(|| v.get("content"));
    match content {
        Some(serde_json::Value::String(s)) => count_prose(s, &mut row),
        Some(serde_json::Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(s) = b.get("text").and_then(|t| t.as_str()) {
                            count_prose(s, &mut row);
                        }
                    }
                    Some(
                        "tool_use" | "tool_result" | "thinking" | "redacted_thinking" | "image",
                    ) => row.fd += 1,
                    _ => {}
                }
            }
        }
        _ => {}
    }
    row
}

/// 数一段 markdown 正文：围栏代码块内外分开数（代码走等宽行高、正文走折行估计，两者算法不同）。
fn count_prose(s: &str, row: &mut IndexRow) {
    let mut in_code = false;
    for line in s.split('\n') {
        let lead = line.trim_start_matches(' ');
        // 围栏：行首至多 3 个空格后 ``` 或 ~~~（CommonMark 口径的近似；不区分开合的字符种类）
        if line.len() - lead.len() <= 3 && (lead.starts_with("```") || lead.starts_with("~~~")) {
            if !in_code {
                row.cb += 1;
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            row.cl += 1;
            continue;
        }
        let l = line.trim_end_matches('\r');
        if l.trim().is_empty() {
            continue;
        }
        row.pl += 1;
        for c in l.chars() {
            row.ch += 1;
            if c as u32 > 0x2e80 {
                row.cj += 1;
            }
        }
    }
}

/// 纯：offset 续拉的字节切片语义（**仅供单测**对拍 aterm `tail -c +(offset+1)`；生产走
/// `stream_from_offset` 的 `File::seek`、不调本函数，故 `#[cfg(test)]` 不进生产二进制）。
/// = `bytes[min(offset,len)..]`——offset ≤ len 时取 [offset, EOF]；offset > len
/// （截断）时取空（与 `File::seek` 过 EOF 后读空一致，不 panic）。
#[cfg(test)]
fn slice_from_offset(bytes: &[u8], offset: u64) -> &[u8] {
    let o = (offset as usize).min(bytes.len());
    &bytes[o..]
}

/// Batch9-F30：`--read-session-tail <path> <N>`——尾部优先输出：
/// 首行 meta `{"kind":"snapshot_meta","total":T,"tail_from":F}`（T/F 均按
/// **可计行**口径：完整（`\n` 收尾）且非 BOM/全空白——与 watcher/monitor 的
/// 行号空间一字一致），随后原样输出可计行 [F,T)（最新 N 行）、再输出 [0,F)。
/// monitor 据 meta 编 seq：前 T-F 行 = F+i，其余 = i。空文件 → 仅 meta。
fn read_session_tail(agent_home: &Path, jsonl_path: &str, n: usize) -> Result<(), String> {
    use std::io::{Read, Seek, SeekFrom};
    let (plan, mut f) = tail_plan(agent_home, jsonl_path, n)?;
    let TailPlan {
        total,
        tail_from,
        split_at,
        end: complete_end,
    } = plan;
    let meta =
        format!("{{\"kind\":\"snapshot_meta\",\"total\":{total},\"tail_from\":{tail_from}}}\n");
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(meta.as_bytes())
        .map_err(|e| format!("stream failed: {e}"))?;
    // 尾段 [split_at, complete_end)
    f.seek(SeekFrom::Start(split_at))
        .map_err(|e| format!("seek failed: {e}"))?;
    std::io::copy(&mut (&mut f).take(complete_end - split_at), &mut out)
        .map_err(|e| format!("stream failed: {e}"))?;
    // 头段 [0, split_at)
    f.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek failed: {e}"))?;
    std::io::copy(&mut (&mut f).take(split_at), &mut out)
        .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// `--read-session-tail` 那一趟扫描的结果：可计行总数 · 尾段起点行号 · 两段的字节边界。
///
/// 〔`C1` · 2026-09-24〕抽出来是因为帧面那条（`history-tail`）只要**这张图**，
/// 正文按字节区间另走 `history-read` 分页拉 —— 一帧应答装不下几十 MB 的会话，
/// 而 CLI 这条仍然一口气印完。**两条路扫的是同一个函数**，行号口径因此只有一份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TailPlan {
    /// 可计行总数（`line_counts` 口径）。
    pub total: u64,
    /// 尾段第一行的行号。
    pub tail_from: u64,
    /// 尾段第一行的字节起点（＝ 头段的字节终点）。
    pub split_at: u64,
    /// 最后一个完整行（`\n` 收尾）之后的字节位置；torn 残尾不在任何一段里。
    pub end: u64,
}

/// 扫一遍，出 [`TailPlan`] 与已打开的那份文件（调用方按区间去读）。
pub(crate) fn tail_plan(
    agent_home: &Path,
    jsonl_path: &str,
    n: usize,
) -> Result<(TailPlan, std::fs::File), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    // 审计 D：整文件 std::fs::read 在 Pi 级设备上对数百 MB 会话有 OOM 风险
    // （旧 --read-session 是 io::copy 流式）——改单遍流式扫描（环形缓冲只存
    // 最近 N 个可计行的字节偏移，O(N) 内存）+ 两次 seek 范围拷贝。
    use std::io::BufRead;
    let f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let mut reader = std::io::BufReader::new(f);
    let mut recent: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
    let keep = n.max(1);
    let mut total: u64 = 0;
    let mut pos: u64 = 0;
    let mut complete_end: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| format!("scan failed: {e}"))?;
        if read == 0 {
            break;
        }
        let line_start = pos;
        pos += read as u64;
        if *buf.last().unwrap() != b'\n' {
            break; // torn 残尾不计（F14 口径）
        }
        complete_end = pos;
        if !line_counts(&buf[..buf.len() - 1]) {
            continue; // 空行不计（与 watcher/monitor 口径一致 —— 判定只有 `line_counts` 一个住址）
        }
        total += 1;
        recent.push_back(line_start);
        if recent.len() > keep {
            recent.pop_front();
        }
    }
    let tail_from = total - recent.len() as u64;
    let split_at = recent.front().copied().unwrap_or(complete_end);
    Ok((
        TailPlan {
            total,
            tail_from,
            split_at,
            end: complete_end,
        },
        reader.into_inner(),
    ))
}

/// 帧面按字节区间分页读一份会话的**一页**（`history-read`）。
///
/// # 为什么要分页（`C1` · 2026-09-24）
///
/// 一次性子命令把整份会话**流**回去（`io::copy`），连接关了就是 EOF；帧面是一问一答，
/// 一帧应答要整个进内存、整个过线 —— 而本仓见过 270 MB 的会话，monitor 那头单帧上限
/// 64 MiB。⇒ 调用方给 `[offset, until)`，本函数回**不超过 `page` 字节、切在行尾**的一页，
/// 外加续点 `next`；调用方循环到 `eof`。
///
/// # 切法
///
/// - 这一页之后区间里还有字节 ⇒ 切在**最后一个 `\n` 之后**，不把半行交出去；
/// - 一页里一个 `\n` 都没有（单行比 `page` 还长）⇒ 往后续读到那一行的 `\n`，
///   但**不超过 `line_cap`** —— 超了回 `oversized_line`（不许截半行冒充整行）；
/// - 区间到头（`until` 或读时的文件长度）⇒ 余下的全给，含 torn 残尾（与 `--read-session`
///   的 `io::copy` 同口径：残尾怎么处置由调用方定）。
///
/// 错误是 `(code, message)`：`refused`（围栏拒）· `failed`（读失败）· `oversized_line`
/// （不叫 `line_too_long`：那是入方向信封的**协议级** code，命令级不许撞名）。
pub(crate) fn read_page(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<ReadPage, (&'static str, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let mut f =
        std::fs::File::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    let len = f
        .metadata()
        .map_err(|e| ("failed", format!("stat failed: {e}")))?
        .len();
    let limit = until.map_or(len, |u| u.min(len));
    if offset >= limit {
        return Ok(ReadPage {
            bytes: Vec::new(),
            next: offset,
            eof: true,
        });
    }
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| ("failed", format!("seek failed: {e}")))?;
    let mut src = f.take(limit - offset);
    let mut bytes = Vec::with_capacity(page.min((limit - offset) as usize));
    (&mut src)
        .take(page as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| ("failed", format!("read failed: {e}")))?;
    let reached_limit = offset + bytes.len() as u64 >= limit;
    if !reached_limit {
        match bytes.iter().rposition(|&b| b == b'\n') {
            Some(i) => bytes.truncate(i + 1),
            None => {
                // 单行比一页还长：续读到它的行尾，但不超过 `line_cap`。
                let mut more = [0u8; 64 * 1024];
                loop {
                    let k = src
                        .read(&mut more)
                        .map_err(|e| ("failed", format!("read failed: {e}")))?;
                    if k == 0 {
                        break;
                    }
                    let done = match more[..k].iter().position(|&b| b == b'\n') {
                        Some(i) => {
                            bytes.extend_from_slice(&more[..=i]);
                            true
                        }
                        None => {
                            bytes.extend_from_slice(&more[..k]);
                            false
                        }
                    };
                    // ⚠ 上限在**每一次**续读之后判，包括找到行尾的那一次 ——
                    //   只在「没找到」那一支判的话，行尾恰好落在这一块里的超长行会被整行交出去。
                    if bytes.len() > line_cap {
                        return Err((
                            "oversized_line",
                            copy_text(
                                "beHistoryQuery.readPage.lineTooLong",
                                &[
                                    ("lineCap", &line_cap.to_string()),
                                    ("offset", &offset.to_string()),
                                ],
                            ),
                        ));
                    }
                    if done {
                        break;
                    }
                }
            }
        }
    }
    let next = offset + bytes.len() as u64;
    Ok(ReadPage {
        bytes,
        next,
        eof: next >= limit,
    })
}

/// [`read_page`] 的一页。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadPage {
    /// 原始字节（调用方负责解码 —— 帧面那层用 `from_utf8_lossy`）。
    pub bytes: Vec<u8>,
    /// 下一页从这里起。
    pub next: u64,
    /// 区间到头了。
    pub eof: bool,
}

/// 〔CF2 · 第四波 4B〕帧面按**行号**取回一段（`history-lines`）：第 `[from, until)` 个可计行的原文。
///
/// # 为什么要有它（`调研/第四波记录/CF2.md §1`）
///
/// 按字节取正文（[`read_page`]）要调用方先知道字节边界 —— 那只有骨架索引给得出。
/// 没接骨架的会话（后台 tab · 老后端 · seq 对不上的）丢掉的正文就无处可回 ⇒ monitor 的重放缓冲
/// 不敢设上界（`设计/05 §3.3.4` 级 3）。本函数是**不依赖索引**的那条取回路：调用方只说行号。
///
/// # 口径
///
/// - 「第几行」只有一个家：[`line_counts`]（BOM 与全空白不占号）＋ 没 `\n` 收尾的残尾不计 ——
///   与 [`tail_plan`] 的 `total` · [`scan_session_index`] 的行号 · 实时 `line` 帧的 `seq` 同一个空间。
/// - 只交**可计行**（空行在号外，交出去调用方也不会给它编号）；第 k 条就是第 `from + k` 行。
/// - `until` 缺 ＝ 到最后一个完整行为止。
///
/// # 一帧装得下
///
/// 交出的原文累计达到 `page` 字节就停（至少交一行）；单行超过 `line_cap` ⇒ `oversized_line`
/// （同 [`read_page`]：不截半行）。停在中途时 `eof == false`、`next < until`，调用方接着要。
///
/// # 代价（写清楚，`CF2.md §1.2`）
///
/// **O(第 `from` 行之前的字节)**：后端零状态，每次从文件头数。本机最大一份会话（111 MB / 4.2 万行）
/// 热缓存从头数到尾 ≈ 22 ms；取回只在用户往上翻过了前端手里最老那一条时才发生，一次一批。
/// 真撞上「弱设备上超大会话翻得卡」的读数时再加锚（`CF2.md §1.1` 候选 C），不先加。
///
/// 错误同 [`read_page`]：`refused` · `failed` · `oversized_line`。
pub(crate) fn read_lines(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let f = std::fs::File::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    read_lines_from(std::io::BufReader::new(f), from, until, page, line_cap)
}

/// [`read_lines`] 的内核：读 `r`（从文件头起）。**纯 I/O 泛型**，单测直接喂字节。
pub(crate) fn read_lines_from<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let until = until.unwrap_or(u64::MAX);
    let mut lines: Vec<String> = Vec::new();
    let mut bytes: usize = 0;
    let mut n: u64 = 0; // 下一个可计行的行号
    let mut buf: Vec<u8> = Vec::new();
    let eof = loop {
        if n >= until {
            break false;
        }
        buf.clear();
        let read = r
            .read_until(b'\n', &mut buf)
            .map_err(|e| ("failed", format!("scan failed: {e}")))?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break true; // 文件到头；torn 残尾不计（同 `tail_plan`）
        }
        let body = &buf[..buf.len() - 1];
        if !line_counts(body) {
            continue;
        }
        let at = n;
        n += 1;
        if at < from {
            continue;
        }
        if body.len() > line_cap {
            return Err((
                "oversized_line",
                copy_text(
                    "beHistoryQuery.readLinesFrom.lineTooLong",
                    &[("at", &at.to_string()), ("lineCap", &line_cap.to_string())],
                ),
            ));
        }
        bytes += body.len();
        lines.push(String::from_utf8_lossy(body).into_owned());
        if bytes >= page {
            break false;
        }
    };
    let next = from + lines.len() as u64;
    Ok(LinesPage {
        from,
        lines,
        next,
        eof,
    })
}

/// [`read_lines`] 的一段。不变量：`next == from + lines.len()`。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LinesPage {
    /// 第一条的行号（＝ 请求里的 `from`）。
    pub from: u64,
    /// 可计行的原文（不含行尾 `\n`；`\r` 与 BOM 原样留着，调用方剥）。
    pub lines: Vec<String>,
    /// 下一段从这一行起。
    pub next: u64,
    /// 读到了最后一个完整行之后（后面没有了）。
    pub eof: bool,
}

/// 〔U4b · 第四波〕「这条会话的记录还在不在」—— `history-record` 帧命令的本体（`read_face` 是它的宿主）。
///
/// `设计/01 §6.2` 最后一条逐字：「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」。
/// 前端 resume 一跳（`tab-session-actions.ts`）在开终端之前问一次；答「不在」就不开。
///
/// - **只收 sid、不收路径**（`INVARIANTS §41.6` 收窄第 3 条）：找文件那一步是两侧共用的
///   `agents::find_session_file`（〔THIN〕经注册表那一格，本体住 `agents/claudecode/branch.rs`；与分叉 / 删会话同一份；符号链接不算命中）。
/// - sid 形状不合法 ⇒ `Err`，**先于任何 IO**；找不到 ⇒ `Ok(present = false)` —— 这是一个答案，不是错误。
/// - 回 `root`（查的是哪棵记录树）：报错时要说清查了什么（`设计/01 §6.9`）。
///
/// ⚠ **射程如实写**：它查的是**这台后端**的记录树（`agent_home()/projects`）。会话若起在另一个
/// 账号的配置根下（`CLAUDE_CONFIG_DIR` 指向别处），这里答「不在」而那边其实有 —— 调用方拿到「不在」
/// 时报的话要说清是「这棵树里没有」，不是「世上没有」。
pub(crate) fn record_in(agent_home: &Path, sid: &str) -> Result<RecordProbe, String> {
    if !shell_quote_core::session_id_ok(sid) {
        return Err(crate::common::contract::malformed(&format!(
            "bad session id shape (letters, digits, hyphen; 1..=64): {sid:?}"
        )));
    }
    let root = projects_root(agent_home);
    let present = crate::agents::find_session_file(&root, sid).is_ok();
    Ok(RecordProbe {
        present,
        root: root.to_string_lossy().into_owned(),
    })
}

/// 〔GP1 · 第四波〕[`record_in`] 按**这次 resume 要用的那个账号配置目录**查（`history-record` 的 `configDir`）。
///
/// 会话起在另一个账号根下（`CLAUDE_CONFIG_DIR` 指别处）时，只查这台后端自己的家目录会答「不在」、误拦 resume
/// （`设计/30 §8` 第 4 条；上面那段「射程如实写」）。monitor 把这次 resume 交给起会话那一格的同一个目录带过来，
/// 这里就在那棵树里找。
/// - `None` ⇒ 这台的家目录（与改之前逐字同一问）；
/// - `Some(d)` ⇒ 先过账号库那一个形状关（`accounts_query::is_safe_config_dir`：绝对 · 不上跳 · 无 shell 元字符与
///   欺骗字符），不过 ⇒ `Err`，**一次 IO 都不做**；过了 ⇒ 查 `<d>/projects`。
/// 只答在不在、查的是哪棵树 —— 不回内容，不收别的路径。
pub(crate) fn record_for(
    agent_home: &Path,
    config_dir: Option<&str>,
    sid: &str,
) -> Result<RecordProbe, String> {
    let tree = match config_dir {
        None => agent_home.to_path_buf(),
        Some(d) if super::accounts_query::is_safe_config_dir(d) => std::path::PathBuf::from(d),
        Some(d) => {
            return Err(crate::common::contract::malformed(&format!(
                "configDir is not a queryable account config dir \
                 (absolute path, no `..`, no shell metacharacters): {d:?}"
            )))
        }
    };
    record_in(&tree, sid)
}

/// [`record_in`] 的答案。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordProbe {
    /// `<sid>.jsonl` 在记录树里（根那一层或项目目录那一层）找得到。
    pub present: bool,
    /// 查的那棵记录树的根。
    pub root: String,
}

/// 帧面那几条要的「这台的 agent 家目录」—— 与 `main.rs` 那一句**同一个出处**。
///
/// 住这里而不是帧面宿主那边：通用层不许点 agent 的名字（`agent_boundary_guard`），
/// 而本层今天本来就是 Claude 专属的（`observe/mod.rs` 头注）。
pub(crate) fn agent_home() -> std::path::PathBuf {
    crate::agents::claudecode::paths::resolve_home()
}

/// 这一行**算不算一行**（＝ 口径 `watcher::read_new_lines`：BOM 与全空白跳过）。
///
/// 🔴 **全仓只有这一个住址** —— 扫描循环数 `total`、`split_tail` 数 `starts`，
/// 两边判得必须**一模一样**；口径一分家，计数就对不上，而那种错**不会报错**。
///
/// # 为什么先看字节（秤 7，`tests/evidence/S7-rust-side.md`）
///
/// 原来这一跳对**每一行**做 `String::from_utf8_lossy(整行)` 再 `trim()`，
/// **只为回答「这行是不是空的」** —— 那是一次**整文件 UTF-8 校验**。
/// 实测（15.1 MiB 会话，gpd）：`tail` **19.52 → 3.79 ms**、其中「数行」那一趟
/// **17.89 → 2.98 ms（↓83%）**，**出字节一字节不差**。
/// 剩下的 ~3 ms/15.1 MiB（≈5 GiB/s ≈ `memchr`）才是 `设计/17 §3.1` 说的
/// 那个**真·不可避免的 O(文件)**。
///
/// # 短路为什么是对的
///
/// - 任何 `< 0x80` 的字节都是**独立的 ASCII 字符** —— UTF-8 多字节序列的
///   首字节与续字节**全部 ≥ 0x80** ⇒ 不会把半个字符误读成 ASCII。
/// - BOM（`U+FEFF` ＝ `EF BB BF`）**三个字节全非 ASCII** ⇒ 短路条件碰不到它。
/// - ⇒ 只要有一个 ASCII 非空白字节，这行剥 BOM、`trim` 完**必然非空**。
/// - 短路**不成立**时退回原判定（保守），所以这不是近似，是**等价**。
///
/// ⚠⚠ **谓词必须是 `char::from(b).is_whitespace()`，不能写成 `b.is_ascii_whitespace()`**
/// —— 后者**不含 `U+000B`**（垂直制表），而 `str::trim` 含它。
/// 秤 7 的第一版变体就是这么写错的，**靠穷举才逮出来**（全部 1/2/3 字节共
/// 16 843 008 个 ＋ 300 万随机串，分歧 0；证据 `tests/evidence/S7-blank-line-equivalence.rs`）——
/// **单测没逮住，因为没有那个样本。**
pub(crate) fn line_counts(line: &[u8]) -> bool {
    // 快路：有 ASCII 非空白 ⇒ 必非空，不必解码
    if line
        .iter()
        .any(|b| b.is_ascii() && !char::from(*b).is_whitespace())
    {
        return true;
    }
    // 慢路：可能全是空白 / 多字节 ⇒ 按原判定来
    let text = String::from_utf8_lossy(line);
    !text.trim_start_matches('\u{feff}').trim().is_empty()
}

/// 纯函数：把文件字节按"最新 N 可计行优先"切成 (meta 行, 尾段, 头段)。
/// 只处理到最后一个 `\n`（torn 残尾不进任何段——F14 口径）。
/// 生产路径已流式化（read_session_tail，审计 D 内存修订）；本函数保留为
/// 口径锚点（tail_tests 锚定语义），流式版与它的等价性由本机行为验证对账
/// （真实 18MB 会话：meta/字节输出逐段一致，见 Batch9 feature 30 §6 留档）。
/// **仅测**（生产走流式版、不调本函数）→ `#[cfg(test)]` 不进生产二进制。
#[cfg(test)]
fn split_tail(bytes: &[u8], n: usize) -> (String, &[u8], &[u8]) {
    let complete_end = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let complete = &bytes[..complete_end];
    // 收集每个可计行的起始字节偏移（口径 = watcher::read_new_lines：BOM/全空白跳过）
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    for line in complete.split_inclusive(|&b| b == b'\n') {
        if line_counts(&line[..line.len() - 1]) {
            starts.push(pos);
        }
        pos += line.len();
    }
    let total = starts.len();
    let tail_from = total.saturating_sub(n.max(1));
    let meta =
        format!("{{\"kind\":\"snapshot_meta\",\"total\":{total},\"tail_from\":{tail_from}}}\n");
    let split_at = starts.get(tail_from).copied().unwrap_or(complete_end);
    (meta, &complete[split_at..], &complete[..split_at])
}

fn created_ms_or_mtime(p: &Path) -> i64 {
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    let t = meta.created().or_else(|_| meta.modified());
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 从 jsonl 头部（前 40 行）提取首个带 cwd 的记录的 cwd。
/// ⚠〔audit-0805 F07 / 报告 I-10〕**只看前 40 行，就别整读**。
///
/// 这里原本是 `read_to_string(p)` —— 一个 257 MB 的会话会被整份读进内存，
/// 而下一行就是 `.take(40)`。`--list-projects` 对**每个项目**都会调它一次。
///
/// ★ 对照 —— 〔`K-R97` 09-12 改写，上一版说的是 monitor 侧那一份〕：monitor 从前也有一份
/// 同功能的头部提取（`BufReader` + 前 30 行早返回），于是同一个问题两边给两个答案。
/// 本机项目列表改走本查询之后，**那一份连同它唯一的调用点一起没了** ——
/// 这件事今天全仓只剩这一处。原话记的那条「强机器整读、弱机器流式，正好反了」（同 B-4）
/// 仍然是本函数存在的理由。
fn extract_cwd_from_head(p: &Path) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open(p).ok()?;
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().map_while(Result::ok).take(40) {
        let line = line.as_str();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
                if !cwd.is_empty() {
                    return Some(cwd.to_string());
                }
            }
        }
    }
    None
}

/// 单个会话的元数据提取（整文件扫描，跑在那台机器的 CPU 上）：
/// - messageCountApprox = 非空行数
/// - firstUserExcerpt = 首条"真用户输入"（跳过 isMeta / 工具结果 / 纯中断标记，剥 CLI 注入的包装）的前 120 字符
///   （换行折成空格、超了加 `…`）
/// - aiTitle = 最后一条标题记录（`ai-title` 的 `aiTitle` 与 CC v2.1.x 起的 `custom-title` 的 `customTitle`，取最新）
/// - cwd = 首个带 cwd 的记录
/// - startedAtMs = 首条 user / assistant 记录的 `timestamp`；一条都解析不出 ⇒ 文件建立时刻（拿不到 ⇒ 修改时刻）
/// - forkedFromSessionId / forkedFromMessageUuid = 首条带 `forkedFrom` 的 user / assistant 记录（`/branch` 分叉来的）
///
/// 〔C4d · 第四波 4B〕**这一行从此是本机与远端共用的唯一口径**：本机的历史会话清单此前由 monitor 进程内自己扫
/// （`history·rs::analyze_jsonl`〔散文墓碑〕，经记录解析器），与这里的「精简版」各算各的 —— 开始时刻 / 摘录 / 标题 / fork 关系
/// 四格两边不一样。历史跨机 join 进了本机后端之后本机也读这一行 ⇒ 把 monitor 那份有、这里没有的三格（fork 关系 ·
/// `custom-title` · 首条时间戳）补进来，摘录的清洗与截断改用 `search-core` 那一份（与全文搜索同一个家）。条数仍是「非空行数」。
fn analyze_session(p: &Path) -> serde_json::Value {
    let session_id = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut count = 0u32;
    let mut excerpt = String::new();
    let mut ai_title: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut started_at: Option<i64> = None;
    let mut forked: Option<(String, String)> = None;
    // Batch11-F32：CC 2.1.x 后台分身会话（←/bg/退出转后台 fork 出的 worker）——
    // 记录级 sessionKind:"bg" 是官方 resume 选择器同款识别信号（内部字段无兼容
    // 承诺，缺失=false 安全降级）。历史列表标 ⚙ 徽标防 resume 选错克隆。
    let mut is_bg = false;
    // 〔audit-0805 F07 / 报告 B-6 第 5 环〕**流式**。原来是 `read_to_string(p)` 整读：
    // `--list-sessions` 对该项目**每个** jsonl 都调它一次，43 个项目 / 2.4 GB 的机器上
    // 一次列表就是把 2.4 GB 读进内存再逐行解析。头注自己也写着「整文件扫描，跑在远端 CPU 上」——
    // 扫描是必须的（要数行、要判 bg），**但不必先整份进内存**。
    if let Ok(file) = std::fs::File::open(p) {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim_start_matches('\u{feff}').trim();
            if trimmed.is_empty() {
                continue;
            }
            count += 1;
            let v: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if !is_bg && v.get("sessionKind").and_then(|k| k.as_str()) == Some("bg") {
                is_bg = true;
            }
            if cwd.is_none() {
                if let Some(c) = v.get("cwd").and_then(|c| c.as_str()) {
                    if !c.is_empty() {
                        cwd = Some(c.to_string());
                    }
                }
            }
            let kind = v.get("type").and_then(|t| t.as_str());
            if matches!(kind, Some("user") | Some("assistant")) {
                if started_at.is_none() {
                    started_at = v
                        .get("timestamp")
                        .and_then(|t| t.as_str())
                        .and_then(crate::observe::search_query::parse_iso8601_ms);
                }
                if forked.is_none() {
                    forked = fork_origin(&v);
                }
            }
            match kind {
                Some("ai-title") => {
                    if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()) {
                        ai_title = Some(t.to_string()); // 取最新（持续覆盖）
                    }
                }
                // CC v2.1.x 起标题记录改名 `custom-title` / `customTitle`（旧的 `ai-title` 在历史记录里仍会出现，两个都认）。
                Some("custom-title") => {
                    if let Some(t) = v.get("customTitle").and_then(|t| t.as_str()) {
                        ai_title = Some(t.to_string());
                    }
                }
                Some("user")
                    if excerpt.is_empty()
                        && v.get("isMeta").and_then(|m| m.as_bool()) != Some(true) =>
                {
                    if let Some(text) = user_text(&v) {
                        let cleaned = search_core::clean_user_text(&text);
                        if !cleaned.is_empty() {
                            excerpt = search_core::truncate_excerpt(&cleaned, 120);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let (forked_sid, forked_uuid) = match forked {
        Some((s, u)) => (Some(s), Some(u)),
        None => (None, None),
    };
    serde_json::json!({
        "sessionId": session_id,
        "jsonlPath": p.to_string_lossy(),
        "startedAtMs": started_at.unwrap_or_else(|| created_ms_or_mtime(p)),
        "updatedAtMs": mtime_ms(p),
        "messageCountApprox": count,
        "firstUserExcerpt": excerpt,
        "aiTitle": ai_title,
        "cwd": cwd,
        "isBg": is_bg,
        "forkedFromSessionId": forked_sid,
        "forkedFromMessageUuid": forked_uuid,
    })
}

/// 〔STC · `设计/90 §4` 阶段 C〕**「这条记录说明本会话是从哪个会话分叉来的」的唯一住址**：
/// `user` / `assistant` 记录上的 `forkedFrom`（两个键都得是串）⇒ (源会话 id, 分叉处的消息 uuid)。
///
/// 两个读者调同一个函数：历史会话行（[`analyze_session`] 的 `forkedFromSessionId`）与活 tab 的会话事实
/// （`facts_query.rs` 的 `forkedFrom`）—— 搬之前活 tab 那一份住前端、口径更松（任何记录、只看 `sessionId`），
/// 同一个会话会在历史树与 tab 栏上一个认是分叉、一个不认。
pub(crate) fn fork_origin(v: &serde_json::Value) -> Option<(String, String)> {
    match v.get("type").and_then(|t| t.as_str()) {
        Some("user") | Some("assistant") => forked_from(v),
        _ => None,
    }
}

/// 一条记录的 `forkedFrom`（`/branch` 分叉出来的会话，每条复制过来的记录都带着）→ (源会话 id, 分叉处的消息 uuid)。
fn forked_from(v: &serde_json::Value) -> Option<(String, String)> {
    let f = v.get("forkedFrom")?;
    Some((
        f.get("sessionId")?.as_str()?.to_string(),
        f.get("messageUuid")?.as_str()?.to_string(),
    ))
}

/// user 记录的纯文本内容：message.content 为字符串直接用；为数组取首个 text 块。
/// 工具结果（tool_result 块）返回 None——它不是用户敲的。
fn user_text(v: &serde_json::Value) -> Option<String> {
    let content = v.get("message")?.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        for block in arr {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

// 〔C4d〕按字符截断的那一份（`truncate_chars`〔散文墓碑〕）没了读者：摘录改用 `search_core::truncate_excerpt`（同样不劈码点，
//   另把换行折成空格、超了加 `…` —— 与本机那条路从前的口径、与全文搜索的标题摘录同一个家）。

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_tail_tests.rs"]
mod tail_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_f07_tests.rs"]
mod f07_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_kr83_tests.rs"]
mod kr83_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_index_tests.rs"]
mod index_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_user_inputs_tests.rs"]
mod user_inputs_tests;
