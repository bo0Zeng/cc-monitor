//! 订阅号令牌：没过期直接给 · 快过期就对假令牌端点续、加锁整份原子写回 · 两个并发只续一次 · 锁有人持着 / 端点拒 / 盘上已被别人续过都照实处置。
//! 令牌全是现编的假值；目录全在临时目录里。

use super::*;
use std::io::{BufRead, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const FACE: LoginFace = crate::agents::claudecode::LOGIN;
/// Claude 那一行配置目录里那把续期锁的名字。
const INSIDE: &str = match FACE.lock_inside {
    Some(n) => n,
    None => panic!("Claude 那一行声明了续期锁"),
};
const NOW_MS: u64 = 1_800_000_000_000;
fn never(_: &std::path::Path) -> bool {
    false
}
/// 有会话在用也续的那一形（原来那一形；`Idle` 那一形只在自己那一条判据里）。
const ANYTIME: When = When {
    renew: crate::agents::Renew::Anytime,
    in_use: never,
    base: false,
};

fn temp_dir(tag: &str) -> PathBuf {
    let p = test_home::scratch(tag);
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
    serve_tokens(answer, status, move || {
        if let Some(dir) = hook.as_deref() {
            write_creds(dir, "acc-by-claude", "ref-by-claude", NOW_MS + 3_600_000);
        }
    })
}

/// 假令牌端点本体：每收到一发记下请求体，作答前先跑 `on_request`（看一眼 · 动一下盘）。
fn serve_tokens(
    answer: &'static str,
    status: u16,
    on_request: impl Fn() + Send + 'static,
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
            on_request();
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
        base: crate::relay::base_of(&format!("http://127.0.0.1:{}", addr.port())).expect("base"),
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

/// 写句柄（测试进程自己的临时根；订阅号凭据的窄句柄从它造，落点是各判据自己的号目录）。
fn own() -> crate::platform::fs::Own {
    crate::platform::fs::Own::for_tests()
}

#[test]
fn a_token_that_is_not_about_to_expire_is_handed_out_without_refreshing() {
    let d = temp_dir("fresh");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS + 3_600_000);
    let got =
        access_token(&own(), &acct, &FACE, Some(&nowhere()), NOW_MS, "q", ANYTIME).expect("能用");
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
    let got = access_token(
        &own(),
        &acct,
        &FACE,
        Some(&endpoint_at(addr)),
        NOW_MS,
        "q",
        ANYTIME,
    )
    .expect("续得上");
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
    assert!(!acct.join(INSIDE).exists());
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
                access_token(&own(), &acct, &FACE, Some(&*ep), NOW_MS, "q", ANYTIME)
                    .map(|s| expose(&s))
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
    std::fs::create_dir(acct.join(INSIDE)).expect("别人的锁");
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    assert_eq!(
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "q",
            ANYTIME
        )
        .map(|s| expose(&s)),
        Err(Unusable::Busy)
    );
    assert!(posted.lock().expect("lock").is_empty());
    assert!(acct.join(INSIDE).is_dir(), "别人的锁不许动");
    assert!(!d.join("acct.lock").exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// 等写锁放开的期限缩短到判据等得起的长度（生产里是那一家登记的那个数）。
const SHORT_WAIT: LoginFace = LoginFace {
    write_wait_ms: 400,
    ..FACE
};

/// 盘上那一份的订阅号令牌是不是 `access` / `refresh`（经 `same_secret` 不读明文以外的东西：夹具全是假值）。
fn disk_has(acct: &Path, access: &str, refresh: &str) -> bool {
    let v = on_disk(acct);
    v["claudeAiOauth"]["accessToken"] == access && v["claudeAiOauth"]["refreshToken"] == refresh
}

fn wait_until(mut ok: impl FnMut() -> bool) -> bool {
    for _ in guard_core::wall::rounds() {
        if ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    ok()
}

/// ★ 续成了、写回时 claude 正持着它的整份写锁：这一发照常拿到新令牌（不被挡）；期间再来的不再续一次、照用内存里那份；
/// 锁一放（文件事件）就照「重读 → 比对 → 只换这一节 → 整份原子写」写回，别的节原样。
#[test]
fn a_write_lock_held_by_claude_does_not_cost_the_new_token() {
    let d = temp_dir("wlock");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let wl = acct.join(FACE.write_lock.expect("Claude 登记了整份写锁").name());
    std::fs::create_dir(&wl).expect("别人的写锁");
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let ep = endpoint_at(addr);
    let r = access_token(&own(), &acct, &FACE, Some(&ep), NOW_MS, "q", ANYTIME).map(|s| expose(&s));
    assert_eq!(r, Ok("acc-new".to_string()), "写锁在，这一发被挡了");
    assert!(disk_has(&acct, "acc-old", "ref-old"), "写锁在，还是写了");
    assert!(wl.is_dir(), "别人的写锁不许动");
    let again =
        access_token(&own(), &acct, &FACE, Some(&ep), NOW_MS, "q", ANYTIME).map(|s| expose(&s));
    assert_eq!(
        again,
        Ok("acc-new".to_string()),
        "等写回期间又来一发，没用内存里那份"
    );
    assert_eq!(
        posted.lock().expect("lock").len(),
        1,
        "等写回期间又续了一次"
    );
    std::fs::remove_dir(&wl).expect("放锁");
    assert!(
        wait_until(|| disk_has(&acct, "acc-new", "ref-new")),
        "锁放了，盘上还不是新令牌"
    );
    assert_eq!(
        on_disk(&acct)["mcpOAuth"],
        serde_json::json!({"keep": "me"}),
        "别的节变了"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 到期限写锁还没放开 ⇒ 照实报写回失败（点名这个号、说可能要重新登录），盘上一个字节不动。
#[test]
fn a_write_lock_that_never_lets_go_is_reported_as_a_failed_write_back() {
    let d = temp_dir("wlock-stuck");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let before = on_disk(&acct);
    let wl = acct.join(FACE.write_lock.expect("Claude 登记了整份写锁").name());
    std::fs::create_dir(&wl).expect("别人的写锁");
    let (addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let ep = endpoint_at(addr);
    let r = access_token(&own(), &acct, &SHORT_WAIT, Some(&ep), NOW_MS, "q", ANYTIME)
        .map(|s| expose(&s));
    assert_eq!(r, Ok("acc-new".to_string()));
    let mut said = None;
    assert!(
        wait_until(|| {
            match access_token(
                &own(),
                &acct,
                &SHORT_WAIT,
                Some(&nowhere()),
                NOW_MS,
                "q",
                ANYTIME,
            ) {
                Err(Unusable::WriteFailed(why)) => {
                    said = Some(Unusable::WriteFailed(why).said("q").said);
                    true
                }
                _ => false,
            }
        }),
        "期限过了也没报写回失败"
    );
    let said = said.unwrap_or_default();
    assert!(said.contains('q') && said.contains("重新登录"), "{said}");
    assert_eq!(on_disk(&acct), before, "写锁一直在，还是写了");
    assert!(wl.is_dir(), "别人的写锁不许动");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_refused_refresh_says_log_in_again_and_leaves_the_file_as_it_was() {
    let d = temp_dir("refused");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let before = on_disk(&acct);
    let (addr, _) = spawn_token_endpoint(r#"{"error":"invalid_grant"}"#, 400, None);
    let r = access_token(
        &own(),
        &acct,
        &FACE,
        Some(&endpoint_at(addr)),
        NOW_MS,
        "q",
        ANYTIME,
    )
    .map(|s| expose(&s));
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
    let got = access_token(
        &own(),
        &acct,
        &FACE,
        Some(&endpoint_at(addr)),
        NOW_MS,
        "q",
        ANYTIME,
    )
    .expect("用它的");
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
        access_token(
            &own(),
            &d.join("acct"),
            &FACE,
            Some(&nowhere()),
            NOW_MS,
            "q",
            ANYTIME
        )
        .map(|s| expose(&s)),
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
/// 本线程上 `f` 期间 `tracing` 说了什么。听法只有一处（量具包的 `test_meters::heard`）：
/// 它进来先重算 callsite 的 interest 缓存 —— 这里原先自己起一个订阅者、不重算，并行的别的测试先碰到同一个
/// callsite 就把它缓存成「没人要」，这条线程上那几行就听不见（`a_stale_lock_directory_…` 负载下间歇红的根因）。
fn logged<T>(f: impl FnOnce() -> T) -> (T, String) {
    let mut out = None;
    let lines = test_meters::heard(|| out = Some(f()));
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
    let (r, log) = logged(|| {
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME,
        )
    });
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
    let (r, log) = logged(|| {
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME,
        )
    });
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

    // 败：断开（没有状态码）
    let d = temp_dir("log-down");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (_, log) =
        logged(|| access_token(&own(), &acct, &FACE, Some(&nowhere()), NOW_MS, "b", ANYTIME));
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
    let (_, log) = logged(|| {
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME,
        )
    });
    assert!(
        log.contains("[auth] 续期：号 b · 让位 · 状态码 200"),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&d);

    // 没去续 ⇒ 不记
    let d = temp_dir("log-fresh");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS + 3_600_000);
    let (_, log) =
        logged(|| access_token(&own(), &acct, &FACE, Some(&nowhere()), NOW_MS, "b", ANYTIME));
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
    let (_, log) = logged(|| {
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME,
        )
    });
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
    let (_, log) = logged(|| {
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME,
        )
    });
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
        let _ = store::with_refresh_lock(
            &store::creds_of(&own(), &acct, &FACE).expect("窄句柄"),
            &FACE,
            |_, _| -> () { panic!("续期那一趟里出事") },
        );
    }));
    assert!(r.is_err(), "panic 该传出来");
    assert!(!acct.join(INSIDE).exists(), "里面那把留下了");
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
            let _ = access_token(
                &own(),
                &other_acct,
                &FACE,
                Some(&endpoint_at(other_addr)),
                NOW_MS,
                "b",
                ANYTIME,
            );
        })
        .join()
        .expect("另一条线程");
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME,
        )
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
    let inside = acct.join(INSIDE);
    let beside = d.join("acct.lock");
    std::fs::create_dir(&inside).expect("死掉的持有方留下的锁");
    std::fs::create_dir(&beside).expect("死掉的持有方留下的锁");
    let ago = std::time::Duration::from_millis(FACE.lock_stale_ms + 30_000);
    age_dir(&inside, ago);
    age_dir(&beside, ago);
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let (r, log) = logged(|| {
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME,
        )
    });
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
    let inside = acct.join(INSIDE);
    std::fs::create_dir(&inside).expect("别人的锁");
    // 比门限新一点点：活着的持有方
    age_dir(
        &inside,
        std::time::Duration::from_millis(FACE.lock_stale_ms.saturating_sub(5_000)),
    );
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    assert_eq!(
        access_token(
            &own(),
            &acct,
            &FACE,
            Some(&endpoint_at(addr)),
            NOW_MS,
            "b",
            ANYTIME
        )
        .map(|s| expose(&s)),
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

/// 只在没有活会话用这个号时才续的那一家（`Renew::Idle`）：快过期而此刻有会话在用 ⇒ 不续、交 `Busy`（留给那一家自己续），
/// 令牌端点一发都不打；没人在用 ⇒ 照常续。
#[test]
fn an_idle_only_family_leaves_renewal_to_itself_while_a_session_uses_the_account() {
    let d = temp_dir("idle");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS + 60_000);
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let busy = When {
        renew: crate::agents::Renew::Idle,
        in_use: |_| true,
        base: false,
    };
    let r = access_token(
        &own(),
        &acct,
        &FACE,
        Some(&endpoint_at(addr)),
        NOW_MS,
        "q",
        busy,
    );
    assert!(matches!(r, Err(Unusable::Busy)), "有会话在用 ⇒ 交 Busy");
    assert!(posted.lock().expect("lock").is_empty(), "一发都不打");
    let idle = When {
        renew: crate::agents::Renew::Idle,
        in_use: |_| false,
        base: false,
    };
    let got = access_token(
        &own(),
        &acct,
        &FACE,
        Some(&endpoint_at(addr)),
        NOW_MS,
        "q",
        idle,
    )
    .expect("没人在用 ⇒ 照常续");
    assert_eq!(expose(&got), "acc-new");
    let _ = std::fs::remove_dir_all(&d);
}

