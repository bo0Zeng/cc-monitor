//! 订阅号令牌：没过期直接给 · 快过期就对假令牌端点续、加锁整份原子写回 · 两个并发只续一次 · 锁有人持着 / 端点拒 / 盘上已被别人续过都照实处置。
//! 令牌全是现编的假值；目录全在临时目录里。

use super::*;
use std::io::{BufRead, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const FACE: LoginFace = crate::agents::claudecode::LOGIN;
const NOW_MS: u64 = 1_800_000_000_000;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-oauth-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(p.join("acct")).expect("mkdir");
    p
}

fn write_creds(dir: &Path, access: &str, refresh: &str, expires_ms: u64) {
    let doc = serde_json::json!({
        "claudeAiOauth": {
            "accessToken": access,
            "refreshToken": refresh,
            "expiresAt": expires_ms,
            "scopes": ["user:inference", "user:profile"],
            "subscriptionType": "max",
            "rateLimitTier": "tier-x"
        },
        "mcpOAuth": {"keep": "me"}
    });
    std::fs::write(dir.join(FACE.creds_file), doc.to_string()).expect("write creds");
}

fn on_disk(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join(FACE.creds_file)).expect("read"))
        .expect("json")
}

/// 假令牌端点：每收到一发就记下请求体（JSON），按 `answer` 作答；`hook` 在作答前对盘做一件事（模拟 claude 抢先续过）。
fn spawn_token_endpoint(
    answer: &'static str,
    status: u16,
    hook: Option<PathBuf>,
) -> (SocketAddr, Arc<Mutex<Vec<Value>>>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = l.local_addr().expect("addr");
    let got = Arc::new(Mutex::new(Vec::new()));
    let g2 = Arc::clone(&got);
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut r = std::io::BufReader::new(s.try_clone().expect("clone"));
            let mut line = String::new();
            let _ = r.read_line(&mut line);
            let mut clen = 0usize;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    clen = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; clen];
            let _ = r.read_exact(&mut body);
            g2.lock()
                .expect("lock")
                .push(serde_json::from_slice(&body).unwrap_or(Value::Null));
            if let Some(dir) = hook.as_deref() {
                write_creds(dir, "acc-by-claude", "ref-by-claude", NOW_MS + 3_600_000);
            }
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                answer.len()
            );
            let _ = s.write_all(reply.as_bytes());
        }
    });
    (addr, got)
}

fn endpoint_at(addr: SocketAddr) -> TokenEndpoint {
    TokenEndpoint {
        base: Base::parse(&format!("http://127.0.0.1:{}", addr.port())).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_secs(5),
    }
}

/// 没人在听的端点：一旦去续就会失败（没过期那一条要它证明「根本没去续」）。
fn nowhere() -> TokenEndpoint {
    endpoint_at(std::net::SocketAddr::from((
        [127, 0, 0, 1],
        crate::refusing_port::refusing_port(),
    )))
}

fn expose(s: &SecretKey) -> String {
    // 判据里看值只经 `same_secret`：拿已知的假值比。
    for cand in ["acc-old", "acc-new", "acc-by-claude"] {
        if s.same_secret(&SecretKey::new(cand)) {
            return cand.to_string();
        }
    }
    "<其它>".into()
}

const NEW_TOKENS: &str = r#"{"access_token":"acc-new","refresh_token":"ref-new","expires_in":3600,"scope":"user:inference user:profile"}"#;

