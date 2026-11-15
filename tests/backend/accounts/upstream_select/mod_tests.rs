//! 上游选择那一层（`Accounts`）自己的判据：锁的持有范围。

use super::*;
use crate::relay::{Ask, Mode, RouteKey};
use std::sync::mpsc;

fn ask(body: &[u8]) -> Ask<'_> {
    Ask {
        label: "",
        origin: None,
        body,
        heads: &[],
    }
}

/// 一发卡在 `act` 里（连上游连得慢 · 写请求体写不动，都在 `act` 里）时凭据文件被改了，
/// 下一发照常拿到去处、看到的是新表 —— 不排在卡住那一发后面。
///
/// 修之前：`resolve` 握着表的读锁跑 `act`；下一发先重载，要写锁 ⇒ 等到卡住那一发的 `act` 返回，
/// 而 std 的 `RwLock` 有写者在等时新读者也进不来 ⇒ 所有中转请求一起卡住。
#[test]
fn a_request_stuck_inside_act_does_not_hold_up_a_reload_or_the_next_request() {
    let dir = test_home::scratch("reload");
    let path = dir.join("creds.json");
    let upstreams = Upstreams::from_env(&|_| None).expect("内置默认必须解析得了");
    let file_agent = upstreams.file_agent();
    let (empty, _, _) = table::build(Vec::new(), file_agent, upstreams.of_credentials_file());
    let accounts = std::sync::Arc::new(
        Accounts::new(empty, upstreams, crate::platform::fs::Own::for_tests())
            .reloading_from(Reload::new(path.clone(), None)),
    );
    let key = RouteKey {
        seg1: file_agent.to_string(),
        seg2: "acct".to_string(),
    };

    // 第一发：进了 `act` 就停住，等第二发拿到去处才出来（兜底等到墙钟上限）。
    // ⇒ 不量快慢，只看先后：第二发是不是在第一发还卡在 `act` 里的时候就答到了。
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let first = {
        let accounts = std::sync::Arc::clone(&accounts);
        let key = key.clone();
        std::thread::spawn(move || {
            let mut answered_meanwhile = false;
            accounts.resolve(Mode::Substitute, &key, &ask(b"{}"), &mut |_| {
                let _ = entered_tx.send(());
                answered_meanwhile = done_rx.recv_timeout(guard_core::wall::CEILING).is_ok();
            });
            answered_meanwhile
        })
    };
    entered_rx
        .recv_timeout(guard_core::wall::CEILING)
        .expect("第一发没进到 act");

    // 改凭据文件（人手写一份 · 只给属主读写）。
    std::fs::write(&path, b"{\"api_key\":\"sk-ant-RELOAD-WHILE-STUCK\"}").expect("写夹具");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("改权限");
    }

    // 第二发：另一线程发。
    let second = {
        let accounts = std::sync::Arc::clone(&accounts);
        let key = key.clone();
        std::thread::spawn(move || {
            accounts.resolve(Mode::Substitute, &key, &ask(b"{}"), &mut |_| {
                let _ = done_tx.send(());
            });
        })
    };
    let answered_meanwhile = first.join().expect("第一发");
    second.join().expect("第二发");
    assert!(
        answered_meanwhile,
        "第一发卡在 act 里时，第二发拿不到去处：重载排在卡住那一发的读锁后面"
    );
    // 非空对照：第二发确实重载了（看到的是新表，不是靠没重载才快）。
    assert_eq!(accounts.table().len(), 1, "第二发没换上新表");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── `/t/` 每一发都按请求体里的账号身份认号（路由写明了号也核一次）─────────────────
//
// 替身：一个临时家（账号库里订阅号 b · d，各一份假身份）· 只在内存与临时目录里的额度账与轮换账本 ·
// 回包头由判据自己编（不连上游：`resolve` 交出去处、`observe` 收这一份回包头）。

const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const UUID_D: &str = "dddddddd-5555-4555-8555-dddddddddddd";
const UUID_NOBODY: &str = "99999999-4444-4444-8444-999999999999";
const AGENT: &str = "claude-code";
const SID: &str = "s-1";