// ───────────── 假 agent 那一行：每一格都与 Claude 不同形，核心照这几格续 ─────────────

/// 假 agent 的登录面（过期时刻交那一家读 · 出处记在凭据里 · 整份写锁是文件锁 · 续成另记顶层一格）。
fn fake_face() -> LoginFace {
    crate::agents::fake::full_row(|| None)
        .accounts
        .and_then(|f| f.login)
        .expect("假 agent 声明了登录面")
}

/// 假 agent 那一形的凭据：`stamped` 是上次续的那一秒（过期 ＝ 它 ＋ 一小时）；`issuer` 缺 ⇒ 没记出处。
fn fake_creds(issuer: Option<&str>, stamped_s: u64) -> Value {
    let mut session = serde_json::json!({
        "bearer": "acc-old",
        "renewal": "ref-old",
        "app": "from-origin",
        "grants": ["a", "b"],
        "extra": "keep"
    });
    if let Some(i) = issuer {
        session["issuer"] = Value::from(i);
    }
    serde_json::json!({"session": session, "stamped": stamped_s, "other": {"keep": [1, 2]}})
}

/// 号目录里此刻有的几样（名字，排好序；目录 · 文件都算）。
fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = crate::platform::fs::Ro::read_dir(dir)
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn fake_disk(acct: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(acct.join(fake_face().creds_file)).expect("read"))
        .expect("json")
}

