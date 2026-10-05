//! 换号那一族的帧命令：默认轮换整份收 · 一批会话的那一份 · 改会话轮换 · 不重启换。临时家目录、假凭据、不起中转。

use super::*;
use crate::accounts::quota::rotation::{SessionRotationState, SwitchWhy};
use crate::accounts::upstream_select::rotate::{LibAccount, Library};
use crate::faces::rotation_switch_face::{restart_args, switch_ask};

const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const B_TOKEN: &str = "fake-access-of-b";
/// 判据里的「此刻」用真钟（假凭据的过期时刻也按真钟算 ⇒ 不会因为钟拨快而去续）。
fn now() -> u64 {
    crate::accounts::quota::now_unix()
}

/// 续令牌一律发到本机一个刚关掉的口（连不上 ⇒ 续不上），绝不发到那一家的真端点。
fn dead_token_endpoint() -> crate::accounts::oauth::TokenEndpoint {
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .expect("bind")
        .port();
    crate::accounts::oauth::TokenEndpoint {
        base: crate::relay::Base::parse(&format!("http://127.0.0.1:{port}")).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_millis(2_000),
    }
}

struct Home {
    root: std::path::PathBuf,
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotface-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let b = root.join("accts").join("b");
        std::fs::create_dir_all(&b).expect("mkdir");
        let far = (crate::accounts::quota::now_unix() + 3600) * 1000;
        std::fs::write(
            b.join(".credentials.json"),
            format!("{{\"claudeAiOauth\":{{\"accessToken\":\"{B_TOKEN}\",\"refreshToken\":\"fake-refresh-of-b\",\"expiresAt\":{far}}}}}"),
        )
        .expect("creds");
        std::fs::write(
            b.join(".claude.json"),
            format!("{{\"oauthAccount\":{{\"accountUuid\":\"{UUID_B}\"}}}}"),
        )
        .expect("identity");
        std::fs::create_dir_all(root.join("accts").join("c")).expect("mkdir");
        Self { root }
    }

    /// 账号库：a（起会话的号）· b（有登录）· c（没登录）· api（按量号，key 表里有）。
    fn ctx(&self) -> Ctx {
        let accts = self.root.join("accts");
        let library: crate::accounts::upstream_select::rotate::LibraryRead =
            Arc::new(move |_| Library {
                enabled: true,
                accounts: [("a", false), ("b", false), ("c", false), ("api", true)]
                    .iter()
                    .map(|(id, api)| LibAccount {
                        id: id.to_string(),
                        dir: Some(accts.join(id)),
                        api: *api,
                    })
                    .collect(),
            });
        Ctx {
            hop: Hop::new(
                Arc::new(RotationStore::at(Some(self.root.join(rotation::FILE_NAME)))),
                Arc::new(Ledger::at(Some(self.root.join(ledger::FILE_NAME)))),
                Some(self.root.clone()),
                library,
                Some(dead_token_endpoint()),
            ),
            rows: Box::new(|_, a| (a == "api").then_some(true)),
        }
    }

    fn saw(&self, ctx: &Ctx, sid: &str) {
        rotation::face_change(&ctx.hop.store, |b| b.saw(sid, "claude-code", "a", now()))
            .expect("write");
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// ★ 默认轮换：整份写回、读回同形；新勾的按量号挪末尾；不合法整份拒、点名那一格、一个字节不写。
#[test]
fn the_default_rotation_is_written_whole_and_refused_whole() {
    let home = Home::new("default");
    let ctx = home.ctx();
    let got = answer_set_with(
        &ctx,
        &json!({"rotation": {"order": [{"start": true}, "api", "b", "c"], "enabled": ["api", "b"], "when": {"threshold": {"n": 90}}}}),
    )
    .expect("ok");
    assert_eq!(
        got["rotation"],
        json!({"order": [{"start": true}, "b", "c", "api"], "enabled": ["api", "b"], "when": {"threshold": {"n": 90}}})
    );
    assert_eq!(got["state"], "present");
    let before = std::fs::read(home.root.join(rotation::FILE_NAME)).expect("read");
    let (code, msg) = answer_set_with(
        &ctx,
        &json!({"rotation": {"order": [{"start": true}, "b"], "enabled": ["x"], "when": "full"}}),
    )
    .expect_err("应拒");
    assert_eq!(code, "bad_args");
    assert!(msg.contains("enabled[0]"), "{msg}");
    assert_eq!(
        std::fs::read(home.root.join(rotation::FILE_NAME)).expect("read"),
        before
    );
}

/// ★ 一批会话：见过的出那一份（「账号」格 · 下一个），没见过的照实标 `absent`、不整批失败。
#[test]
fn a_batch_read_answers_every_session_and_marks_the_unknown() {
    let home = Home::new("read");
    let ctx = home.ctx();
    answer_set_with(
        &ctx,
        &json!({"rotation": {"order": [{"start": true}, "c", "b"], "enabled": ["c", "b"], "when": "full"}}),
    )
    .expect("ok");
    home.saw(&ctx, "s-1");
    let got = answer_session_read_with(&ctx, &json!({"sids": ["s-1", "s-2"]}), now()).expect("ok");
    assert_eq!(
        got["sessions"]["s-2"],
        json!({"state": "absent", "inPlace": "noRelay"})
    );
    let one: SessionRotationState =
        serde_json::from_value(got["sessions"]["s-1"].clone()).expect("shape");
    let SessionRotationState::Present(v) = one else {
        panic!("应查得到");
    };
    assert_eq!(
        (v.account.start.as_str(), v.account.current.as_str()),
        ("a", "a")
    );
    assert_eq!(v.account.in_place, rotation::InPlace::Ok);
    assert_eq!(v.next.as_deref(), Some("b"), "c 没登录 ⇒ 跳过它");
    assert_eq!(v.blocked, None);
    assert_eq!(v.fallback_api.as_deref(), Some("api"));
    assert!(answer_session_read_with(&ctx, &json!({"sids": ["bad id"]}), now()).is_err());
}

/// ★ 改会话轮换：自己那一份 → 切回跟随（自己那一份留着）→ `"custom"` 恢复它；不合法整批拒；没见过的跳过。
#[test]
fn a_session_can_go_custom_and_back_keeping_its_own() {
    let home = Home::new("session");
    let ctx = home.ctx();
    home.saw(&ctx, "s-1");
    let custom = json!({"order": ["a", "api", "b"], "enabled": ["a", "b", "api"], "when": "full"});
    let got = answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-1", "s-9"], "rotation": {"custom": custom}}),
        now(),
    )
    .expect("ok");
    assert_eq!(got["sessions"]["s-1"], json!({"state": "done"}));
    assert_eq!(
        got["sessions"]["s-9"],
        json!({"state": "skipped", "code": "noRelay"})
    );
    let s = ctx.hop.store.now().sessions["s-1"].clone();
    assert!(!s.follow);
    assert_eq!(
        serde_json::to_value(&s.custom).expect("json")["order"],
        json!(["a", "b", "api"]),
        "新勾的按量号挪末尾"
    );
    answer_session_set_with(&ctx, &json!({"sids": ["s-1"], "rotation": "follow"}), now())
        .expect("ok");
    let s = ctx.hop.store.now().sessions["s-1"].clone();
    assert!(s.follow && s.custom.is_some(), "切回跟随，自己那一份留着");
    answer_session_set_with(&ctx, &json!({"sids": ["s-1"], "rotation": "custom"}), now())
        .expect("ok");
    assert!(!ctx.hop.store.now().sessions["s-1"].follow);
    let bad = json!({"sids": ["s-1"], "rotation": {"custom": {"order": ["b", "b"], "enabled": [], "when": "full"}}});
    let before = ctx.hop.store.now();
    assert_eq!(
        answer_session_set_with(&ctx, &bad, now())
            .expect_err("应拒")
            .0,
        "bad_args"
    );
    assert_eq!(ctx.hop.store.now(), before);
    let fresh =
        json!({"sids": ["s-new"], "rotation": "follow", "agent": "claude-code", "start": "a"});
    assert_eq!(
        answer_session_set_with(&ctx, &fresh, now()).expect("ok")["sessions"]["s-new"],
        json!({"state": "done"})
    );
}

