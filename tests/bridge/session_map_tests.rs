
/// ★★ **心跳分支不许重读文件**〔audit-0805 08-08，Phase G 第 57 件，E12〕。
///
/// `diff_sessions` 的头注逐字写着「scan → 本函数，是状态变化的**唯一检出点**
/// （心跳分支不重读文件）」。那句话撑着一条真实的性能与语义契约：
/// 心跳每 2s 一次，只做 `is_process_alive` 探活；一旦它也去 `scan_dir`，
/// 就变成**每 2s 一次全目录读盘**，而且状态变化会有两个检出点、各自发一份事件。
///
/// ⇒ 而它**只是散文**（E12 的判准是「有没有一条会红的判据读它」）。本条就是那条。
///
/// 钉法：`run_watcher` 的**心跳分支**（`} else {` 之后到函数收尾）里不许出现
/// `scan_dir(`；同时要求 `scan` 分支里**确实有**一处，否则本条在「两边都没有」
/// 的退化状态下会零命中地绿。
#[test]
fn the_heartbeat_branch_never_rereads_the_directory() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/session_map.rs"));
    let at = prod
        .find("fn run_watcher")
        .expect("生产段里没有 `fn run_watcher` —— 抽取器坏了，本条此刻无效");
    let body: Vec<&str> = prod[at..]
        .lines()
        .take_while(|l| {
            let cont = l.starts_with("where") || l.starts_with(')') || l.trim() == "{";
            l.is_empty() || l.starts_with(char::is_whitespace) || cont || l.starts_with("fn ")
        })
        .collect();
    // 心跳分支的起点：那句 `} else {`（`if scan {` 的否定支）。
    let split = body
        .iter()
        .position(|l| l.trim() == "} else {")
        .expect("找不到 `} else {` —— `run_watcher` 的双触发结构变了，本条此刻无效");
    let (scan_half, beat_half) = body.split_at(split);
    // 自检：扫描分支里确实读了目录，否则下面那句是空转。
    assert!(
        scan_half.iter().any(|l| l.contains("scan_dir(")),
        "`if scan` 分支里找不到 `scan_dir(` —— 双触发结构变了，本条此刻无效"
    );
    let offenders: Vec<&&str> = beat_half
        .iter()
        .filter(|l| l.contains("scan_dir("))
        .collect();
    assert!(
        offenders.is_empty(),
        "心跳分支里出现了 `scan_dir(`：{offenders:?}\n\
             ⚠ 心跳每 2s 一次。它一旦重读目录，就是**每 2s 一次全目录读盘**；\n\
             而且状态变化从此有两个检出点，同一次变化会被发两遍事件。\n\
             `diff_sessions` 的头注逐字写着「心跳分支不重读文件」—— 那句话由本条守着。"
    );
}
use super::*;