/// 两小时前续过 ⇒ 已过期。
const STALE_S: u64 = NOW_MS / 1000 - 7_200;

/// ★ 凭据里记了出处 ⇒ 续期发到出处那个地址、用出处的客户端 id（调用方给的缺省端点没人听）；
/// 写回整份原样，只换那几格令牌 ＋ 那一家要记的「几点续的」，过期时刻交那一家读的那一形不另写一格；续完下一发照新的时刻不再续。
/// 那一家不记授权范围 ⇒ 请求体里没有 `scope`，回包里的 `scope` 不写进凭据（盘上那一格原样）。
#[test]
fn a_recorded_origin_is_where_renewal_goes_and_only_the_token_cells_change() {
    let face = fake_face();
    let d = temp_dir("origin");
    let acct = d.join("acct");
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    std::fs::write(
        acct.join(face.creds_file),
        fake_creds(Some(&issuer), STALE_S).to_string(),
    )
    .expect("write");
    let got = access_token(&own(), &acct, &face, Some(&nowhere()), NOW_MS, "f", ANYTIME)
        .expect("照出处续得上");
    assert_eq!(expose(&got), "acc-new");
    assert_eq!(
        posted.lock().expect("lock").clone(),
        vec![serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": "ref-old",
            "client_id": "from-origin"
        })]
    );
    assert_eq!(
        fake_disk(&acct),
        serde_json::json!({
            "session": {
                "bearer": "acc-new",
                "renewal": "ref-new",
                "app": "from-origin",
                "grants": ["a", "b"],
                "extra": "keep",
                "issuer": issuer
            },
            "stamped": NOW_MS / 1000,
            "other": {"keep": [1, 2]}
        })
    );
    assert!(
        acct.join("fake-write.lock").is_file(),
        "文件锁那一形：锁文件，不是目录"
    );
    let again = access_token(&own(), &acct, &face, Some(&nowhere()), NOW_MS, "f", ANYTIME)
        .expect("刚续过 ⇒ 不再续");
    assert_eq!(expose(&again), "acc-new");
    assert_eq!(posted.lock().expect("lock").len(), 1);
    let _ = std::fs::remove_dir_all(&d);
}