/// ★ 不重启换：钉过去、记一条 `manualHot`、这个会话响一下；目标号没登录 ⇒ `failed{targetNeedsLogin}`，钉号不动；
/// 中转没见过 ⇒ `skipped{noRelay}`。应答与盘上零令牌。
#[test]
fn a_hot_switch_pins_the_session_or_says_why_not() {
    let home = Home::new("hot");
    let ctx = home.ctx();
    home.saw(&ctx, "s-1");
    let mut rx = rotation::changes().subscribe();
    assert_eq!(
        hot_one(&ctx, "s-1", "c", now()),
        SwitchOutcome::NotSwitched {
            code: "targetNeedsLogin".into()
        }
    );
    assert_eq!(ctx.hop.store.now().sessions["s-1"].current, "a");
    assert_eq!(hot_one(&ctx, "s-1", "b", now()), SwitchOutcome::Switched);
    let s = ctx.hop.store.now().sessions["s-1"].clone();
    assert_eq!(
        (s.current.as_str(), &s.history[0].why),
        ("b", &SwitchWhy::ManualHot)
    );
    let mut rang = Vec::new();
    while let Ok(sid) = rx.try_recv() {
        rang.push(sid);
    }
    assert!(rang.iter().any(|s| s == "s-1"), "{rang:?}");
    assert_eq!(
        hot_one(&ctx, "s-2", "b", now()),
        SwitchOutcome::Skipped {
            code: "noRelay".into()
        }
    );
    let read = answer_session_read_with(&ctx, &json!({"sids": ["s-1"]}), now())
        .expect("ok")
        .to_string();
    let disk = std::fs::read_to_string(home.root.join(rotation::FILE_NAME)).expect("read");
    for text in [read, disk] {
        assert!(text.contains("manualHot"), "正控");
        assert!(
            !text.contains(B_TOKEN) && !text.contains("fake-refresh-of-b"),
            "{text}"
        );
    }
}

/// 入参：不重启换收会话 id、重启换收 `session-restart` 的入参（补上 `account`）；形状不对整条拒。
#[test]
fn the_switch_arguments_are_shaped_per_mode() {
    assert!(switch_ask(&json!({"sessions": ["s-1"], "target": "b", "mode": "hot"})).is_ok());
    assert!(
        switch_ask(&json!({"sessions": [{"sid": "s-1"}], "target": "b", "mode": "hot"})).is_err()
    );
    assert!(switch_ask(&json!({"sessions": ["s-1"], "target": "b", "mode": "restart"})).is_err());
    assert!(switch_ask(&json!({"sessions": ["s-1"], "target": "_", "mode": "hot"})).is_err());
    assert!(switch_ask(&json!({"sessions": ["s-1"], "target": "b", "mode": "later"})).is_err());
    let item = json!({"sid": "s-1", "cwd": "/p", "compact_first": false});
    assert_eq!(
        restart_args(&item, "b"),
        json!({"sid": "s-1", "cwd": "/p", "compact_first": false, "account": "b"})
    );
}