#[test]
fn a_token_that_is_not_about_to_expire_is_handed_out_without_refreshing() {
    let d = temp_dir("fresh");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS + 3_600_000);
    let got = access_token(&acct, &FACE, &nowhere(), NOW_MS, "q").expect("能用");
    assert_eq!(expose(&got), "acc-old");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_expiring_token_is_refreshed_and_the_whole_file_is_written_back_atomically() {
    let d = temp_dir("refresh");
    let acct = d.join("acct");
    // 还剩 1 分钟（< 5 分钟余量）⇒ 要续。
    write_creds(&acct, "acc-old", "ref-old", NOW_MS + 60_000);
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let got = access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "q").expect("续得上");
    assert_eq!(expose(&got), "acc-new");
    assert_eq!(
        posted.lock().expect("lock").clone(),
        vec![serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": "ref-old",
            "client_id": FACE.client_id,
            "scope": "user:inference user:profile"
        })]
    );
    assert_eq!(
        on_disk(&acct),
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "acc-new",
                "refreshToken": "ref-new",
                "expiresAt": NOW_MS + 3_600_000,
                "scopes": ["user:inference", "user:profile"],
                "subscriptionType": "max",
                "rateLimitTier": "tier-x"
            },
            "mcpOAuth": {"keep": "me"}
        })
    );
    // 只剩那一份：没留临时文件，两把锁都放了。
    assert_eq!(
        guard_core::files_under(&acct),
        vec![FACE.creds_file.to_string()]
    );
    assert!(!acct.join(FACE.lock_inside).exists());
    assert!(!d.join("acct.lock").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(acct.join(FACE.creds_file))
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn two_refreshes_at_once_reach_the_endpoint_once() {
    let d = temp_dir("twice");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let ep = Arc::new(endpoint_at(addr));
    let hs: Vec<_> = (0..2)
        .map(|_| {
            let (acct, ep) = (acct.clone(), Arc::clone(&ep));
            std::thread::spawn(move || {
                access_token(&acct, &FACE, &ep, NOW_MS, "q").map(|s| expose(&s))
            })
        })
        .collect();
    let outs: Vec<_> = hs.into_iter().map(|h| h.join().expect("join")).collect();
    assert_eq!(
        outs,
        vec![Ok("acc-new".to_string()), Ok("acc-new".to_string())]
    );
    assert_eq!(posted.lock().expect("lock").len(), 1, "只续一次");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_lock_held_by_someone_else_is_said_and_left_alone() {
    let d = temp_dir("busy");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    std::fs::create_dir(acct.join(FACE.lock_inside)).expect("别人的锁");
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    assert_eq!(
        access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "q").map(|s| expose(&s)),
        Err(Unusable::Busy)
    );
    assert!(posted.lock().expect("lock").is_empty());
    assert!(acct.join(FACE.lock_inside).is_dir(), "别人的锁不许动");
    assert!(!d.join("acct.lock").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_refused_refresh_says_log_in_again_and_leaves_the_file_as_it_was() {
    let d = temp_dir("refused");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let before = on_disk(&acct);
    let (addr, _) = spawn_token_endpoint(r#"{"error":"invalid_grant"}"#, 400, None);
    let r = access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "q").map(|s| expose(&s));
    assert_eq!(r, Err(Unusable::Refused(400)));
    assert_eq!(on_disk(&acct), before);
    let s = Unusable::Refused(400).said("q");
    let said = s.said;
    assert!(said.contains('q') && !said.contains("ref-old"), "{said}");
    // 端点答的码是原话：不上句子，跟着 raw 进日志。
    assert!(!said.contains("400"), "{said}");
    assert_eq!(s.raw.as_deref(), Some("HTTP 400"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn when_the_agent_refreshed_first_its_tokens_win_and_ours_are_not_written() {
    let d = temp_dir("cas");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, Some(acct.clone()));
    let got = access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "q").expect("用它的");
    assert_eq!(expose(&got), "acc-by-claude");
    assert_eq!(
        on_disk(&acct)["claudeAiOauth"]["refreshToken"],
        "ref-by-claude"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn no_credentials_file_is_not_logged_in() {
    let d = temp_dir("none");
    assert_eq!(
        access_token(&d.join("acct"), &FACE, &nowhere(), NOW_MS, "q").map(|s| expose(&s)),
        Err(Unusable::NotLoggedIn)
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_registered_token_endpoint_parses() {
    let ep = TokenEndpoint::of(&FACE, std::time::Duration::from_secs(1)).expect("端点");
    assert!(ep.base.tls);
    assert_eq!(ep.rest, "/v1/oauth/token");
}

// ── 续期与被拒进日志：按号一行，令牌形状一样都不许进 ──────────────────────────────

/// 跑 `f`，把这段里 `tracing` 打出来的全部文本收回来（只收本线程：续期是同步的）。
/// 本线程上 `f` 期间 `tracing` 说了什么。听法只有一处（`stream::run_route::tests::heard`）：
/// 它进来先重算 callsite 的 interest 缓存 —— 这里原先自己起一个订阅者、不重算，并行的别的测试先碰到同一个
/// callsite 就把它缓存成「没人要」，这条线程上那几行就听不见（`a_stale_lock_directory_…` 负载下间歇红的根因）。
fn logged<T>(f: impl FnOnce() -> T) -> (T, String) {
    let mut out = None;
    let lines = crate::stream::run_route::tests::heard(|| out = Some(f()));
    (out.expect("f 跑过了"), lines.join("\n"))
}

/// 日志里像令牌的东西：`sk-ant` 前缀 · UUID（8-4-4-4-12 位十六进制）· 连着 24 个以上的令牌字符。
fn token_shapes(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    if text.contains("sk-ant") {
        hits.push("sk-ant".to_string());
    }
    let tok =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '+' | '/' | '=');
    for run in text.split(|c: char| !tok(c)).filter(|r| !r.is_empty()) {
        if run.chars().count() >= 24 {
            hits.push(run.to_string());
        }
        let parts: Vec<&str> = run.split('-').collect();
        let lens: Vec<usize> = parts.iter().map(|p| p.len()).collect();
        if parts.len() >= 5
            && lens.windows(5).any(|w| w == [8, 4, 4, 4, 12])
            && parts.iter().all(|p| {
                p.chars()
                    .all(|c| c.is_ascii_hexdigit() || c.is_ascii_alphabetic())
            })
        {
            hits.push(run.to_string());
        }
    }
    hits
}

const UUID: &str = "8c1f3b2a-4d5e-4f60-9a7b-0c1d2e3f4a5b";
const LIVE_ACCESS: &str = "sk-ant-oat01-Qm9vYmFyQmF6UXV4Q29yZ2VHcmF1bHRHYXJwbHk";
const LIVE_REFRESH: &str = "sk-ant-ort01-WmFwRm9vQmFyQmF6UXV4Q29yZ2VHcmF1bHQ";

#[test]
fn the_token_shape_detector_sees_every_shape_it_is_meant_to() {
    assert!(!token_shapes(LIVE_ACCESS).is_empty());
    assert!(!token_shapes(&format!("x {UUID} y")).is_empty());
    assert!(!token_shapes("Qm9vYmFyQmF6UXV4Q29yZ2VHcmF1").is_empty());
    assert!(token_shapes(
        "[auth] 续期：号 b · 败 · 状态码 400 · 错误码 invalid_grant · 原因 refused"
    )
    .is_empty());
}

#[test]
fn every_renewal_is_one_log_line_naming_the_account_outcome_code_and_status() {
    // 成
    let d = temp_dir("log-ok");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let (r, log) = logged(|| access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b"));
    assert!(r.is_ok());
    let lines: Vec<&str> = log.lines().filter(|l| l.contains("[auth] 续期")).collect();
    assert_eq!(lines.len(), 1, "{log}");
    assert!(
        lines[0].contains("[auth] 续期：号 b · 成 · 状态码 200 · 错误码 — · 原因 —"),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&d);

    // 败：令牌端点不收（invalid_grant）
    let d = temp_dir("log-refused");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (addr, _) = spawn_token_endpoint(
        r#"{"error":"invalid_grant","error_description":"Refresh token not found or invalid"}"#,
        400,
        None,
    );
    let (r, log) = logged(|| access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b"));
    assert_eq!(r.map(|s| expose(&s)), Err(Unusable::Refused(400)));
    assert!(
        log.contains("[auth] 续期：号 b · 败 · 状态码 400 · 错误码 invalid_grant · 原因 refused"),
        "{log}"
    );
    assert!(
        !log.contains("Refresh token not found"),
        "error_description 不进日志：{log}"
    );
    let _ = std::fs::remove_dir_all(&d);

    // 败：连不上（没有状态码）
    let d = temp_dir("log-down");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (_, log) = logged(|| access_token(&acct, &FACE, &nowhere(), NOW_MS, "b"));
    assert!(
        log.contains("[auth] 续期：号 b · 败 · 状态码 — · 错误码 —"),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&d);

    // 让位：发出去的刷新令牌在盘上已被那个号自己的 claude 换掉
    let d = temp_dir("log-cas");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, Some(acct.clone()));
    let (_, log) = logged(|| access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b"));
    assert!(
        log.contains("[auth] 续期：号 b · 让位 · 状态码 200"),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&d);

    // 没去续 ⇒ 不记
    let d = temp_dir("log-fresh");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS + 3_600_000);
    let (_, log) = logged(|| access_token(&acct, &FACE, &nowhere(), NOW_MS, "b"));
    assert!(!log.contains("[auth]"), "{log}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn no_token_refresh_token_or_account_uuid_reaches_the_renewal_log() {
    let d = temp_dir("log-shapes");
    let acct = d.join("acct");
    write_creds(&acct, LIVE_ACCESS, LIVE_REFRESH, NOW_MS - 1);
    // 端点把刷新令牌与账号 UUID 回显进 `error` 与 `error_description`（最坏的那一种回包）。
    let echo: &'static str = Box::leak(
        format!(r#"{{"error":"{LIVE_REFRESH}","error_description":"token {LIVE_REFRESH} of {UUID}","account":{{"uuid":"{UUID}"}}}}"#)
            .into_boxed_str(),
    );
    let (addr, _) = spawn_token_endpoint(echo, 401, None);
    let (_, log) = logged(|| access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b"));
    assert!(log.contains("[auth] 续期：号 b · 败 · 状态码 401"), "{log}");
    assert_eq!(token_shapes(&log), Vec::<String>::new(), "{log}");
    let _ = std::fs::remove_dir_all(&d);

    // 成的那一发：回包里是真形状的新令牌。
    let d = temp_dir("log-shapes-ok");
    let acct = d.join("acct");
    write_creds(&acct, LIVE_ACCESS, LIVE_REFRESH, NOW_MS - 1);
    let fresh: &'static str = Box::leak(
        format!(r#"{{"access_token":"{LIVE_ACCESS}x","refresh_token":"{LIVE_REFRESH}y","expires_in":3600,"account":{{"uuid":"{UUID}"}}}}"#)
            .into_boxed_str(),
    );
    let (addr, _) = spawn_token_endpoint(fresh, 200, None);
    let (_, log) = logged(|| access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b"));
    assert!(log.contains("[auth] 续期：号 b · 成"), "{log}");
    assert_eq!(token_shapes(&log), Vec::<String>::new(), "{log}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_upstream_401_or_403_is_one_line_per_account_and_nothing_else_is() {
    use crate::accounts::upstream_select::auth_refusal_line;
    assert_eq!(
        auth_refusal_line("b", 401).as_deref(),
        Some("[auth] 被拒：号 b · 状态码 401 · 令牌不被上游认")
    );
    assert_eq!(
        auth_refusal_line("b", 403).as_deref(),
        Some("[auth] 被拒：号 b · 状态码 403 · 令牌不被上游认")
    );
    for s in [200, 400, 404, 429, 500, 529, 0] {
        assert_eq!(auth_refusal_line("b", s), None, "{s}");
    }
    assert!(token_shapes(&auth_refusal_line("b", 401).unwrap_or_default()).is_empty());
}

// ── 续期锁：持锁中途出事不留锁 · 过期的锁目录当无主 · 新鲜的照旧挡 ─────────────────────

/// 把一个锁目录的修改时刻拨回 `ago`（模拟持有方早就死了）。
fn age_dir(p: &Path, ago: std::time::Duration) {
    let f = std::fs::File::open(p).expect("开锁目录");
    f.set_modified(std::time::SystemTime::now() - ago)
        .expect("拨修改时刻");
}

#[test]
fn a_panic_while_holding_the_refresh_lock_leaves_no_lock_behind() {
    let d = temp_dir("panic");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = store::with_refresh_lock(&acct, &FACE, |_| -> () {
            panic!("续期那一趟里出事")
        });
    }));
    assert!(r.is_err(), "panic 该传出来");
    assert!(!acct.join(FACE.lock_inside).exists(), "里面那把留下了");
    assert!(!d.join("acct.lock").exists(), "旁边那把留下了");
    let _ = std::fs::remove_dir_all(&d);
}

/// 听日志那一手（`logged`）不怕「别的线程先碰到同一个 callsite」：并行的测试线程上没有订阅者，它先碰到
/// 续期那几行，interest 就被按它的默认缓存成「没人要」；本线程随后再听就听不见（`a_stale_lock_directory_…` 负载下间歇红的那一形）。
/// 这里把那一形做成确定的：另起一条没有订阅者的线程先续一次，再在 `logged` 里续一次。
#[test]
fn logging_is_heard_even_when_another_thread_touched_the_same_lines_first() {
    let other = temp_dir("heard-other");
    let other_acct = other.join("acct");
    write_creds(&other_acct, "acc-old", "ref-old", NOW_MS - 1);
    let (other_addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let d = temp_dir("heard");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let (r, log) = logged(|| {
        std::thread::spawn(move || {
            let _ = access_token(&other_acct, &FACE, &endpoint_at(other_addr), NOW_MS, "b");
        })
        .join()
        .expect("另一条线程");
        access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b")
    });
    assert_eq!(r.map(|s| expose(&s)), Ok("acc-new".to_string()), "{log}");
    assert!(log.contains("[auth] 续期：号 b · 成"), "{log}");
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&other);
}

#[test]
fn a_stale_lock_directory_no_longer_blocks_renewal_and_the_reclaim_is_logged() {
    let d = temp_dir("stale");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let inside = acct.join(FACE.lock_inside);
    let beside = d.join("acct.lock");
    std::fs::create_dir(&inside).expect("死掉的持有方留下的锁");
    std::fs::create_dir(&beside).expect("死掉的持有方留下的锁");
    let ago = std::time::Duration::from_millis(FACE.lock_stale_ms + 30_000);
    age_dir(&inside, ago);
    age_dir(&beside, ago);
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let (r, log) = logged(|| access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b"));
    assert_eq!(r.map(|s| expose(&s)), Ok("acc-new".to_string()), "{log}");
    assert_eq!(posted.lock().expect("lock").len(), 1);
    assert!(!inside.exists() && !beside.exists(), "续完两把都放掉");
    assert!(log.contains("[auth] 续期：号 b · 成"), "{log}");
    assert!(log.contains("收回过期锁 里面那把、旁边那把"), "{log}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_fresh_lock_directory_still_says_busy() {
    let d = temp_dir("fresh-lock");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let inside = acct.join(FACE.lock_inside);
    std::fs::create_dir(&inside).expect("别人的锁");
    // 比门限新一点点：活着的持有方
    age_dir(
        &inside,
        std::time::Duration::from_millis(FACE.lock_stale_ms.saturating_sub(5_000)),
    );
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    assert_eq!(
        access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS, "b").map(|s| expose(&s)),
        Err(Unusable::Busy)
    );
    assert!(posted.lock().expect("lock").is_empty());
    assert!(inside.is_dir(), "活着的锁不许动");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_said_line_keeps_the_raw_beside_it_not_in_it() {
    // 读不出来 / 续不上 / 写不回那三形：句子只带原因词，下层原话跟在旁边（换号那一路记日志时拼上），不进句子。
    let raw = || crate::common::said::Said::with_raw("x".to_string(), "os error 13: RAW-TEXT");
    for u in [
        Unusable::Unreadable(raw()),
        Unusable::NotRenewed(raw()),
        Unusable::WriteFailed(raw()),
    ] {
        let s = u.said("q");
        assert!(
            s.said.contains('q') && !s.said.contains("RAW-TEXT"),
            "{s:?}"
        );
        assert_eq!(s.raw.as_deref(), Some("os error 13: RAW-TEXT"), "{u:?}");
    }
}