/// 一家给不全：没有缺省令牌端点、凭据里也没记出处 ⇒ 不替它续，照实说（那一句点名是哪个号），盘上一个字节不动。
#[test]
fn without_an_endpoint_or_a_recorded_origin_it_is_not_renewed_and_says_so() {
    let face = fake_face();
    let d = temp_dir("noorigin");
    let acct = d.join("acct");
    let body = fake_creds(None, STALE_S).to_string();
    std::fs::write(acct.join(face.creds_file), &body).expect("write");
    assert_eq!(
        access_token(&own(), &acct, &face, None, NOW_MS, "f", ANYTIME).map(|s| expose(&s)),
        Err(Unusable::CannotRenew)
    );
    assert_eq!(
        std::fs::read_to_string(acct.join(face.creds_file)).expect("read"),
        body
    );
    assert_eq!(
        Unusable::CannotRenew.said("f").said,
        copy_text("beOauth.unusable.cannotRenew", &[("account", "f")])
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 整份写锁是文件锁、写回那一刻别人持着 ⇒ 新令牌照用、盘上先不动；那把锁一放（到期再试那一下）就写回。
/// 文件锁放开不出文件事件，写回只在期限到的那一下再试：放锁要赶在期限前。期限给 5 秒，不是 400 毫秒 ——
/// 机器忙时这条判据线程从续期到放锁被饿过 400 毫秒，期限先到、写回作废，就是时红时绿。
#[test]
fn a_file_write_lock_held_elsewhere_defers_the_write_and_keeps_the_new_token() {
    let face = LoginFace {
        write_wait_ms: 5_000,
        ..fake_face()
    };
    let d = temp_dir("filewl");
    let acct = d.join("acct");
    let (addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    std::fs::write(
        acct.join(face.creds_file),
        fake_creds(Some(&issuer), STALE_S).to_string(),
    )
    .expect("write");
    let theirs = test_home::held_elsewhere(&acct, "fake-write.lock");
    let r = access_token(&own(), &acct, &face, None, NOW_MS, "f", ANYTIME).map(|s| expose(&s));
    assert_eq!(r, Ok("acc-new".to_string()), "新令牌照用");
    assert_eq!(
        fake_disk(&acct)["session"]["bearer"],
        "acc-old",
        "锁还持着 ⇒ 盘上不动"
    );
    test_home::let_go(theirs);
    assert!(
        wait_until(|| fake_disk(&acct)["session"]["bearer"] == "acc-new"),
        "锁放了 ⇒ 到期那一下写回"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 过期时刻交那一家读的那一形：回包里没有 `expires_in` 也续得成（几点过期照那一家的读法读写回后的那一份）；
/// 记在一格里的那一形（Claude）缺它照旧当读不懂。
#[test]
fn a_family_that_reads_its_own_expiry_does_not_need_expires_in() {
    const NO_TTL: &str = r#"{"access_token":"acc-new","refresh_token":"ref-new"}"#;
    let face = fake_face();
    let d = temp_dir("nottl");
    let acct = d.join("acct");
    let (addr, posted) = spawn_token_endpoint(NO_TTL, 200, None);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    std::fs::write(
        acct.join(face.creds_file),
        fake_creds(Some(&issuer), STALE_S).to_string(),
    )
    .expect("write");
    let got = access_token(&own(), &acct, &face, None, NOW_MS, "f", ANYTIME).expect("续得上");
    assert_eq!(expose(&got), "acc-new");
    assert_eq!(fake_disk(&acct)["stamped"], NOW_MS / 1000);
    let again = access_token(&own(), &acct, &face, None, NOW_MS, "f", ANYTIME).expect("不再续");
    assert_eq!(expose(&again), "acc-new");
    assert_eq!(posted.lock().expect("lock").len(), 1);
    let c = temp_dir("nottl-claude");
    let cacct = c.join("acct");
    write_creds(&cacct, "acc-old", "ref-old", NOW_MS - 1);
    let (caddr, _) = spawn_token_endpoint(NO_TTL, 200, None);
    assert!(matches!(
        access_token(
            &own(),
            &cacct,
            &FACE,
            Some(&endpoint_at(caddr)),
            NOW_MS,
            "q",
            ANYTIME
        ),
        Err(Unusable::NotRenewed(_))
    ));
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&c);
}

// ───────────── 账号 0 不续不写：照那一家的声明（替身家声明不续不写） ─────────────

/// 账号 0 那一形的 `When`（有会话在用也续，只差「是不是账号 0」）。
const BASE: When = When {
    base: true,
    ..ANYTIME
};

/// ★ 声明账号 0 不续不写的那一家：账号 0 到期了也不续 —— 令牌端点一发不打、不拿续期锁 · 写锁（号目录里不多一个文件）、
/// 凭据文件一字不变、不记成续不上，照实回那一句。同一家的号载体那个号照续。
#[test]
fn account_zero_of_a_family_that_leaves_it_alone_is_never_renewed_locked_or_written() {
    let face = fake_face();
    assert_eq!(face.base_login, crate::agents::BaseLogin::Untouched);
    let d = temp_dir("base-untouched");
    let acct = d.join("acct");
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    let body = fake_creds(Some(&issuer), STALE_S).to_string();
    std::fs::write(acct.join(face.creds_file), &body).expect("write");
    // 那一家声明的锁（替身家只有整份写锁）：拿过就留在号目录里。
    let locks = [acct.join(face.write_lock.expect("替身家声明了写锁").name())];
    assert_eq!(
        access_token(&own(), &acct, &face, None, NOW_MS, "0", BASE).map(|s| expose(&s)),
        Err(Unusable::BaseUntouched)
    );
    assert!(posted.lock().expect("lock").is_empty(), "一发都不打");
    for l in &locks {
        assert!(!l.exists(), "不拿锁：{} 不在", l.display());
    }
    assert_eq!(
        std::fs::read_to_string(acct.join(face.creds_file)).expect("read"),
        body,
        "凭据一字不变"
    );
    assert_eq!(
        missed::missed(&acct, &face, NOW_MS / 1000),
        None,
        "不记成续不上"
    );
    assert_eq!(
        Unusable::BaseUntouched.said("0").said,
        copy_text("beOauth.unusable.baseUntouched", &[("account", "0")])
    );
    assert!(!Unusable::BaseUntouched.is_account_fact());

    let carrier =
        access_token(&own(), &acct, &face, None, NOW_MS, "c", ANYTIME).expect("号载体照续");
    assert_eq!(expose(&carrier), "acc-new");
    assert_eq!(posted.lock().expect("lock").len(), 1);
    assert!(
        locks[0].exists(),
        "续过一次写锁就在：上面判「不在」不是空话"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 声明账号 0 不续不写的那一家：账号 0 的令牌还没过期 ⇒ 照读照用（只读：不拿锁、不打端点、凭据一字不变）。
#[test]
fn account_zero_of_a_family_that_leaves_it_alone_still_hands_out_an_unexpired_token() {
    let face = fake_face();
    let d = temp_dir("base-untouched-fresh");
    let acct = d.join("acct");
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    // 一分钟前续过 ⇒ 还剩五十九分钟，没进快过期的余量。
    let body = fake_creds(Some(&issuer), NOW_MS / 1000 - 60).to_string();
    std::fs::write(acct.join(face.creds_file), &body).expect("write");
    let got = access_token(&own(), &acct, &face, None, NOW_MS, "0", BASE).map(|s| expose(&s));
    assert_eq!(got, Ok("acc-old".to_string()), "没过期 ⇒ 照用");
    assert!(posted.lock().expect("lock").is_empty(), "一发都不打");
    assert_eq!(
        entries(&acct),
        vec![face.creds_file.to_string()],
        "号目录里不多一样东西"
    );
    assert_eq!(
        std::fs::read_to_string(acct.join(face.creds_file)).expect("read"),
        body,
        "凭据一字不变"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 换号判接不接得上问的那一处（[`unrenewed_base`]）与拿令牌同一判：不归我方续的账号 0 过期了 ⇒ 是；没过期 · 号载体 ·
/// 不声明的那一家 · 没登录 ⇒ 不是。只读：问完盘上一字不变、不拿锁。
#[test]
fn an_expired_account_zero_left_to_its_family_is_said_unreachable_by_the_same_judgment() {
    let face = fake_face();
    let d = temp_dir("base-unrenewed");
    let stale = d.join("acct");
    let body = fake_creds(None, STALE_S).to_string();
    std::fs::write(stale.join(face.creds_file), &body).expect("write");
    let fresh_dir = d.join("fresh");
    std::fs::create_dir_all(&fresh_dir).expect("mkdir");
    std::fs::write(
        fresh_dir.join(face.creds_file),
        fake_creds(None, NOW_MS / 1000 - 60).to_string(),
    )
    .expect("write");
    let empty = d.join("empty");
    std::fs::create_dir_all(&empty).expect("mkdir");
    assert!(unrenewed_base(&stale, &face, BASE, NOW_MS));
    assert_eq!(
        access_token(&own(), &stale, &face, None, NOW_MS, "0", BASE).map(|s| expose(&s)),
        Err(Unusable::BaseUntouched),
        "拿令牌那一下同一判"
    );
    assert!(!unrenewed_base(&fresh_dir, &face, BASE, NOW_MS), "没过期");
    assert!(!unrenewed_base(&stale, &face, ANYTIME, NOW_MS), "号载体");
    let renews = LoginFace {
        base_login: crate::agents::BaseLogin::Renew,
        ..face
    };
    assert!(!unrenewed_base(&stale, &renews, BASE, NOW_MS), "不声明");
    assert!(!unrenewed_base(&empty, &face, BASE, NOW_MS), "没登录");
    assert_eq!(
        std::fs::read_to_string(stale.join(face.creds_file)).expect("read"),
        body
    );
    assert_eq!(entries(&stale), vec![face.creds_file.to_string()]);
    let _ = std::fs::remove_dir_all(&d);
}

/// 不声明（账号 0 照续）的那一家：账号 0 到期了照常续、写回。
#[test]
fn account_zero_of_a_family_that_renews_it_is_renewed_like_any_other() {
    let face = LoginFace {
        base_login: crate::agents::BaseLogin::Renew,
        ..fake_face()
    };
    let d = temp_dir("base-renew");
    let acct = d.join("acct");
    let (addr, posted) = spawn_token_endpoint(NEW_TOKENS, 200, None);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    std::fs::write(
        acct.join(face.creds_file),
        fake_creds(Some(&issuer), STALE_S).to_string(),
    )
    .expect("write");
    let got = access_token(&own(), &acct, &face, None, NOW_MS, "0", BASE).expect("账号 0 照续");
    assert_eq!(expose(&got), "acc-new");
    assert_eq!(posted.lock().expect("lock").len(), 1);
    assert_eq!(fake_disk(&acct)["session"]["bearer"], "acc-new");
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 那一家续期不拿锁（替身家不声明续期锁）⇒ 续得成；续期那一发发出的那一刻，号目录里只有凭据文件，没多出一把锁目录。
#[test]
fn a_family_without_a_renew_lock_renews_without_making_one() {
    let face = fake_face();
    assert_eq!(face.lock_inside, None);
    assert_eq!(face.lock_beside, None);
    let d = temp_dir("nolock");
    let acct = d.join("acct");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (addr, posted) = {
        let (seen, acct) = (Arc::clone(&seen), acct.clone());
        serve_tokens(NEW_TOKENS, 200, move || {
            seen.lock().expect("lock").push(entries(&acct));
        })
    };
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    std::fs::write(
        acct.join(face.creds_file),
        fake_creds(Some(&issuer), STALE_S).to_string(),
    )
    .expect("write");
    let got = access_token(&own(), &acct, &face, None, NOW_MS, "f", ANYTIME).map(|s| expose(&s));
    assert_eq!(got, Ok("acc-new".to_string()));
    assert_eq!(posted.lock().expect("lock").len(), 1);
    assert_eq!(
        seen.lock().expect("lock").clone(),
        vec![vec![face.creds_file.to_string()]],
        "续期那一刻号目录里多了东西"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 那一家声明续成另换一格（替身家：回包 `id_token` ⇒ `session.badge`，先核 JWT 形状）：
/// 是 JWT 形状 ⇒ 那一格换成新的（存原串），访问 · 刷新令牌照换；回包里没有 · null · 空串 ⇒ 那一格原样。没声明的格（回包里别的）一概不写。
#[test]
fn a_declared_extra_cell_is_renewed_when_it_passes_its_check() {
    const JWT: &str = r#"{"access_token":"acc-new","refresh_token":"ref-new","id_token":"aGVhZA.Ym9keQ.c2ln","other":"x"}"#;
    const NONE: &str = r#"{"access_token":"acc-new","refresh_token":"ref-new"}"#;
    const NULL: &str = r#"{"access_token":"acc-new","refresh_token":"ref-new","id_token":null}"#;
    const EMPTY: &str = r#"{"access_token":"acc-new","refresh_token":"ref-new","id_token":""}"#;
    let face = fake_face();
    for (tag, answer, badge) in [
        ("cell-jwt", JWT, "aGVhZA.Ym9keQ.c2ln"),
        ("cell-none", NONE, "b.a.d"),
        ("cell-null", NULL, "b.a.d"),
        ("cell-empty", EMPTY, "b.a.d"),
    ] {
        let d = temp_dir(tag);
        let acct = d.join("acct");
        let (addr, _) = spawn_token_endpoint(answer, 200, None);
        let issuer = format!("http://127.0.0.1:{}", addr.port());
        let mut creds = fake_creds(Some(&issuer), STALE_S);
        creds["session"]["badge"] = Value::from("b.a.d");
        std::fs::write(acct.join(face.creds_file), creds.to_string()).expect("write");
        let got =
            access_token(&own(), &acct, &face, None, NOW_MS, "f", ANYTIME).map(|s| expose(&s));
        assert_eq!(got, Ok("acc-new".to_string()), "{tag}");
        let disk = fake_disk(&acct);
        assert_eq!(disk["session"]["bearer"], "acc-new", "{tag}");
        assert_eq!(disk["session"]["renewal"], "ref-new", "{tag}");
        assert_eq!(disk["session"]["badge"], badge, "{tag}");
        assert!(
            disk["session"].get("other").is_none(),
            "{tag}：没声明的格写进去了"
        );
        assert!(disk["session"].get("id_token").is_none(), "{tag}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// ★ 回包里那一格过不了它的校验（替身家：`id_token` 不是 JWT 形状）⇒ 整次不落盘：盘上一个字节不变，照实回「续期回包读不懂」。
#[test]
fn a_declared_extra_cell_that_fails_its_check_writes_nothing() {
    const NOT_JWT: &str =
        r#"{"access_token":"acc-new","refresh_token":"ref-new","id_token":"not-a-jwt"}"#;
    const NOT_TEXT: &str = r#"{"access_token":"acc-new","refresh_token":"ref-new","id_token":123}"#;
    for (tag, bad) in [("cell-bad", NOT_JWT), ("cell-num", NOT_TEXT)] {
        bad_cell_writes_nothing(tag, bad);
    }
}

fn bad_cell_writes_nothing(tag: &str, bad: &'static str) {
    let face = fake_face();
    let d = temp_dir(tag);
    let acct = d.join("acct");
    let (addr, posted) = spawn_token_endpoint(bad, 200, None);
    let issuer = format!("http://127.0.0.1:{}", addr.port());
    let mut creds = fake_creds(Some(&issuer), STALE_S);
    creds["session"]["badge"] = Value::from("b.a.d");
    let body = creds.to_string();
    std::fs::write(acct.join(face.creds_file), &body).expect("write");
    let got = access_token(&own(), &acct, &face, None, NOW_MS, "f", ANYTIME);
    assert_eq!(posted.lock().expect("lock").len(), 1, "续期那一发打了");
    match got {
        Err(Unusable::NotRenewed(why)) => assert_eq!(
            why.said,
            copy_text("beOauth.refresh.unparsable", &[]),
            "照实说回包读不懂"
        ),
        other => panic!(
            "{tag}：坏的那一格该整次不落盘：{:?}",
            other.map(|s| expose(&s))
        ),
    }
    assert_eq!(
        std::fs::read_to_string(acct.join(face.creds_file)).expect("read"),
        body,
        "{tag}：盘上一个字节不变"
    );
    let _ = std::fs::remove_dir_all(&d);
}