struct Home {
    root: std::path::PathBuf,
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = test_home::scratch(&format!("whose-{tag}"));
        for (id, uuid) in [("b", UUID_B), ("d", UUID_D)] {
            let dir = root.join("accts").join(id);
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(
                dir.join(".claude.json"),
                format!("{{\"oauthAccount\":{{\"accountUuid\":\"{uuid}\"}}}}"),
            )
            .expect("identity");
        }
        Self { root }
    }

    fn own(&self) -> crate::platform::fs::Own {
        crate::platform::fs::Own::issue_data_at(&self.root)
    }

    fn rotation_path(&self) -> std::path::PathBuf {
        self.root.join(crate::accounts::quota::rotation::FILE_NAME)
    }

    /// 生产段的上游选择：空表 · 换号装上（账号库 b · d，续令牌只发本机一个关掉的口）· 额度账落在这个家里。
    fn accounts(&self) -> (Accounts, std::sync::Arc<Ledger>) {
        use crate::accounts::upstream_select::rotate::{test_support::book_row, HistoryPorts, Hop};
        let quota = std::sync::Arc::new(Ledger::at(Some(
            self.root.join(crate::accounts::quota::ledger::FILE_NAME),
        )));
        let accts = self.root.join("accts");
        let hop = Hop::new(
            std::sync::Arc::new(crate::accounts::quota::rotation::RotationStore::at(Some(
                self.rotation_path(),
            ))),
            &crate::accounts::quota::adoptions_at(),
            std::sync::Arc::clone(&quota),
            Some(self.root.clone()),
            std::sync::Arc::new(move |_| {
                Some(acct_core::AccountBook {
                    accounts: ["b", "d"]
                        .iter()
                        .map(|id| book_row(id, Some(accts.join(id)), false))
                        .collect(),
                })
            }),
            HistoryPorts {
                live: std::sync::Arc::new(std::collections::BTreeSet::new),
                ring: std::sync::Arc::new(|| {}),
            },
            Some(dead_token_endpoint()),
        );
        let upstreams = Upstreams::from_env(&|_| None).expect("内置默认必须解析得了");
        let accounts = Accounts::new(
            RoutingTable::build(std::iter::empty()),
            upstreams,
            self.own(),
        )
        .recording_to(std::sync::Arc::clone(&quota))
        .rotating_with(hop);
        (accounts, quota)
    }

    fn session(&self) -> crate::accounts::quota::rules::book::Session {
        crate::accounts::quota::rotation::RotationStore::at(Some(self.rotation_path()))
            .now()
            .sessions[SID]
            .clone()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 续令牌一律发到这里：本机一个刚关掉的口，绝不发到那一家的真端点。
fn dead_token_endpoint() -> crate::accounts::oauth::TokenEndpoint {
    let port =
        std::net::TcpListener::bind(std::net::SocketAddr::new(crate::common::net::LOOPBACK, 0))
            .and_then(|l| l.local_addr())
            .expect("bind")
            .port();
    crate::accounts::oauth::TokenEndpoint {
        base: crate::relay::base_of(&format!("http://127.0.0.1:{port}")).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_millis(2_000),
    }
}

/// 请求体：`uuid` ＝ 账号身份那一格（`None` ⇒ 没有这一格）。
fn body_of(uuid: Option<&str>) -> Vec<u8> {
    let mut v = serde_json::json!({
        "model": "m",
        "messages": [{"role": "user", "content": "你好"}],
        "stream": true,
    });
    if let Some(uuid) = uuid {
        let user_id = format!(
            "{{\"device_id\":\"{}\",\"account_uuid\":\"{uuid}\",\"session_id\":\"{SID}\"}}",
            "d".repeat(64)
        );
        v["metadata"] = serde_json::json!({ "user_id": user_id });
    }
    serde_json::to_vec(&v).expect("json")
}

/// 走 `/t/<家>/<route>/…` 发一发：问去处 ⇒ 记下标签 ⇒ 交一份带 5 小时与周窗的回包头。回这一发的标签。
fn send(accounts: &Accounts, route: &str, body: &[u8]) -> String {
    let key = RouteKey {
        seg1: AGENT.to_string(),
        seg2: route.to_string(),
    };
    let ask = Ask {
        label: SID,
        origin: None,
        body,
        heads: &[],
    };
    let mut tag = None;
    accounts.resolve(Mode::Passthrough, &key, &ask, &mut |d| {
        if let comms_outward::Destination::Passthrough { tag: t, .. } = d {
            tag = Some(t.to_string());
        }
    });
    let tag = tag.expect("直通的一发交的是原样转发");
    let h = |k: &str, v: &str| (k.to_string(), v.to_string());
    let headers = [
        h("anthropic-ratelimit-unified-status", "allowed"),
        h(
            "anthropic-ratelimit-unified-representative-claim",
            "five_hour",
        ),
        h("anthropic-ratelimit-unified-5h-utilization", "0.1"),
        h("anthropic-ratelimit-unified-5h-reset", "4000000000"),
        h("anthropic-ratelimit-unified-7d-utilization", "0.27"),
        h("anthropic-ratelimit-unified-7d-reset", "4000600000"),
    ];
    accounts.observe(
        Mode::Passthrough,
        &key,
        &ask,
        &comms_outward::Heard {
            tag: &tag,
            status: 200,
            headers: &headers,
        },
    );
    tag
}

/// 额度账上这个号有哪几格窗口（没这个号 ⇒ 空）。
fn windows_of(quota: &Ledger, account: &str) -> Vec<String> {
    quota
        .entry(AGENT, account)
        .map(|o| o.reading.windows.iter().map(|w| w.name.clone()).collect())
        .unwrap_or_default()
}

/// ① 拿 b 的令牌走 d 的路由（请求体身份是 b）⇒ 这一发记给 b：b 有这一格周窗，d 名下没有；标签与起始号都是 b。
#[test]
fn a_request_routed_as_d_but_carrying_b_is_booked_under_b() {
    let home = Home::new("b-on-d");
    let (accounts, quota) = home.accounts();
    let tag = send(&accounts, "d", &body_of(Some(UUID_B)));
    assert_eq!(tag, "b");
    assert!(windows_of(&quota, "b").contains(&"seven_day".to_string()));
    assert!(
        windows_of(&quota, "d").is_empty(),
        "{:?}",
        windows_of(&quota, "d")
    );
    assert_eq!(home.session().start, "b");
}

/// ② 路由 d、身份也是 d ⇒ 记给 d（同今天）。
#[test]
fn a_request_routed_as_d_carrying_d_is_booked_under_d() {
    let home = Home::new("d-on-d");
    let (accounts, quota) = home.accounts();
    assert_eq!(send(&accounts, "d", &body_of(Some(UUID_D))), "d");
    assert!(windows_of(&quota, "d").contains(&"seven_day".to_string()));
    assert!(windows_of(&quota, "b").is_empty());
}

/// ③ 路由 d、身份在账号库里谁都对不上 ⇒ 不记给 d（记在「未指定账号」名下）。
#[test]
fn a_request_routed_as_d_carrying_a_stranger_is_not_booked_under_d() {
    let home = Home::new("nobody-on-d");
    let (accounts, quota) = home.accounts();
    let tag = send(&accounts, "d", &body_of(Some(UUID_NOBODY)));
    assert_eq!(tag, relay_route_core::UNDECLARED_ACCOUNT_SEGMENT);
    assert!(
        windows_of(&quota, "d").is_empty(),
        "{:?}",
        windows_of(&quota, "d")
    );
}

/// ④ 路由 d、请求体里没有身份这一格 ⇒ 照旧记给 d。这是已知的盲区（认不出是谁就只能信路由），钉住它，免得被当成已修好。
#[test]
fn a_request_routed_as_d_without_an_identity_cell_stays_under_d() {
    let home = Home::new("absent-on-d");
    let (accounts, quota) = home.accounts();
    assert_eq!(send(&accounts, "d", &body_of(None)), "d");
    assert!(windows_of(&quota, "d").contains(&"seven_day".to_string()));
}

/// ⑤ 会话不轮换、路由 d、身份 b ⇒ 轮换账本上起始号与在用号都是 b，换号记录里没有「d 换到 b」那一笔；接着的几发照旧走 b。
#[test]
fn a_session_routed_as_d_carrying_b_starts_on_b_without_a_fake_swap() {
    let home = Home::new("start-b");
    let (accounts, _quota) = home.accounts();
    for _ in 0..2 {
        assert_eq!(send(&accounts, "d", &body_of(Some(UUID_B))), "b");
    }
    let s = home.session();
    assert_eq!((s.start.as_str(), s.current.as_str()), ("b", "b"));
    let log = serde_json::to_string(&s.log).expect("json");
    assert!(!log.contains("\"d\""), "换号记录里出现了 d：{log}");
}

/// ⑥ 路由 d、d 盘上的身份读不到（没登录 · 文件不在）⇒ 说不出 d 是谁，照路由记给 d，不当成「对不上」记到未指定账号。
#[test]
fn a_request_routed_as_d_whose_own_identity_is_unreadable_stays_under_d() {
    let home = Home::new("d-unreadable");
    std::fs::remove_file(home.root.join("accts").join("d").join(".claude.json")).expect("rm");
    let (accounts, quota) = home.accounts();
    assert_eq!(send(&accounts, "d", &body_of(Some(UUID_NOBODY))), "d");
    assert!(windows_of(&quota, "d").contains(&"seven_day".to_string()));
}

/// ⑦ 路由 d、d 盘上的身份读不到、请求体身份是 b（认得出）⇒ 记给 b：说不出路由号是谁，不等于信路由。
#[test]
fn a_request_routed_as_d_whose_own_identity_is_unreadable_but_carrying_b_is_booked_under_b() {
    let home = Home::new("b-on-unreadable-d");
    std::fs::remove_file(home.root.join("accts").join("d").join(".claude.json")).expect("rm");
    let (accounts, quota) = home.accounts();
    assert_eq!(send(&accounts, "d", &body_of(Some(UUID_B))), "b");
    assert!(windows_of(&quota, "d").is_empty());
}

/// 这一段里「路由写的号」那几行 warn（别的 warn 不算）。
fn said_about_route(f: impl FnOnce()) -> Vec<String> {
    test_meters::heard(f)
        .into_iter()
        .filter(|l| l.contains("路由写的号"))
        .collect()
}

/// ⑧ 同一个号连发几次身份对不上 ⇒ warn 只一行；认出的号变了（b ⇒ 谁都对不上）再一行。
#[test]
fn a_mismatch_is_warned_once_and_again_only_when_the_recognized_account_changes() {
    let home = Home::new("warn-once");
    let (accounts, _quota) = home.accounts();
    let lines = said_about_route(|| {
        for _ in 0..3 {
            send(&accounts, "d", &body_of(Some(UUID_B)));
        }
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("对不上"), "{lines:?}");
    let lines = said_about_route(|| {
        for _ in 0..2 {
            send(&accounts, "d", &body_of(Some(UUID_NOBODY)));
        }
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    let undeclared = relay_route_core::UNDECLARED_ACCOUNT_SEGMENT;
    assert!(lines[0].contains("对不上"), "{lines:?}");
    assert!(
        lines[0].contains(&format!("照 {undeclared} 记")),
        "{lines:?}"
    );
    // 中间一发没有身份格（盲区，照路由）不算认出的变了：再拿 b 的身份来，b 那一行已说过就不再说。
    let lines = said_about_route(|| {
        send(&accounts, "d", &body_of(Some(UUID_B)));
        send(&accounts, "d", &body_of(None));
        send(&accounts, "d", &body_of(Some(UUID_B)));
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    // 对上了一发（身份就是 d）⇒ 上次认出的忘掉：之后再拿 b 的身份来，照样再说一行。
    let lines = said_about_route(|| {
        send(&accounts, "d", &body_of(Some(UUID_D)));
        send(&accounts, "d", &body_of(Some(UUID_B)));
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
}

/// ⑨ 两种原因各一句：路由号盘上的身份读不到（认出是 b）不说成「对不上」。
#[test]
fn an_unreadable_route_identity_is_said_apart_from_a_mismatch() {
    let home = Home::new("warn-unreadable");
    std::fs::remove_file(home.root.join("accts").join("d").join(".claude.json")).expect("rm");
    let (accounts, _quota) = home.accounts();
    let lines = said_about_route(|| {
        for _ in 0..2 {
            send(&accounts, "d", &body_of(Some(UUID_B)));
        }
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("读不到"), "{lines:?}");
    assert!(!lines[0].contains("对不上"), "{lines:?}");
    // 中间一发认不出谁（照路由记给 d）⇒ 认出的变回路由号；再拿 b 的身份来，再说一行。
    let lines = said_about_route(|| {
        send(&accounts, "d", &body_of(Some(UUID_NOBODY)));
        send(&accounts, "d", &body_of(Some(UUID_B)));
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    // d 登录上了（身份读得到、不是 b）：认出的还是 b，原因换了 ⇒ 再说一行「对不上」。
    std::fs::write(
        home.root.join("accts").join("d").join(".claude.json"),
        format!("{{\"oauthAccount\":{{\"accountUuid\":\"{UUID_D}\"}}}}"),
    )
    .expect("identity");
    let lines = said_about_route(|| {
        for _ in 0..2 {
            send(&accounts, "d", &body_of(Some(UUID_B)));
        }
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("对不上"), "{lines:?}");
}