/// Batch7-F24：scan_dir 的开关双分支——开（默认）保留 bg 且 kind/name 透传；
/// 关 = F21 行为（bg 不算会话）。
#[test]
fn scan_dir_show_bg_switch() {
    let dir = std::env::temp_dir().join(format!("ccm-scanbg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("1.json"),
        r#"{"pid":1,"sessionId":"sid-int","cwd":"/p","kind":"interactive"}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("2.json"),
        r#"{"pid":2,"sessionId":"sid-bg","cwd":"/p","kind":"bg","name":"评估"}"#,
    )
    .unwrap();
    let on = scan_dir(&dir, true);
    assert_eq!(on.len(), 2, "开 = bg 保留");
    assert_eq!(on["sid-bg"].kind.as_deref(), Some("bg"), "kind 透传下游");
    assert_eq!(on["sid-bg"].name.as_deref(), Some("评估"), "name 透传下游");
    let off = scan_dir(&dir, false);
    assert_eq!(off.len(), 1, "关 = F21 行为");
    assert!(off.contains_key("sid-int"));
    std::fs::remove_dir_all(&dir).ok();
}

/// v2.22.2:同 sid 多 pidfile 的 kind 冲突消解——interactive 恒压过 bg,
/// 与目录扫描顺序无关(实证形态:cc-daemon bg-spare 复用父会话 sid)。
#[test]
fn scan_dir_same_sid_interactive_wins_over_bg() {
    let dir = std::env::temp_dir().join(format!("ccm-kindrace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // bg 文件名排前(1.json),interactive 排后(2.json)——旧实现目录序先到先得会输
    std::fs::write(
        dir.join("1.json"),
        r#"{"pid":3051720,"sessionId":"sid-parent","cwd":"/p","kind":"bg","name":"迁移服务","jobId":"sid-pare"}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("2.json"),
        r#"{"pid":16609,"sessionId":"sid-parent","cwd":"/p","kind":"interactive","name":"迁移服务"}"#,
    )
    .unwrap();
    let map = scan_dir(&dir, true);
    assert_eq!(map.len(), 1, "同 sid 归并成一条");
    assert_eq!(
        map["sid-parent"].kind.as_deref(),
        Some("interactive"),
        "interactive 压过 bg(不论扫描顺序)"
    );
    assert_eq!(
        map["sid-parent"].pid, 16609,
        "保留的是 interactive 那份 pidfile"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// v2.22.2:同 rank 平局判新——procStart 数值大者胜,缺失回退 pid。
#[test]
fn scan_dir_same_sid_same_kind_newer_wins() {
    let dir = std::env::temp_dir().join(format!("ccm-kindtie-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("1.json"),
        r#"{"pid":100,"sessionId":"s","cwd":"/p","kind":"interactive","procStart":"200"}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("2.json"),
        r#"{"pid":999,"sessionId":"s","cwd":"/p","kind":"interactive","procStart":"100"}"#,
    )
    .unwrap();
    let map = scan_dir(&dir, true);
    assert_eq!(
        map["s"].pid, 100,
        "procStart 更大(更新)者胜,与 pid 大小无关"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Batch6-F21/Batch7-F24：kind 解析 + 关开关时的过滤契约（开 = 保留带标注）。
#[test]
fn kind_field_parses_and_bg_is_filtered() {
    // 真实 bg 样本形态（本机 732685.json）：kind:"bg" + jobId
    let bg = r#"{"pid":732685,"sessionId":"6d2d9a38-55a0-4a46-a04e-18cadb0fc9af","cwd":"/x","kind":"bg","jobId":"6d2d9a38"}"#;
    let info: SessionInfo = serde_json::from_str(bg).unwrap();
    assert_eq!(info.kind.as_deref(), Some("bg"));

    let interactive = r#"{"pid":1,"sessionId":"s1","cwd":"/x","kind":"interactive"}"#;
    let legacy = r#"{"pid":2,"sessionId":"s2","cwd":"/x"}"#; // 旧 CC 无 kind
    let i2: SessionInfo = serde_json::from_str(interactive).unwrap();
    let i3: SessionInfo = serde_json::from_str(legacy).unwrap();

    // scan_dir 的过滤规则（产线谓词直测，审计 S2）：bg 拒、interactive 放、缺失放
    let mut info = info;
    let mut i2 = i2;
    let mut i3 = i3;
    assert!(
        !is_interactive(&String::new(), &mut info),
        "kind:bg must be filtered"
    );
    assert!(is_interactive(&String::new(), &mut i2));
    assert!(
        is_interactive(&String::new(), &mut i3),
        "legacy CC without kind must be kept"
    );
}

#[test]
fn parse_session_info() {
    // 来自 Claude Code 实际写入的 sessions/<PID>.json 的最小代表样本；
    // startedAt 等 monitor 不消费的字段也带上，确认 serde 默认能忽略未声明字段。
    // issue #23 起 status 被消费（红绿灯主信号）。
    let raw = r#"{"pid":35776,"sessionId":"5b67f422-52a9-453c-bd64-3288a78a24a0","cwd":"D:\\x","startedAt":1779157297377,"procStart":"639147828963703970","status":"busy"}"#;
    let info: SessionInfo = serde_json::from_str(raw).unwrap();
    assert_eq!(info.pid, 35776);
    assert_eq!(info.session_id, "5b67f422-52a9-453c-bd64-3288a78a24a0");
    assert_eq!(info.proc_start.as_deref(), Some("639147828963703970"));
    assert_eq!(info.status.as_deref(), Some("busy"));
    assert_eq!(info.waiting_for, None);
    assert_eq!(info.name, None);
}

// === issue #23: diff_sessions 行为测试（"变化才发"契约的唯一实现点） ===

/// Batch5-F18：骨架清单排序契约——HashMap 迭代序随机，(cwd, sid) 排序保证
/// tab 栏跨启动稳定且同项目相邻。
#[test]
fn snapshot_active_sorted_by_cwd_then_sid() {
    let mut a = mk("sid-b", None, None);
    a.cwd = "/proj/alpha".into();
    let mut b = mk("sid-a", None, None);
    b.cwd = "/proj/alpha".into();
    let mut c = mk("sid-c", None, None);
    c.cwd = "/proj/beta".into();
    let map = SessionMap {
        dir: std::path::PathBuf::new(),
        by_id: Arc::new(RwLock::new(as_map(vec![a, b, c]))),
        show_bg: true,
    };
    let out: Vec<(String, String)> = map
        .snapshot_active()
        .into_iter()
        .map(|e| (e.session_id, e.cwd))
        .collect();
    assert_eq!(
        out,
        vec![
            ("sid-a".to_string(), "/proj/alpha".to_string()),
            ("sid-b".to_string(), "/proj/alpha".to_string()),
            ("sid-c".to_string(), "/proj/beta".to_string()),
        ]
    );
}

fn mk(sid: &str, status: Option<&str>, waiting: Option<&str>) -> SessionInfo {
    SessionInfo {
        pid: 1,
        session_id: sid.to_string(),
        cwd: "x".into(),
        proc_start: None,
        status: status.map(String::from),
        waiting_for: waiting.map(String::from),
        name: None,
        kind: None,
    }
}
fn as_map(items: Vec<SessionInfo>) -> HashMap<String, SessionInfo> {
    items
        .into_iter()
        .map(|i| (i.session_id.clone(), i))
        .collect()
}

#[test]
fn diff_new_session_counts_as_added_and_status_changed() {
    // 新会话 → added + status_changed（前端立即拿初始灯色）
    let prev = as_map(vec![]);
    let next = as_map(vec![mk("s1", Some("busy"), None)]);
    let c = diff_sessions(&prev, &next);
    assert_eq!(c.added, vec!["s1".to_string()]);
    assert!(c.removed.is_empty());
    assert_eq!(c.status_changed.len(), 1);
    assert_eq!(c.status_changed[0].status.as_deref(), Some("busy"));
}

#[test]
fn diff_status_flip_detected() {
    // busy → idle 翻转检出，且不误报 added/removed
    let prev = as_map(vec![mk("s1", Some("busy"), None)]);
    let next = as_map(vec![mk("s1", Some("idle"), None)]);
    let c = diff_sessions(&prev, &next);
    assert!(c.added.is_empty() && c.removed.is_empty());
    assert_eq!(c.status_changed.len(), 1);
    assert_eq!(c.status_changed[0].status.as_deref(), Some("idle"));
}

/// 同 `mk`，但能指定 pid 与 `procStart` —— P3 刀 0 要的正是这两个字段。
fn mk_id(sid: &str, pid: u32, proc_start: Option<&str>) -> SessionInfo {
    SessionInfo {
        pid,
        session_id: sid.to_string(),
        cwd: "x".into(),
        proc_start: proc_start.map(String::from),
        status: None,
        waiting_for: None,
        name: None,
        kind: None,
    }
}

/// ★★ **P3-Y0：本地也判得出 `Superseded`**（`/branch` / `/clear` 那一格）。
///
/// # 为什么钉 cause 本身，而不是钉它下游的归档决定
///
/// **刀 1 之前**本地 sid 不进 `tmux_raw_registry` ⇒ `find_tmux_origin_for_sid` 恒 `None`
/// ⇒ `classify_removed(None, Gone)` 与 `classify_removed(None, Superseded)`
/// **今天给出同一个结果**（都归档）。
/// ⇒ 测下游**证明不了任何事** —— 把本函数改回全产 `Gone`，那种测试照样绿。
/// 本条因此直接断言 `cause`。
///
/// # 为什么要求正面证据
///
/// `procStart` 缺席时**退回 `Gone`**。pid 会被复用；判错方向的代价不对称 ——
/// 误判 `Superseded` 让一个真死的会话不归档（留个消不掉的条目），
/// 误判 `Gone` 只是回到今天的行为。
#[test]
fn diff_detects_superseded_only_with_positive_identity_evidence() {
    // ① 同一条命（pid + procStart 都相同）换了 sid ⇒ 被顶替
    let prev = as_map(vec![mk_id("old", 42, Some("13300000000000000"))]);
    let next = as_map(vec![mk_id("new", 42, Some("13300000000000000"))]);
    let c = diff_sessions(&prev, &next);
    assert_eq!(
        c.removed,
        vec![RemovedSid::superseded("old")],
        "同 pid + 同 procStart 换 sid 没判成 Superseded ——\n\
             那正是 `/branch` 的形状；判成 Gone 会在本机 tmux 进表之后变成永远消不掉的灰点。"
    );

    // ② procStart 缺席 ⇒ 证据不足 ⇒ Gone（不拿 pid 单独一条就断言同一条命）
    let prev = as_map(vec![mk_id("old", 42, None)]);
    let next = as_map(vec![mk_id("new", 42, None)]);
    assert_eq!(
        diff_sessions(&prev, &next).removed,
        vec![RemovedSid::gone("old")],
        "没有 procStart 也敢判 Superseded —— pid 是会被复用的，\n\
             两个毫无关系的进程会被说成「同一条命换了个 sid」。"
    );

    // ③ pid 相同但 procStart 不同（= pid 被复用）⇒ Gone
    let prev = as_map(vec![mk_id("old", 42, Some("13300000000000000"))]);
    let next = as_map(vec![mk_id("new", 42, Some("13399999999999999"))]);
    assert_eq!(
        diff_sessions(&prev, &next).removed,
        vec![RemovedSid::gone("old")],
        "pid 复用被当成了同一条命 —— procStart 就是用来区分这个的"
    );

    // ④ 真死（那条命在 next 里整个不见了）⇒ Gone
    let prev = as_map(vec![mk_id("old", 42, Some("13300000000000000"))]);
    let next = as_map(vec![]);
    assert_eq!(
        diff_sessions(&prev, &next).removed,
        vec![RemovedSid::gone("old")],
        "会话真的没了却判成 Superseded —— 那会让它永远不归档"
    );
}

#[test]
fn diff_waiting_for_only_change_detected() {
    // status 同为 waiting、仅 waitingFor 变 → 也算变化（tooltip 细分要跟）
    let prev = as_map(vec![mk("s1", Some("waiting"), Some("dialog open"))]);
    let next = as_map(vec![mk("s1", Some("waiting"), Some("permission prompt"))]);
    let c = diff_sessions(&prev, &next);
    assert_eq!(c.status_changed.len(), 1);
    assert_eq!(
        c.status_changed[0].waiting_for.as_deref(),
        Some("permission prompt")
    );
}

#[test]
fn diff_no_change_is_all_empty() {
    // 无变化 → 三个集合全空（watcher 据此不 send，保持稀疏）
    let prev = as_map(vec![mk("s1", Some("busy"), None), mk("s2", None, None)]);
    let next = as_map(vec![mk("s1", Some("busy"), None), mk("s2", None, None)]);
    let c = diff_sessions(&prev, &next);
    assert!(c.added.is_empty() && c.removed.is_empty() && c.status_changed.is_empty());
}

#[test]
fn diff_removed_session_not_in_status_changed() {
    // 消失的会话只进 removed（灯由 session-ended → archiveTab 收尾）
    let prev = as_map(vec![mk("s1", Some("busy"), None)]);
    let next = as_map(vec![]);
    let c = diff_sessions(&prev, &next);
    assert_eq!(c.removed, vec![RemovedSid::gone("s1")]);
    assert!(c.status_changed.is_empty());
}

/// issue #23：waiting 状态带 waitingFor 细分（CLI v2.1.175 实测字段）。
/// 旧版 CC 无 status 字段 → None（parse_session_info_minimal 已覆盖缺省路径）。
#[test]
fn parse_session_info_waiting_with_reason() {
    let raw = r#"{"pid":1,"sessionId":"s","cwd":"x","procStart":"100","status":"waiting","waitingFor":"permission prompt","updatedAt":1781280404074,"statusUpdatedAt":1781280404074}"#;
    let info: SessionInfo = serde_json::from_str(raw).unwrap();
    assert_eq!(info.status.as_deref(), Some("waiting"));
    assert_eq!(info.waiting_for.as_deref(), Some("permission prompt"));
}

#[test]
fn parse_session_info_minimal() {
    // 最小必需字段（无 status / name / startedAt 等）也能解析
    let raw = r#"{"pid":1,"sessionId":"s","cwd":"x","procStart":"100"}"#;
    let info: SessionInfo = serde_json::from_str(raw).unwrap();
    assert_eq!(info.session_id, "s");
    assert_eq!(info.name, None);
}

/// v2.4.2 issue：Claude Code 某些启动路径不写 procStart。
/// 之前 SessionInfo.proc_start: String 必填导致这种 session 被静默忽略
/// → monitor Tab 漏。Option 化后能正常解析，proc_start = None。
#[test]
fn parse_session_info_without_proc_start() {
    let raw = r#"{"pid":22832,"sessionId":"2bb6394f-xx","cwd":"D:\\x"}"#;
    let info: SessionInfo = serde_json::from_str(raw).unwrap();
    assert_eq!(info.pid, 22832);
    assert_eq!(info.session_id, "2bb6394f-xx");
    assert!(info.proc_start.is_none());
}
