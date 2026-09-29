//! # 要求住址：`设计/70 §6.3`（日志那一块：三个开关 ＋ 文件位置）＋ `INVARIANTS §2.1`（`config.json` 是真相）
//!
//! 核原文：`设计/70 §6.3` 逐字「日志文件开关（改了要重启）· 级别（立即生效）· 错误提示开关 · 文件位置 ＋ 大小」
//! —— 本族判诊断配置的默认值、`logging.rs::build_env_filter` 认哪些级别、`logging.rs::find_latest_log_file` 指到哪一份；
//! 写回 `diagnostics` 不丢同文件别的字段，对 `INVARIANTS §2.1` 那张表里 `config.json` 的「真相 · 全用户手填」。
//! 〔JA1 点址 2026-09-24〕〔TL1 · 4C〕`diagnostics_legacy_config_missing_field_uses_defaults` 从前在测试里逐字重写了
//! `logging.rs::read_diagnostics_from_config` 那条链、不在执行链上；今天改调生产函数，且「缺 `diagnostics` 键」那一支只有它量（`TL1.md` 件 1）。

use super::*;

#[test]
fn diagnostics_default_is_user_friendly() {
    let d = DiagnosticsConfig::default();
    assert!(d.log_enabled, "log 默认开（issue #4 要求）");
    assert!(
        d.error_toast,
        "error toast 默认开（用户看不见 ERROR 是 v1.7 事故根因）"
    );
    assert_eq!(d.log_level, "info");
    assert_eq!(d.max_files, 3);
}

