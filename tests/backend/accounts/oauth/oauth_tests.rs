//! 订阅号令牌：没过期直接给 · 快过期就对假令牌端点续、加锁整份原子写回 · 两个并发只续一次 · 锁有人持着 / 端点拒 / 盘上已被别人续过都照实处置。
//! 令牌全是现编的假值；目录全在临时目录里。

use super::*;
use std::io::{BufRead, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener};
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
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = l.local_addr().expect("addr");
    drop(l);
    endpoint_at(addr)
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
    let got = access_token(&acct, &FACE, &nowhere(), NOW_MS).expect("能用");
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
    let got = access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS).expect("续得上");
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
            std::thread::spawn(move || access_token(&acct, &FACE, &ep, NOW_MS).map(|s| expose(&s)))
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
        access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS).map(|s| expose(&s)),
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
    let r = access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS).map(|s| expose(&s));
    assert_eq!(r, Err(Unusable::Refused(400)));
    assert_eq!(on_disk(&acct), before);
    let said = Unusable::Refused(400).said("q");
    assert!(said.contains('q') && !said.contains("ref-old"), "{said}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn when_the_agent_refreshed_first_its_tokens_win_and_ours_are_not_written() {
    let d = temp_dir("cas");
    let acct = d.join("acct");
    write_creds(&acct, "acc-old", "ref-old", NOW_MS - 1);
    let (addr, _) = spawn_token_endpoint(NEW_TOKENS, 200, Some(acct.clone()));
    let got = access_token(&acct, &FACE, &endpoint_at(addr), NOW_MS).expect("用它的");
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
        access_token(&d.join("acct"), &FACE, &nowhere(), NOW_MS).map(|s| expose(&s)),
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