/// 一份没有 `diagnostics` 键的 `config.json`（别的设置都在）⇒ 诊断设置取默认值。
///
/// 〔TL1 · 4C〕从前这里把 `read_diagnostics_from_config` 那条链在测试里逐字重写了一遍、没调生产函数（`JA1.md §3.3`）；
/// 今天经临时目录真读一份文件。「缺这个键」那一支只有本条走：`write_then_read_diagnostics_roundtrip` 走的是「键在」那一支。
#[test]
fn diagnostics_legacy_config_missing_field_uses_defaults() {
    let tmp = std::env::temp_dir().join(format!("ccm-log-test-legacy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(
        tmp.join("config.json"),
        r#"{"claudeDir":"/home/u/.claude","theme":{}}"#,
    )
    .unwrap();
    assert_eq!(
        read_diagnostics_from_config(&tmp),
        DiagnosticsConfig::default()
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn diagnostics_partial_field_serde_default_other() {
    // 只有 log_level 一个字段的 config 也能 deserialize（其他字段拿 default）
    let raw = r#"{"log_level":"debug"}"#;
    let d: DiagnosticsConfig = serde_json::from_str(raw).unwrap();
    assert_eq!(d.log_level, "debug");
    assert!(d.log_enabled);
    assert!(d.error_toast);
    assert_eq!(d.max_files, 3);
}

#[test]
fn build_env_filter_accepts_valid_levels() {
    for lv in ["trace", "debug", "info", "warn", "error", "off"] {
        assert!(build_env_filter(lv).is_some(), "level {lv} should parse");
    }
}

#[test]
fn build_env_filter_rejects_garbage() {
    assert!(build_env_filter("nonsense=42=42=").is_none());
}

#[test]
fn write_then_read_diagnostics_roundtrip() {
    let tmp = std::env::temp_dir().join(format!("ccm-log-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let cfg = DiagnosticsConfig {
        log_enabled: true,
        log_level: "warn".to_string(),
        error_toast: false,
        max_files: 7,
    };
    write_diagnostics_to_config(&tmp, &cfg).unwrap();

    let back = read_diagnostics_from_config(&tmp);
    assert_eq!(back, cfg);

    // 不破坏 config.json 已有字段（手写 theme/claudeDir 后再写 diagnostics）
    let cfg_path = tmp.join("config.json");
    let raw = std::fs::read_to_string(&cfg_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(v.get("diagnostics").is_some());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn write_diagnostics_preserves_other_fields() {
    let tmp = std::env::temp_dir().join(format!("ccm-log-test2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // 先写 config.json 含 theme + claudeDir（模拟老用户）。
    // 用 r##"..."## 加一层 `#` 是因为 JSON 内含 "#000000"，普通 r#"..."# 会被
    // "# 提前终止（raw string 的结束分隔符是 `"` 后跟匹配数量的 `#`）
    let original = r##"{"claudeDir":"C:\\foo","theme":{"bg":"#000000"}}"##;
    std::fs::write(tmp.join("config.json"), original).unwrap();

    // 再写 diagnostics
    write_diagnostics_to_config(&tmp, &DiagnosticsConfig::default()).unwrap();

    // 老字段必须还在
    let raw = std::fs::read_to_string(tmp.join("config.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v.get("claudeDir").and_then(|x| x.as_str()), Some("C:\\foo"));
    assert!(v.get("theme").is_some());
    assert!(v.get("diagnostics").is_some());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn find_latest_log_picks_newest_mtime() {
    let tmp = std::env::temp_dir().join(format!("ccm-log-test3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    std::fs::write(tmp.join("monitor.2026-01-01.log"), "old").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(tmp.join("monitor.2026-05-25.log"), "new").unwrap();
    // 非 log 文件不应被选中
    std::fs::write(tmp.join("notes.txt"), "noise").unwrap();

    let latest = find_latest_log_file(&tmp).unwrap();
    assert!(latest.to_string_lossy().contains("monitor.2026-05-25"));

    let _ = std::fs::remove_dir_all(&tmp);
}

/// 〔S5 · 第四波〕要求住址：D4（不许把「读不出」静默当成空）· D7（失败要说清原因）——
/// 主会话转来的 JA1 读数逐字「遇坏 config.json 会退成 `{}` 再整份写回，用户手填的内容被覆盖、没有判据守」。
///
/// 一份读不懂的 config.json（少一个逗号）⇒ `Err`、**盘上字节一个不动**、话里点名那份文件并说为什么没存。
/// 对照：合法的那份照常写（上面 `write_diagnostics_preserves_other_fields`）。
#[test]
fn a_config_we_cannot_parse_is_left_alone_not_overwritten() {
    // 〔NT2〕先前与 `find_latest_log_picks_newest_mtime` 共用 `ccm-log-test3-<pid>` 这一个目录 ⇒ 两条并行时互删对方的文件（时好时坏）。
    let tmp = std::env::temp_dir().join(format!("ccm-log-test3b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let broken = "{\"claudeDir\":\"/x\" \"theme\":{}}"; // 少一个逗号
    let cfg_path = tmp.join("config.json");
    std::fs::write(&cfg_path, broken).unwrap();

    let err = write_diagnostics_to_config(&tmp, &DiagnosticsConfig::default())
        .expect_err("读不懂的 config.json 被当成空的写回去了");
    assert_eq!(
        std::fs::read_to_string(&cfg_path).unwrap(),
        broken,
        "读不懂的那份被改了 —— 用户手填的内容没了"
    );
    // 〔CFG1〕临时件名带 pid 了（`config.json.<pid>.tmp`，写口 `config::patch_config_at`）。
    assert!(
        !tmp.join(format!("config.json.{}.tmp", std::process::id()))
            .exists(),
        "临时文件都写出来了 —— 「不写」要在写之前就停"
    );
    assert!(
        err.contains("config.json") && err.contains("读不懂") && err.contains("没有存"),
        "那句话没说清是哪份文件、为什么没存：{err}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// ═══ 〔NT2 · S1〕本机后端（脱离那条载体）的 stderr 诊断文件 ═══════════════════════════════════════
//
// 守的要求（住址，纪律 19）：`设计/15 §4.7 S1`（逐字）「**本机 · 脱离常驻载体**（Linux 缺省；全部 SSH 与中转都在它里面）|
// null（`StderrSink::Null`）| **仍开**」· 主会话 4C 第二批裁（逐字）「脱离载体的常驻后端 stderr 落本机日志文件（有上限、滚动），
// 设置页『日志』里看得到」。设计：`调研/第四波记录/NT2.md §2`。

/// L4 ★ 设置页读到的那一族：后端那个子目录里的普通文件，新在前；目录不在 ⇒ 空（不报错）。
/// 另一向：日志目录顶层只收 `.log`，后端那个子目录不混进 monitor 自己的「当前文件」。
#[test]
fn the_backend_stderr_files_are_listed_newest_first_and_kept_apart_from_ours() {
    let root = std::env::temp_dir().join(format!("nt2-logging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cur = backend_stderr_log_path(&root);
    let dir = cur.parent().unwrap().to_path_buf();
    assert!(
        list_log_entries(&dir, None).is_empty(),
        "目录不在时应当是空"
    );
    std::fs::create_dir_all(&dir).unwrap();
    let old = dir.join("stderr.old.log");
    std::fs::write(&old, b"old").unwrap();
    std::fs::write(&cur, b"current!").unwrap();
    // 让 mtime 分得开：把旧的那份的 mtime 拨回去一小时。
    let f = std::fs::File::options().write(true).open(&old).unwrap();
    f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
        .unwrap();
    drop(f);
    let got: Vec<(String, u64)> = list_log_entries(&dir, None)
        .into_iter()
        .map(|e| (e.path, e.size_bytes))
        .collect();
    assert_eq!(
        got,
        vec![
            (cur.to_string_lossy().into_owned(), 8),
            (old.to_string_lossy().into_owned(), 3)
        ]
    );
    // 顶层按 `.log` 收：子目录不算一份文件。
    std::fs::write(root.join("logs").join("monitor.2026-09-25.log"), b"m").unwrap();
    let top = list_log_entries(&root.join("logs"), Some(LOG_FILE_SUFFIX));
    assert_eq!(top.len(), 1, "顶层混进了后端那一族：{top:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// 交给后端的那个变量名 == 后端读的那个（异源：从后端源码里现抠 `pub const ENV`）；路径就是设置页读的那一份（同一个函数）。
/// 接线（文本，如实登记：按行为量要真起脱离后端）：`spawn_detached` 交它恰好一处，被监护那条的 `relay_host_envs` 不交。
#[test]
fn the_detached_backend_is_handed_its_stderr_log_path_and_only_that_carrier_is() {
    let backend = include_str!("../../../src/backend/stderr_log.rs");
    guard_core::pin_line(backend, "pub const ENV: &str = \"CCM_BACKEND_STDERR_LOG\";")
        .unwrap_or_else(|e| panic!("后端读的变量名变了（或不是恰好一处）：{e}"));
    assert_eq!(BACKEND_STDERR_LOG_ENV, "CCM_BACKEND_STDERR_LOG");
    let host = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    assert_eq!(
        host.matches("crate::logging::BACKEND_STDERR_LOG_ENV")
            .count(),
        1,
        "交这一格不是恰好一处"
    );
    let at = guard_core::find_pinned(
        &host,
        "fn spawn_detached(\n    bin: &std::path::Path,\n    port: u16,",
    )
    .unwrap_or_else(|e| panic!("切不出 Linux 那条 spawn_detached：{e}"));
    let (body, _) = host[at..].split_once("\n}\n").expect("切不出函数体");
    assert!(
        body.contains("crate::logging::BACKEND_STDERR_LOG_ENV")
            && body.contains("crate::logging::backend_stderr_log_path(&d)"),
        "脱离那条载体没交诊断文件路径（或路径不是那一个函数算的）"
    );
    assert!(
        !crate::local_backend_host::relay_host_envs()
            .iter()
            .any(|(k, _)| k == BACKEND_STDERR_LOG_ENV),
        "两条载体共用那份环境里也交了 —— 被监护那条的 stderr 会分成两处"
    );
}

/// L4 ★ 设置页真读的那一口（`LoggingState::log_file_info`）：后端那份文件在 ⇒ `backend_stderr` 里有它、且不混进 monitor 自己的
/// `current_file`；那层目录不在 ⇒ 空（另一向）。状态由本测试就地造（不装全局 tracing）。
#[test]
fn log_file_info_reports_the_backend_stderr_file_from_the_same_path_it_was_handed() {
    let root = std::env::temp_dir().join(format!("nt2-logging-info-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (_layer, reload_handle) = reload::Layer::new(EnvFilter::new("info"));
    let state = LoggingState {
        cfg: RwLock::new(DiagnosticsConfig::default()),
        reload_handle,
        log_dir: root.join(LOG_DIR_NAME),
        monitor_data_dir: root.clone(),
        error_emit_fn: Arc::new(RwLock::new(None)),
        error_emit_enabled: Arc::new(AtomicBool::new(false)),
        _guard: Mutex::new(None),
    };
    assert!(
        state.log_file_info().backend_stderr.is_empty(),
        "目录不在时应当是空"
    );
    let cur = backend_stderr_log_path(&root);
    std::fs::create_dir_all(cur.parent().unwrap()).unwrap();
    std::fs::write(&cur, b"backend said").unwrap();
    let info = state.log_file_info();
    assert_eq!(
        info.backend_stderr
            .iter()
            .map(|e| (e.path.clone(), e.size_bytes))
            .collect::<Vec<_>>(),
        vec![(cur.to_string_lossy().into_owned(), 12)]
    );
    assert_eq!(
        info.current_file, None,
        "后端那一份混进了 monitor 自己的当前文件"
    );
    let _ = std::fs::remove_dir_all(&root);
}
