//! 换号那一族的帧命令：默认轮换整份收 · 一批会话的那一份 · 改会话轮换 · 不重启换。临时家目录、假凭据、不起中转。

use super::*;
use crate::accounts::quota::rotation::{SessionRotationState, SwitchWhy};
use crate::accounts::upstream_select::rotate::{LibAccount, Library};
use crate::faces::rotation_switch_face::{restart_args, restart_outcome, switch_ask};

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
    /// 此刻活着的会话（`saw` 记下的都活着，`end` 摘掉）。
    live: Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
    /// 活着的会话此刻在干什么（没写的 ＝ 说不清）。
    doing: Arc<std::sync::Mutex<std::collections::BTreeMap<String, Doing>>>,
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
        Self {
            root,
            live: Arc::default(),
            doing: Arc::default(),
        }
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
            lineage: Arc::new(crate::lineage::LineageStore::at(Some(
                self.root.join(crate::lineage::FILE_NAME),
            ))),
            rows: Box::new(|_, a| (a == "api").then_some(true)),
            live: {
                let live = Arc::clone(&self.live);
                Box::new(move || live.lock().expect("lock").clone())
            },
            doing: {
                let doing = Arc::clone(&self.doing);
                Box::new(move || doing.lock().expect("lock").clone())
            },
        }
    }

    fn saw(&self, ctx: &Ctx, sid: &str) {
        rotation::face_change(&ctx.hop.store, |b| b.saw(sid, "claude-code", "a", now()))
            .expect("write");
        self.live.lock().expect("lock").insert(sid.to_string());
    }

    fn end(&self, sid: &str) {
        self.live.lock().expect("lock").remove(sid);
    }
}

/// 改默认规则那一份（经 `rotation-rule-save`，名字照旧）；回那条规则（线上形状）。
fn answer_set_with(ctx: &Ctx, args: &Value) -> Answer {
    let rules = answer_rules_read_with(ctx).expect("read");
    let id = rules["defaultRule"].as_str().expect("default").to_string();
    let name = rules["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|r| r["id"] == id.as_str())
        .expect("default rule")["name"]
        .clone();
    let got = answer_rule_save_with(
        ctx,
        &json!({"id": id, "name": name, "rotation": args["rotation"]}),
        now(),
    )?;
    assert_eq!(got["state"], "saved", "{got}");
    Ok(got["rule"].clone())
}

/// 这个会话此刻的来源。
fn source_of(ctx: &Ctx, sid: &str) -> Source {
    ctx.hop.store.now().sessions[sid].source.clone()
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
        &json!({"rotation": {"order": [{"start": true}, "api", "b", "c"], "enabled": ["api", "b"], "cap": {"*": {"5h": 90, "7d": 90}}}}),
    )
    .expect("ok");
    assert_eq!(
        got["rotation"],
        json!({"order": [{"start": true}, "b", "c", "api"], "enabled": ["api", "b"], "cap": {"*": {"5h": 90, "7d": 90}}, "atLimit": "continue", "wait": 40})
    );
    assert_eq!(got["isDefault"], true);
    let before = std::fs::read(home.root.join(rotation::FILE_NAME)).expect("read");
    let (
        crate::stream::inbound::spec::Fail {
            code, message: msg, ..
        },
        diag,
    ) = crate::common::contract::tests::diag(|| {
        answer_set_with(
            &ctx,
            &json!({"rotation": {"order": [{"start": true}, "b"], "enabled": ["x"]}}),
        )
        .expect_err("应拒")
    });
    assert_eq!(code, "bad_args");
    assert!(diag.contains("enabled[0]"), "{msg}");
    let (
        crate::stream::inbound::spec::Fail {
            code, message: msg, ..
        },
        diag,
    ) = crate::common::contract::tests::diag(|| {
        answer_set_with(
        &ctx,
        &json!({"rotation": {"order": [{"start": true}, "b"], "enabled": ["b"], "atLimit": "halt"}}),
    )
    .expect_err("应拒")
    });
    assert_eq!(code, "bad_args");
    assert!(diag.contains("atLimit"), "{msg}");
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
        &json!({"rotation": {"order": [{"start": true}, "c", "b"], "enabled": ["c", "b"]}}),
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

/// ★ 「下一个」跳过用不了的号：没登录的（c）· 被拒着的（b：一发没带限额头、也没说几点再试的 429，照适配层读成的那一形记进额度账）；
/// 被拒的短期限一过，b 照常排回下一个。
#[test]
fn next_skips_an_account_without_login_and_one_refused_without_a_reset() {
    let home = Home::new("next-skip");
    let ctx = home.ctx();
    answer_set_with(
        &ctx,
        &json!({"rotation": {"order": [{"start": true}, "b", "c", "api"], "enabled": ["b", "c", "api"]}}),
    )
    .expect("ok");
    home.saw(&ctx, "s-1");
    let t = now();
    let r = crate::agents::claudecode::quota::read(429, &[], t).expect("被拒");
    ledger::record_seen(&ctx.hop.quota, "claude-code", "b", r, t);
    let next_at = |at: u64| {
        let got = answer_session_read_with(&ctx, &json!({"sids": ["s-1"]}), at).expect("ok");
        let SessionRotationState::Present(v) =
            serde_json::from_value(got["sessions"]["s-1"].clone()).expect("shape")
        else {
            panic!("应查得到");
        };
        v.next
    };
    assert_eq!(
        next_at(t).as_deref(),
        Some("api"),
        "b 被拒着、c 没登录 ⇒ 都跳过"
    );
    assert_eq!(
        next_at(t + crate::agents::claudecode::quota::REFUSED_BRIEFLY).as_deref(),
        Some("b"),
        "短期限过了 ⇒ b 照常可选"
    );
}

/// ★ 改会话轮换：自己那一份 → 切回跟随（自己那一份留着）→ `"custom"` 恢复它；不合法整批拒；没见过的跳过。
#[test]
fn a_session_can_go_custom_and_back_keeping_its_own() {
    let home = Home::new("session");
    let ctx = home.ctx();
    home.saw(&ctx, "s-1");
    let custom = json!({"order": ["a", "api", "b"], "enabled": ["a", "b", "api"]});
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
    assert_eq!(s.source, Source::Custom);
    assert_eq!(
        serde_json::to_value(&s.custom).expect("json")["order"],
        json!(["a", "b", "api"]),
        "新勾的按量号挪末尾"
    );
    answer_session_set_with(&ctx, &json!({"sids": ["s-1"], "rotation": "follow"}), now())
        .expect("ok");
    let s = ctx.hop.store.now().sessions["s-1"].clone();
    assert!(
        s.source == Source::Follow && s.custom.is_some(),
        "切回跟随，自己那一份留着"
    );
    answer_session_set_with(&ctx, &json!({"sids": ["s-1"], "rotation": "custom"}), now())
        .expect("ok");
    assert_eq!(source_of(&ctx, "s-1"), Source::Custom);
    let bad =
        json!({"sids": ["s-1"], "rotation": {"custom": {"order": ["b", "b"], "enabled": []}}});
    let before = ctx.hop.store.now();
    assert_eq!(
        answer_session_set_with(&ctx, &bad, now())
            .expect_err("应拒")
            .code,
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

/// 重启换那一格：`session-restart` 的几种结局 ⇒ 成（带终端）· 没成（码原样 ＋ 旧会话在不在，由 `data.stopped` 说）。
fn restart_cases() -> Vec<(&'static str, Value)> {
    let ok = |started: &str| {
        Ok(json!({"compact": "skipped", "started": started, "terminal": "proj-cc", "account": "b"}))
    };
    let err = |code: &str, data: Option<Value>| Err((code.to_string(), "那一句".to_string(), data));
    [
        ("arrived", ok("arrived")),
        ("missed", ok("missed")),
        (
            "account_unavailable",
            err("account_unavailable", Some(json!({"requested": "b"}))),
        ),
        ("not_in_terminal", err("not_in_terminal", None)),
        (
            "stop_failed",
            err("stop_failed", Some(json!({"why": "wrong_owner"}))),
        ),
        (
            "live_before",
            err("session_already_live", Some(json!({"pids": [11]}))),
        ),
        (
            "live_after",
            err(
                "session_already_live",
                Some(json!({"pids": [13], "stopped": true})),
            ),
        ),
        (
            "start_failed",
            err(
                "start_failed",
                Some(json!({"terminal": "proj-cc", "why": "x", "stopped": true})),
            ),
        ),
    ]
    .into_iter()
    .map(|(k, r)| (k, serde_json::to_value(restart_outcome(&r)).unwrap()))
    .collect()
}

#[test]
fn a_restart_says_whether_the_old_session_is_still_there() {
    let got: std::collections::BTreeMap<_, _> = restart_cases().into_iter().collect();
    let failed = |code: &str, old: &str| json!({"state": "failed", "code": code, "old": old});
    assert_eq!(
        got["arrived"],
        json!({"state": "done", "terminal": "proj-cc"})
    );
    assert_eq!(got["missed"], failed("notArrived", "ended"));
    assert_eq!(
        got["account_unavailable"],
        failed("account_unavailable", "kept")
    );
    assert_eq!(got["not_in_terminal"], failed("not_in_terminal", "kept"));
    assert_eq!(got["stop_failed"], failed("stop_failed", "kept"));
    assert_eq!(got["live_before"], failed("session_already_live", "kept"));
    assert_eq!(got["live_after"], failed("session_already_live", "ended"));
    assert_eq!(got["start_failed"], failed("start_failed", "ended"));
}

/// 跨语言金样：重启换那几格（TS `decodeRestartOutcomes` 读同一份）。`CCM_BLESS=1` 重写。
#[test]
fn restart_outcomes_match_the_cross_language_golden() {
    let got = Value::Object(
        restart_cases()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    );
    let got = json!({ "sessions": got });
    let path = crate::guard_support::repo_root()
        .join("tests/__fixtures__/rotation-switch-restart.golden.json");
    if std::env::var_os("CCM_BLESS").is_some() {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&got).unwrap()),
        )
        .unwrap();
    }
    let golden: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("金样")).unwrap();
    assert_eq!(
        got, golden,
        "重启换那一格的形状变了：TS 解码器读同一份金样，两边一起改（CCM_BLESS=1 重写）"
    );
}

// ── 额度显示态（`quota-read` 的每条 · 没出过数的号 · 此刻可用 · 最早回来）────────────────────

fn seen(ctx: &Ctx, account: &str, used_5h: f64, refused_until: Option<u64>) {
    use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
    let now = now();
    let r = match refused_until {
        Some(t) => QuotaReading {
            status: None,
            refused: true,
            limiting: None,
            resets_at: Some(t),
            windows: Vec::new(),
            overage: None,
        },
        None => QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: Some("five_hour".into()),
            resets_at: Some(now + 3_600),
            windows: vec![QuotaWindow {
                name: "five_hour".into(),
                used: Some(used_5h),
                resets_at: Some(now + 3_600),
                warned_at: None,
            }],
            overage: None,
        },
    };
    ledger::record_seen(&ctx.hop.quota, "claude-code", account, r, now);
}

/// ★ `quota-read`：原有几格一格不动，每条加显示态；「快满」按这台默认轮换的 N（没设 80%）；
/// 没出过数的号另列；此刻可用 · 最早回来；应答里零令牌、零账号身份原值。
#[test]
fn quota_read_adds_the_display_state_and_the_machine_summary() {
    let home = Home::new("quota");
    let ctx = home.ctx();
    seen(&ctx, "a", 0.5, None);
    seen(&ctx, "b", 0.86, None);
    seen(&ctx, "api", 0.0, Some(now() + 600));
    let got = serde_json::to_value(quota_read_with(&ctx, now())).expect("json");
    let row = |acct: &str| {
        got["accounts"]
            .as_array()
            .expect("accounts")
            .iter()
            .find(|r| r["account"] == acct)
            .cloned()
            .expect("row")
    };
    let b = row("b");
    let keys: std::collections::BTreeSet<&str> = b
        .as_object()
        .expect("obj")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "agent", "account", "seenAt", "reading", "kind", "state", "stale", "limiting", "slots",
            "login", "subId", "windows"
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        (&b["kind"], &b["state"], &b["login"], &b["limiting"]),
        (&json!("sub"), &json!("near"), &json!("ok"), &json!("5h"))
    );
    assert_eq!(b["slots"][0]["pct"], 86);
    assert_eq!(b["subId"], show::sub_id_of(UUID_B));
    assert_eq!(
        (&row("a")["state"], &row("a")["login"]),
        (&json!("ok"), &json!("needsLogin")),
        "a 的配置目录没有登录"
    );
    let api = row("api");
    assert_eq!(
        (&api["kind"], &api["state"], &api["login"]),
        (&json!("api"), &json!("refused"), &json!("ok"))
    );
    assert_eq!(
        got["unseen"],
        json!([{"agent": "claude-code", "account": "c", "kind": "sub", "login": "needsLogin"}])
    );
    assert_eq!(got["usableNow"], json!(["b"]));
    assert_eq!(
        got["earliestReturn"],
        json!({"account": "api", "at": now() + 600})
    );
    let text = got.to_string();
    for secret in [UUID_B, B_TOKEN, "fake-refresh-of-b"] {
        assert!(!text.contains(secret), "{secret} 漏进了应答");
    }
    // 默认轮换改成「到 90% 换」⇒ 86% 不再算快满。
    answer_set_with(
        &ctx,
        &json!({"rotation": {"order": [{"start": true}], "enabled": [], "cap": {"*": {"5h": 90, "7d": 90}}}}),
    )
    .expect("ok");
    let got = serde_json::to_value(quota_read_with(&ctx, now())).expect("json");
    let b = got["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .find(|r| r["account"] == "b")
        .cloned()
        .expect("row");
    assert_eq!(b["state"], "ok");
}

/// ★ 同一订阅：两台各有一个号目录、身份相同 ⇒ 标识相同；身份不同 ⇒ 不同。
#[test]
fn the_same_subscription_on_two_machines_gets_the_same_id() {
    let here = Home::new("sub-here");
    let there = Home::new("sub-there");
    let c = here.root.join("accts").join("c");
    let far = (now() + 3600) * 1000;
    std::fs::write(
        c.join(".credentials.json"),
        format!("{{\"claudeAiOauth\":{{\"accessToken\":\"fake-c\",\"refreshToken\":\"fake-rc\",\"expiresAt\":{far}}}}}"),
    )
    .expect("creds");
    std::fs::write(
        c.join(".claude.json"),
        "{\"oauthAccount\":{\"accountUuid\":\"cccccccc-3333-4333-8333-cccccccccccc\"}}",
    )
    .expect("identity");
    let id = |h: &Home, acct: &str| {
        let ctx = h.ctx();
        serde_json::to_value(quota_read_with(&ctx, now())).expect("json")["unseen"]
            .as_array()
            .expect("unseen")
            .iter()
            .find(|u| u["account"] == acct)
            .map(|u| u["subId"].clone())
            .expect("unseen row")
    };
    assert_eq!(id(&here, "b"), id(&there, "b"));
    assert!(id(&here, "b").is_string());
    assert_ne!(id(&here, "c"), id(&here, "b"));
}

/// ★ 默认规则的在用：跟随它、且此刻活着的会话（自定义的不算；已结束的另数）。
#[test]
fn followers_count_only_live_sessions_on_the_default() {
    let home = Home::new("followers");
    let ctx = home.ctx();
    for sid in ["s-1", "s-2", "s-3"] {
        home.saw(&ctx, sid);
    }
    home.end("s-2");
    answer_session_set_with(&ctx, &json!({"sids": ["s-3"], "rotation": "custom"}), now())
        .expect("ok");
    let rules = answer_rules_read_with(&ctx).expect("read");
    assert_eq!(rules["rules"][0]["users"]["follow"], 1);
    assert_eq!(rules["rules"][0]["users"]["live"], 1);
    assert_eq!(rules["rules"][0]["users"]["ended"], 1);
    // 设置里「在用」展开要列名单：活着的与已结束的各给 sid（已结束的折在「已结束 N」里）。
    assert_eq!(rules["rules"][0]["users"]["sids"], json!(["s-1"]));
    assert_eq!(rules["rules"][0]["users"]["endedSids"], json!(["s-2"]));
}

/// ★ 在用名单每个会话的状态由后端给（与主窗口标签页同一判）：运行中 · 空闲 · 在等你（等批准 / 等回答 / 判不出）· 已结束；
/// 活着却说不清在干什么 ⇒ 运行中（主窗口那颗点同样画成在跑）。
#[test]
fn rule_users_carry_each_sessions_state() {
    use crate::agents::SessionActivity as A;
    use crate::observe::facts_query::NeedsKind as K;
    let home = Home::new("doing");
    let ctx = home.ctx();
    for sid in [
        "s-run", "s-idle", "s-ask", "s-wait", "s-quiet", "s-bg", "s-gone",
    ] {
        home.saw(&ctx, sid);
    }
    home.end("s-gone");
    {
        let mut d = home.doing.lock().expect("lock");
        let at = |activity, needs| Doing {
            activity: Some(activity),
            needs,
        };
        d.insert("s-run".into(), at(A::Working, None));
        d.insert("s-idle".into(), at(A::Idle, None));
        d.insert("s-ask".into(), at(A::NeedsYou, Some(K::Approve)));
        d.insert("s-wait".into(), at(A::NeedsYou, None));
        // 已结束的会话 pidfile 还留着说「在跑」：已结束为准。
        d.insert("s-gone".into(), at(A::Working, None));
        d.insert("s-bg".into(), at(A::BackgroundWork, None));
    }
    let rules = answer_rules_read_with(&ctx).expect("read");
    let doing = &rules["rules"][0]["users"]["doing"];
    let t = |k: &str| copy_core::copy_text(k, &[]);
    // 字与语气与主窗口同一处写（`wire::activity_cells` · `needs_words` · 去向的字）：界面照抄，不按 `state` 取字。
    // `state` 是轮换那一侧的判（后台命令在跑按「在跑」算：重启会掐掉它），显示的字照 activity 来。
    assert_eq!(
        doing,
        &json!({
            "s-run": {"state": "working", "needs": null, "text": t("beSession.activity.working"), "tone": "now"},
            "s-idle": {"state": "idle", "needs": null, "text": t("beSession.activity.idle"), "tone": "plain"},
            "s-ask": {"state": "needsYou", "needs": "approve", "text": t("beSession.needs.approve"), "tone": "need"},
            "s-wait": {"state": "needsYou", "needs": "unknown", "text": t("beSession.needs.unknown"), "tone": "need"},
            "s-quiet": {"state": "working", "needs": null, "text": t("beSession.activity.unclear"), "tone": "now"},
            "s-bg": {"state": "working", "needs": null, "text": t("beSession.activity.backgroundWork"), "tone": "busy"},
            "s-gone": {"state": "ended", "needs": null, "text": t("sessionState.ended.name"), "tone": "plain"},
        }),
        "{rules}"
    );
}

/// ★ 会话已结束 ⇒ `inPlace = ended`，不重启换跳过它（`skipped{ended}`）。
#[test]
fn an_ended_session_says_so_and_is_not_hot_switched() {
    let home = Home::new("ended");
    let ctx = home.ctx();
    home.saw(&ctx, "s-1");
    home.end("s-1");
    let got = answer_session_read_with(&ctx, &json!({"sids": ["s-1"]}), now()).expect("ok");
    assert_eq!(got["sessions"]["s-1"]["account"]["inPlace"], "ended");
    assert_eq!(
        hot_one(&ctx, "s-1", "b", now()),
        SwitchOutcome::Skipped {
            code: "ended".into()
        }
    );
    assert_eq!(ctx.hop.store.now().sessions["s-1"].current, "a");
}

/// ★ 会话那一份带它此刻那个号的显示态，「快满」按这个会话的 N（跟随 ⇒ 默认的；没设 80%）。
#[test]
fn a_session_shows_its_current_account_by_its_own_n() {
    let home = Home::new("session-n");
    let ctx = home.ctx();
    home.saw(&ctx, "s-1");
    seen(&ctx, "a", 0.86, None);
    let state = |ctx: &Ctx| {
        answer_session_read_with(ctx, &json!({"sids": ["s-1"]}), now()).expect("ok")["sessions"]
            ["s-1"]["quota"]["state"]
            .clone()
    };
    assert_eq!(state(&ctx), "near");
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-1"], "rotation": {"custom": {"order": [{"start": true}], "enabled": [], "cap": {"*": {"5h": 90, "7d": 90}}}}}),
        now(),
    )
    .expect("ok");
    assert_eq!(state(&ctx), "ok");
}

/// ★ 按量号这台 key 表里没有它 ⇒ `login = needsKey`，绝不说 `needsLogin`（那是订阅号「要重新登录」）。
#[test]
fn an_api_account_without_a_key_needs_a_key_not_a_login() {
    let home = Home::new("needs-key");
    let mut ctx = home.ctx();
    ctx.rows = Box::new(|_, _| None);
    let got = serde_json::to_value(quota_read_with(&ctx, now())).expect("json");
    let api = got["unseen"]
        .as_array()
        .expect("unseen")
        .iter()
        .find(|u| u["account"] == "api")
        .cloned()
        .expect("api 那一行");
    assert_eq!(
        (&api["kind"], &api["login"]),
        (&json!("api"), &json!("needsKey"))
    );
    let c = got["unseen"]
        .as_array()
        .expect("unseen")
        .iter()
        .find(|u| u["account"] == "c")
        .cloned()
        .expect("c 那一行");
    assert_eq!(c["login"], "needsLogin", "订阅号照旧");
}

/// ★ 会话事实补一格：换进来那一刻这个号各窗口的用量（基线）随会话持久；`rotation-session-read` 回这一段用了几个点
/// （与单段预算并排）。新三格经 `rotation-set` 整份收、读回同形。
#[test]
fn a_switch_records_the_baseline_and_the_session_read_says_how_much_this_stretch_used() {
    let home = Home::new("segment");
    let ctx = home.ctx();
    let set = answer_set_with(
        &ctx,
        &json!({"rotation": {"order": [{"start": true}, "b"], "enabled": ["b"],
                             "stint": {"b": {"5h": 5}}, "preempt": true}}),
    )
    .expect("ok");
    assert_eq!(set["rotation"]["stint"], json!({"b": {"5h": 5}}));
    assert_eq!(set["rotation"]["preempt"], true);
    home.saw(&ctx, "s-1");
    seen(&ctx, "b", 0.30, None);
    seen(&ctx, "a", 1.0, Some(now() + 600));
    assert_eq!(hot_one(&ctx, "s-1", "b", now()), SwitchOutcome::Switched);
    let s = ctx.hop.store.now().sessions["s-1"].clone();
    assert_eq!(s.baseline["5h"].used, 0.30, "换进来那一刻 b 的 5h");
    assert_eq!(
        s.blocked_above,
        ["a"],
        "换进 b 那一刻挡在前面的：被拒着的 a"
    );
    seen(&ctx, "b", 0.33, None);
    let got = answer_session_read_with(&ctx, &json!({"sids": ["s-1"]}), now()).expect("ok");
    assert_eq!(
        got["sessions"]["s-1"]["account"]["segment"],
        json!([{"w": "5h", "base": 30, "spent": 3, "stint": 5}])
    );
    assert_eq!(
        got["sessions"]["s-1"]["account"]["blockedAbove"],
        json!(["a"])
    );
    assert_eq!(
        got["sessions"]["s-1"]["quota"]["windows"][0]["key"], "5h",
        "会话那一份的显示态也照原名出窗口"
    );
    // 盘上那份重读（后端重启）：基线还在。
    let again = RotationStore::at(Some(home.root.join(rotation::FILE_NAME)));
    assert_eq!(again.now().sessions["s-1"].baseline["5h"].used, 0.30);
}

// ── 规则表：存取 · 逐格校验 · 版本 · 删 · 默认 · 链接 · 升级 ─────────────────────────────────

fn rot_json(enabled: &[&str]) -> Value {
    let mut order = vec![json!({"start": true})];
    order.extend(enabled.iter().map(|a| json!(a)));
    json!({"order": order, "enabled": enabled})
}

fn new_rule(ctx: &Ctx, name: &str, enabled: &[&str]) -> String {
    let got = answer_rule_save_with(
        ctx,
        &json!({"name": name, "rotation": rot_json(enabled)}),
        now(),
    )
    .expect("ok");
    assert_eq!(got["state"], "saved", "{got}");
    got["rule"]["id"].as_str().expect("id").to_string()
}

fn errors_of(v: &Value) -> Vec<(String, String)> {
    assert_eq!(v["state"], "refused", "{v}");
    v["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|e| {
            (
                e["cell"].as_str().expect("cell").to_string(),
                e["code"].as_str().expect("code").to_string(),
            )
        })
        .collect()
}

/// ★ 没有文件：一条「默认」规则、是默认；新建一条读得回；带对的版本改 ⇒ 版本加一；带旧版本改 ⇒ `conflict`、不写。
#[test]
fn rules_round_trip_and_a_stale_rev_is_refused() {
    let home = Home::new("rules");
    let ctx = home.ctx();
    let first = answer_rules_read_with(&ctx).expect("read");
    assert_eq!(first["rules"].as_array().expect("rules").len(), 1);
    assert_eq!(
        first["rules"][0]["name"],
        copy_text("beRotation.rule.defaultName", &[])
    );
    assert_eq!(first["rules"][0]["isDefault"], true);
    let id = new_rule(&ctx, "夜间", &["b"]);
    let read = answer_rules_read_with(&ctx).expect("read");
    let night = read["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|r| r["id"] == id.as_str())
        .cloned()
        .expect("夜间");
    assert_eq!(night["rev"], 1);
    assert_eq!(night["isDefault"], false);
    assert_eq!(night["rotation"]["enabled"], json!(["b"]));
    let ok = answer_rule_save_with(
        &ctx,
        &json!({"id": id, "name": "夜间", "rotation": rot_json(&["b", "c"]), "ifRev": 1}),
        now(),
    )
    .expect("ok");
    assert_eq!(ok["rule"]["rev"], 2);
    let before = ctx.hop.store.now();
    let stale = answer_rule_save_with(
        &ctx,
        &json!({"id": id, "name": "夜间", "rotation": rot_json(&["c"]), "ifRev": 1}),
        now(),
    )
    .expect("ok");
    assert_eq!(stale, json!({"state": "conflict", "rev": 2}));
    assert_eq!(ctx.hop.store.now(), before, "冲突一个字节不写");
    assert_eq!(
        answer_rule_save_with(
            &ctx,
            &json!({"id": "r_nothere", "name": "x", "rotation": rot_json(&[])}),
            now()
        )
        .expect_err("不在")
        .code,
        "no_such_rule"
    );
}

/// ★ 名称：空 · 超 24 字 · 与别的规则重名（去首尾空白、不分大小写）⇒ 逐格拒；改名同一套。
#[test]
fn rule_names_are_checked_by_the_backend() {
    let home = Home::new("names");
    let ctx = home.ctx();
    let id = new_rule(&ctx, "Night", &["b"]);
    let save = |name: &str| {
        answer_rule_save_with(
            &ctx,
            &json!({"name": name, "rotation": rot_json(&[])}),
            now(),
        )
        .expect("ok")
    };
    assert_eq!(errors_of(&save("  ")), [("name".into(), "empty".into())]);
    assert_eq!(errors_of(&save(" night ")), [("name".into(), "dup".into())]);
    assert_eq!(
        errors_of(&save(&"长".repeat(25))),
        [("name".into(), "tooLong".into())]
    );
    assert_eq!(save(&"长".repeat(24))["state"], "saved");
    let renamed =
        answer_rule_rename_with(&ctx, &json!({"id": id, "name": "夜间", "ifRev": 1}), now())
            .expect("ok");
    assert_eq!(renamed["rule"]["name"], "夜间");
    assert_eq!(renamed["rule"]["rev"], 2);
    assert_eq!(
        renamed["rule"]["rotation"]["enabled"],
        json!(["b"]),
        "改名不动内容"
    );
}

/// ★ `dedupe: true`（复制 · 复制到别的机器）：重名不拒，后端在名后加 ` 2` · ` 3` … 取第一个不重的；
/// 不给它照旧拒 `dup`；加了后缀超长照旧拒 `tooLong`。
#[test]
fn a_copy_gets_the_first_free_numbered_name() {
    let home = Home::new("dedupe");
    let ctx = home.ctx();
    let id = new_rule(&ctx, "夜间", &["b"]);
    let copy = |name: &str| {
        answer_rule_save_with(
            &ctx,
            &json!({"name": name, "from": id, "dedupe": true}),
            now(),
        )
        .expect("ok")
    };
    let a = copy("夜间");
    assert_eq!(a["rule"]["name"], "夜间 2", "{a}");
    assert_eq!(a["rule"]["rotation"]["enabled"], json!(["b"]), "从那条拷");
    assert_eq!(copy(" 夜间 ")["rule"]["name"], "夜间 3");
    assert_eq!(
        copy("夜间 副本")["rule"]["name"],
        "夜间 副本",
        "不重名就照原名"
    );
    assert_eq!(
        copy(&"长".repeat(24))["state"],
        "saved",
        "不重名的 24 字照存"
    );
    assert_eq!(
        errors_of(&copy(&"长".repeat(24))),
        [("name".into(), "tooLong".into())],
        "加了后缀超 24 字 ⇒ 照旧拒"
    );
    assert_eq!(
        errors_of(
            &answer_rule_save_with(&ctx, &json!({"name": "夜间", "from": id}), now()).expect("ok")
        ),
        [("name".into(), "dup".into())],
        "不给 dedupe ⇒ 照旧拒"
    );
}

/// ★ 触发那一行（`cap["*"]`）逐格判 1–99：错的那一格回 `cap.*.5h` · `cap.*.7d`，不写盘；另一格照收。
/// 落下来的值带是哪一窗（`trigger` 那一层 ＋ `w`）。
#[test]
fn the_trigger_line_is_checked_per_window() {
    let home = Home::new("trig-cells");
    let ctx = home.ctx();
    let mut r = rot_json(&["b"]);
    r["cap"] = json!({"*": {"5h": 120, "7d": 0}});
    let got = answer_rule_save_with(&ctx, &json!({"name": "x", "rotation": r}), now()).expect("ok");
    assert_eq!(
        errors_of(&got),
        [
            ("cap.*.5h".into(), "range".into()),
            ("cap.*.7d".into(), "range".into())
        ]
    );
    r["cap"] = json!({"*": {"5h": 90}});
    let got = answer_plan_with(&ctx, &json!({"rotation": r}), now()).expect("ok");
    assert_eq!(
        got["effective"]["b"]["5h"],
        json!({"v": 90, "layer": "trigger", "w": "5h", "below": {"v": 90, "layer": "trigger", "w": "5h"}}),
        "{got}"
    );
    assert_eq!(
        got["effective"]["b"]["7d"]["layer"], "none",
        "7d 空着 ⇒ 不封顶"
    );
}

/// ★ 封顶时段逐格判：重叠（含跨午夜）· 起止相同 · 时刻写错 · 上限越界；会话自己那份时段重叠同样整份拒。
#[test]
fn cap_slots_are_checked_cell_by_cell_including_overlap_across_midnight() {
    let home = Home::new("slots");
    let ctx = home.ctx();
    let mut r = rot_json(&["b"]);
    r["cap"] = json!({"b": {"*": [
        {"at": "17:00-02:00", "n": 0},
        {"at": "01:00-05:00", "n": 99},
        {"at": "06:00-06:00", "n": 50},
        {"at": "9:5-12:00", "n": 50},
        {"at": "12:00-13:00", "n": 120}
    ]}});
    let got = answer_rule_save_with(&ctx, &json!({"name": "x", "rotation": r}), now()).expect("ok");
    assert_eq!(
        errors_of(&got),
        [
            ("cap.b.*[1]".into(), "overlap".into()),
            ("cap.b.*[2]".into(), "same".into()),
            ("cap.b.*[3]".into(), "time".into()),
            ("cap.b.*[4]".into(), "range".into()),
        ]
    );
    assert_eq!(got["errors"][0]["with"], 0, "与第 1 段（0 起）重叠");
    let mut ok = rot_json(&["b"]);
    ok["cap"] =
        json!({"b": {"*": [{"at": "17:00-02:00", "n": 0}, {"at": "02:00-17:00", "n": 99}]}});
    assert_eq!(
        answer_rule_save_with(&ctx, &json!({"name": "y", "rotation": ok}), now()).expect("ok")
            ["state"],
        "saved",
        "首尾相接不算重叠"
    );
    home.saw(&ctx, "s-1");
    let mut own = rot_json(&["b"]);
    own["cap"] =
        json!({"b": {"5h": [{"at": "22:00-03:00", "n": 0}, {"at": "23:00-23:30", "n": 50}]}});
    assert_eq!(
        answer_session_set_with(
            &ctx,
            &json!({"sids": ["s-1"], "rotation": {"custom": own}}),
            now()
        )
        .expect_err("重叠应拒")
        .code,
        "bad_args"
    );
}

/// ★ 链接：会话用某条规则 ⇒ 它按那条走；改了那条 ⇒ 它立刻按新的走；指向的规则不在 ⇒ 落默认那条。
#[test]
fn a_session_on_a_rule_follows_its_edits() {
    let home = Home::new("link");
    let ctx = home.ctx();
    let id = new_rule(&ctx, "夜间", &["b"]);
    home.saw(&ctx, "s-1");
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-1"], "rotation": {"rule": id}}),
        now(),
    )
    .expect("ok");
    let pool = |ctx: &Ctx| {
        let b = ctx.hop.store.now();
        b.rotation_of(&b.sessions["s-1"]).pool("a")
    };
    assert_eq!(pool(&ctx), ["a", "b"]);
    answer_rule_save_with(
        &ctx,
        &json!({"id": id, "name": "夜间", "rotation": rot_json(&["c"])}),
        now(),
    )
    .expect("ok");
    assert_eq!(pool(&ctx), ["a", "c"], "规则改了，用它的会话下一发就按新的");
    let got = answer_session_read_with(&ctx, &json!({"sids": ["s-1"]}), now()).expect("ok");
    assert_eq!(got["sessions"]["s-1"]["source"], json!({"rule": id}));
    assert_eq!(got["sessions"]["s-1"]["ruleName"], "夜间");
    assert!(got["sessions"]["s-1"]["explain"]
        .as_str()
        .is_some_and(|e| !e.is_empty()));
    let mut b = ctx.hop.store.now();
    b.sessions.get_mut("s-1").expect("s").source = Source::Rule("r_gone".into());
    assert_eq!(
        b.rotation_of(&b.sessions["s-1"]),
        b.default_rotation(),
        "指向不在的 ⇒ 默认那条"
    );
    assert_eq!(
        answer_session_set_with(
            &ctx,
            &json!({"sids": ["s-1"], "rotation": {"rule": "r_gone"}}),
            now()
        )
        .expect_err("不在")
        .code,
        "no_such_rule"
    );
}

/// ★ 转为本会话（`detach`）照此刻生效的那条拷；`custom` 恢复上一份自己的（撤销用）。
#[test]
fn detach_copies_the_rule_and_custom_restores_the_own_one() {
    let home = Home::new("detach");
    let ctx = home.ctx();
    let id = new_rule(&ctx, "夜间", &["b"]);
    home.saw(&ctx, "s-1");
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-1"], "rotation": {"custom": rot_json(&["c"])}}),
        now(),
    )
    .expect("ok");
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-1"], "rotation": {"rule": id}}),
        now(),
    )
    .expect("ok");
    answer_session_set_with(&ctx, &json!({"sids": ["s-1"], "rotation": "custom"}), now())
        .expect("ok");
    let s = ctx.hop.store.now().sessions["s-1"].clone();
    assert_eq!(
        (s.source, s.custom.expect("own").enabled),
        (Source::Custom, vec!["c".to_string()]),
        "恢复自己那份"
    );
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-1"], "rotation": {"rule": id}}),
        now(),
    )
    .expect("ok");
    answer_session_set_with(&ctx, &json!({"sids": ["s-1"], "rotation": "detach"}), now())
        .expect("ok");
    let s = ctx.hop.store.now().sessions["s-1"].clone();
    assert_eq!(
        (s.source, s.custom.expect("own").enabled),
        (Source::Custom, vec!["b".to_string()]),
        "照规则拷"
    );
}

/// ★ 删：默认那条不许删；在用的按 `then` 落（转为本会话 ＝ 照那条拷、行为不变 · 跟随默认）。
#[test]
fn deleting_a_rule_moves_its_sessions_as_told() {
    let home = Home::new("delete");
    let ctx = home.ctx();
    let a = new_rule(&ctx, "甲", &["b"]);
    let b = new_rule(&ctx, "乙", &["c"]);
    for sid in ["s-1", "s-2"] {
        home.saw(&ctx, sid);
    }
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-1"], "rotation": {"rule": a}}),
        now(),
    )
    .expect("ok");
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["s-2"], "rotation": {"rule": b}}),
        now(),
    )
    .expect("ok");
    let def = answer_rules_read_with(&ctx).expect("read")["defaultRule"]
        .as_str()
        .expect("def")
        .to_string();
    assert_eq!(
        answer_rule_delete_with(&ctx, &json!({"ids": [def], "then": "custom"}))
            .expect_err("默认")
            .code,
        "is_default"
    );
    let got = answer_rule_delete_with(&ctx, &json!({"ids": [a], "then": "custom"})).expect("ok");
    assert_eq!(got["moved"], json!({"s-1": "custom"}));
    let s1 = ctx.hop.store.now().sessions["s-1"].clone();
    assert_eq!(
        (s1.source, s1.custom.expect("copy").enabled),
        (Source::Custom, vec!["b".to_string()])
    );
    answer_rule_delete_with(&ctx, &json!({"ids": [b], "then": "follow"})).expect("ok");
    assert_eq!(source_of(&ctx, "s-2"), Source::Follow);
    assert_eq!(
        answer_rules_read_with(&ctx).expect("read")["rules"]
            .as_array()
            .expect("rules")
            .len(),
        1
    );
}

/// ★ 设为默认：跟随默认的会话随之换；不在的规则拒。
#[test]
fn setting_the_default_moves_the_followers() {
    let home = Home::new("defset");
    let ctx = home.ctx();
    let night = new_rule(&ctx, "夜间", &["b"]);
    home.saw(&ctx, "s-1");
    let got = answer_default_set_with(&ctx, &json!({"rule": night})).expect("ok");
    assert_eq!(got, json!({"defaultRule": night, "followers": 1}));
    let b = ctx.hop.store.now();
    assert_eq!(b.rotation_of(&b.sessions["s-1"]).pool("a"), ["a", "b"]);
    assert_eq!(
        answer_default_set_with(&ctx, &json!({"rule": "r_gone"}))
            .expect_err("不在")
            .code,
        "no_such_rule"
    );
}

/// ★ 草稿逐格校验：不写盘；错的格照 `rule-save` 那一形回；对的回空。
#[test]
fn a_draft_is_checked_cell_by_cell_without_writing() {
    let home = Home::new("plan");
    let ctx = home.ctx();
    let before = ctx.hop.store.now();
    let mut r = rot_json(&["b"]);
    r["cap"] = json!({"b": {"*": [{"at": "22:00-03:00", "n": 0}, {"at": "02:00-04:00", "n": 50}]}});
    let got = answer_plan_with(&ctx, &json!({"rotation": r}), now()).expect("ok");
    assert_eq!(
        got["errors"],
        json!([{"cell": "cap.b.*[1]", "code": "overlap", "with": 0, "said": copy_core::copy_text("rot.capErr.overlap", &[("i", "1")])}])
    );
    assert_eq!(
        answer_plan_with(&ctx, &json!({"rotation": rot_json(&["b"])}), now()).expect("ok")
            ["errors"],
        json!([])
    );
    assert_eq!(
        answer_plan_with(&ctx, &json!({"rotation": {"order": 3}}), now())
            .expect_err("形状")
            .code,
        "bad_args"
    );
    assert_eq!(ctx.hop.store.now(), before, "一个字节不写");
}

/// ★ 预览：一条规则 / 一份草稿 / 一个会话此刻那一份，接下来会怎么走（`plan`）· 各号不能用的段（`lanes`）· 各格此刻取的上限与下一层（`effective`）。
/// b 全天封顶 0 ⇒ 从头就换到起始账号（`off`，不借「到 0% 换」），b 那条泳道整段 `off`；视窗写错 ⇒ `bad_args`；规则不在 ⇒ `no_such_rule`。
#[test]
fn the_plan_says_who_runs_next_and_what_each_cell_takes() {
    let home = Home::new("plan-view");
    let ctx = home.ctx();
    let t = now();
    let draft = json!({"order": ["b", {"start": true}], "enabled": ["b"], "atLimit": "continue", "cap": {"b": {"*": 0}}});
    let got = answer_plan_with(&ctx, &json!({"rotation": draft, "span": "6h"}), t).expect("ok");
    let end = t + 6 * 3600;
    assert_eq!(got["errors"], json!([]));
    assert_eq!(
        got["plan"]
            .as_array()
            .expect("plan")
            .iter()
            .map(|p| (
                p["from"].clone(),
                p["to"].clone(),
                p["account"].clone(),
                p["why"].clone()
            ))
            .collect::<Vec<_>>(),
        vec![(json!(t), json!(end), json!("0"), json!({"off": {}}))],
        "{got}"
    );
    assert!(got["plan"][0]["fromText"].is_string(), "时刻带写好的字");
    assert_eq!(got["lanes"][0]["account"], "b");
    assert_eq!(got["lanes"][0]["spans"][0]["state"], "off", "{got}");
    assert_eq!(got["lanes"][0]["spans"][0]["to"], json!(end));
    // 全部窗口那一格下面是两窗各一条线：两窗此刻各取多少、这一格不算时各取多少，各一句（后端拼）。
    let line = |w: &str, n: &str| copy_text("beRotation.eff.line", &[("w", w), ("n", n)]);
    let none = |w: &str| copy_text("beRotation.eff.none", &[("w", w)]);
    let sep = copy_text("kit.text.sep", &[]);
    let (h5, d7) = (
        copy_text("acct.slot.fiveHour", &[]),
        copy_text("acct.slot.sevenDay", &[]),
    );
    assert_eq!(
        got["effective"]["b"]["*"],
        json!({"v": 0, "layer": "all", "below": {"v": null, "layer": "none"},
               "list": ([line(&h5, "0"), line(&d7, "0")].join(&sep)),
               "belowList": ([none(&h5), none(&d7)].join(&sep))})
    );
    assert_eq!(
        got["effective"]["b"]["5h"],
        json!({"v": 0, "layer": "all", "below": {"v": 0, "layer": "all"}})
    );
    // 规则：默认那条（只有起始账号）⇒ 一整段起始账号。
    let rules = answer_rules_read_with(&ctx).expect("read");
    let id = rules["defaultRule"].as_str().expect("id");
    let got = answer_plan_with(&ctx, &json!({"rule": id}), t).expect("ok");
    assert_eq!(got["plan"].as_array().expect("plan").len(), 1);
    assert_eq!(got["plan"][0]["to"], json!(t + 12 * 3600), "缺省 12h");
    // 会话：从它此刻的号起。
    home.saw(&ctx, "s-1");
    let got = answer_plan_with(&ctx, &json!({"sid": "s-1"}), t).expect("ok");
    assert_eq!(got["plan"][0]["account"], "a");
    assert_eq!(
        answer_plan_with(&ctx, &json!({"rule": id, "span": "3h"}), t)
            .expect_err("视窗")
            .code,
        "bad_args"
    );
    assert_eq!(
        answer_plan_with(&ctx, &json!({"rule": "r_gone"}), t)
            .expect_err("不在")
            .code,
        "no_such_rule"
    );
}

/// 记一条换号（`from` → `to`，`at` 那一刻），会话此刻走 `to`。
fn switched(ctx: &Ctx, sid: &str, from: &str, to: &str, at: u64, why: SwitchWhy) {
    rotation::face_change(&ctx.hop.store, |b| {
        let s = b.sessions.get_mut(sid).expect("session");
        s.history.push(rotation::SwitchRecord {
            at,
            at_text: None,
            from: from.into(),
            to: to.into(),
            why,
            from_resets_at: None,
            from_resets_at_text: None,
        });
        s.current = to.into();
        s.since = at;
    })
    .expect("write");
}

/// ★ 时间轴那一问（`view`）：视窗前后各一截（24h ＝ 前 6h · 后 18h）· `past` 照这个会话的换号记录切段（换进那一段的原因带上）·
/// 顶行 `head`（在用哪个号 · 卡人的窗口 · 用了多少 · 到 ≥N% 还差几点）· 每条泳道此刻的用量 `pct`。
#[test]
fn the_timeline_view_looks_back_and_says_who_runs_now() {
    let home = Home::new("tl-session");
    let ctx = home.ctx();
    let t = now();
    let mut r = rot_json(&["b"]);
    r["cap"] = json!({"*": {"5h": 90, "7d": 90}});
    answer_set_with(&ctx, &json!({"rotation": r})).expect("set");
    home.saw(&ctx, "s-1");
    switched(
        &ctx,
        "s-1",
        "a",
        "b",
        t - 3600,
        SwitchWhy::Threshold {
            n: 90,
            w: Some("5h".into()),
        },
    );
    seen(&ctx, "b", 0.63, None);
    let got = answer_plan_with(&ctx, &json!({"sid": "s-1", "view": "24h"}), t).expect("ok");
    assert_eq!(got["from"], json!(t - 6 * 3600), "{got}");
    assert_eq!(got["until"], json!(t + 18 * 3600));
    assert!(got["fromText"].is_string());
    let past: Vec<_> = got["past"]
        .as_array()
        .expect("past")
        .iter()
        .map(|p| {
            (
                p["from"].clone(),
                p["to"].clone(),
                p["account"].clone(),
                p["why"].clone(),
            )
        })
        .collect();
    assert_eq!(
        past,
        vec![
            (
                json!(t - 6 * 3600),
                json!(t - 3600),
                json!("a"),
                Value::Null
            ),
            (
                json!(t - 3600),
                json!(t),
                json!("b"),
                json!({"threshold": {"n": 90, "w": "5h"}})
            ),
        ],
        "{got}"
    );
    assert_eq!(
        got["head"],
        json!({"account": "b", "w": "5h", "pct": 63, "toLine": {"w": "5h", "n": 27}}),
        "{got}"
    );
    // 刻度：24h 一格 1h（按这台本地钟对齐），每 3h 一个轴上的字（`HH:MM`）；悬停 / 键盘按格走，每格带写好的字。
    let grid = got["grid"].as_array().expect("grid");
    assert!((24..=25).contains(&grid.len()), "{got}"); // 此刻正落在整点上 ⇒ 视窗两头都是格
    assert!(grid
        .windows(2)
        .all(|w| w[1]["at"].as_u64().unwrap() - w[0]["at"].as_u64().unwrap() == 3600));
    assert!(grid.iter().all(|g| g["atText"].is_string()));
    let labels: Vec<&str> = grid.iter().filter_map(|g| g["label"].as_str()).collect();
    assert!((8..=9).contains(&labels.len()), "{got}");
    assert!(
        labels.iter().all(|l| l.len() == 5 && l.ends_with(":00")),
        "{labels:?}"
    );
    let week = answer_plan_with(&ctx, &json!({"sid": "s-1", "view": "7d"}), t).expect("ok");
    assert_eq!(week["until"], json!(t + 6 * 86_400));
    let last = week["plan"]
        .as_array()
        .expect("plan")
        .last()
        .expect("seg")
        .clone();
    assert_eq!(last["to"], json!(t + 86_400), "7d 的将来只画到 +1d：{week}");
    let wl: Vec<&str> = week["grid"]
        .as_array()
        .expect("grid")
        .iter()
        .filter_map(|g| g["label"].as_str())
        .collect();
    assert!(
        (6..=7).contains(&wl.len()) && wl.iter().all(|l| l.len() == 5 && l.as_bytes()[2] == b'-'),
        "{wl:?}"
    );
    let lane_b = got["lanes"]
        .as_array()
        .expect("lanes")
        .iter()
        .find(|l| l["account"] == "b")
        .expect("b");
    assert_eq!(lane_b["pct"], json!(63));
    // 不带 view ＝ 编辑器那一问：照旧从此刻起、没有 past。
    let got = answer_plan_with(&ctx, &json!({"sid": "s-1"}), t).expect("ok");
    assert_eq!(got["from"], json!(t));
    assert!(got.get("past").is_none(), "{got}");
    assert_eq!(
        answer_plan_with(&ctx, &json!({"sid": "s-1", "view": "12h"}), t)
            .expect_err("视窗")
            .code,
        "bad_args"
    );
}

/// ★ 卡住：池里都被拒 ⇒ 顶行换成 `head.blocked`（最早回来的那个号 · 几点 · 哪个窗口重置）。
#[test]
fn the_timeline_head_says_when_the_earliest_one_comes_back() {
    let home = Home::new("tl-blocked");
    let ctx = home.ctx();
    let t = now();
    answer_set_with(&ctx, &json!({"rotation": rot_json(&["b"])})).expect("set");
    home.saw(&ctx, "s-1");
    seen(&ctx, "a", 0.0, Some(t + 7200));
    seen(&ctx, "b", 0.0, Some(t + 3600));
    let got = answer_plan_with(&ctx, &json!({"sid": "s-1", "view": "6h"}), t).expect("ok");
    assert_eq!(got["from"], json!(t - 2 * 3600), "{got}");
    assert_eq!(got["until"], json!(t + 4 * 3600));
    assert_eq!(got["head"]["blocked"]["account"], "b", "{got}");
    assert_eq!(got["head"]["blocked"]["at"], json!(t + 3600));
    assert!(got["head"]["blocked"]["atText"].is_string());
}

/// ★ 设置里那一问（`{machine: true}`）：本机全部号各一条泳道（按默认规则判封顶）· 每号此刻有几个活着的会话在用（`usedBy`）· 没有本会话那几格。
/// quota-warm 留了状态文件、进程还活着 ⇒ 泳道带下一次开窗的时刻（`warm`）；进程没了 ⇒ 整类不给。
#[test]
fn the_machine_timeline_lists_every_account_and_who_uses_it() {
    let home = Home::new("tl-machine");
    let ctx = home.ctx();
    let t = now();
    home.saw(&ctx, "s-1");
    home.saw(&ctx, "s-2");
    home.saw(&ctx, "s-3");
    switched(&ctx, "s-3", "a", "b", t - 60, SwitchWhy::Preempt);
    home.end("s-2");
    let warm = home.root.join("quota-warm.json");
    std::fs::write(
        &warm,
        json!({"pid": std::process::id(), "next": [{"account": "b", "at": t + 1800}]}).to_string(),
    )
    .expect("warm");
    let got = answer_plan_with(&ctx, &json!({"machine": true, "view": "24h"}), t).expect("ok");
    let lanes = got["lanes"].as_array().expect("lanes");
    assert_eq!(
        lanes
            .iter()
            .map(|l| l["account"].clone())
            .collect::<Vec<_>>(),
        vec![json!("a"), json!("b"), json!("c"), json!("api")],
        "{got}"
    );
    let of = |a: &str| {
        lanes
            .iter()
            .find(|l| l["account"] == a)
            .expect("lane")
            .clone()
    };
    assert_eq!(of("a")["usedBy"], json!(1), "s-2 已结束不算");
    assert_eq!(of("b")["usedBy"], json!(1));
    assert_eq!(of("c")["usedBy"], json!(0));
    assert_eq!(
        of("b")["warm"],
        json!([{"at": t + 1800, "atText": of("b")["warm"][0]["atText"]}])
    );
    assert!(of("b")["warm"][0]["atText"].is_string());
    assert!(of("a").get("warm").is_none(), "{got}");
    assert!(
        got.get("past").is_none() && got.get("head").is_none(),
        "{got}"
    );
    std::fs::write(
        &warm,
        json!({"pid": u32::MAX - 7, "next": [{"account": "b", "at": t + 1800}]}).to_string(),
    )
    .expect("warm");
    let got = answer_plan_with(&ctx, &json!({"machine": true, "view": "24h"}), t).expect("ok");
    assert!(
        got["lanes"]
            .as_array()
            .expect("lanes")
            .iter()
            .all(|l| l.get("warm").is_none()),
        "{got}"
    );
}

/// ★ 起会话框带规则：起之前给定好的 sid 记一条、来源 ＝ 那条规则；中转随后第一次看见它（起它的号对不上也一样）⇒ 来源不动；
/// 规则不在 ⇒ `no_such_rule`；起不成撤掉（只撤还没换过号的）。
#[test]
fn a_preset_session_keeps_its_rule_when_the_relay_first_sees_it() {
    let home = Home::new("preset");
    let ctx = home.ctx();
    let night = new_rule(&ctx, "夜间", &["b"]);
    let sid = crate::accounts::quota::rotation::new_session_id();
    assert!(shell_quote_core::session_id_ok(&sid), "{sid}");
    assert_ne!(
        sid,
        crate::accounts::quota::rotation::new_session_id(),
        "每次一个新的"
    );
    preset_with(&ctx, &sid, "claude", &night, now()).expect("preset");
    home.saw(&ctx, &sid);
    assert_eq!(source_of(&ctx, &sid), Source::Rule(night.clone()));
    assert_eq!(
        preset_with(&ctx, "s-x", "claude", "r_gone", now())
            .expect_err("不在")
            .0,
        "no_such_rule"
    );
    forget_preset_with(&ctx, &sid);
    assert!(!ctx.hop.store.now().sessions.contains_key(&sid));
}

/// ★ `session-new` 上了 CLI 面（第二个前端经一次性 `ccm -- --session-new` 调）：带规则那一臂在一次性进程里答得真 ——
/// 规则表与先定的那一条都在盘上（跨进程锁 ＋ 盘上动过就重读），不读常驻进程的内存。
/// 两份各自新建的 `Ctx` ＝ 常驻那一个与一次性那一个：常驻存的规则一次性认得；一次性记的那一条常驻读得到；
/// 一次性起不成撤掉，常驻那边也跟着没了。
#[test]
fn a_one_shot_preset_is_seen_by_the_resident_process() {
    let home = Home::new("preset-cli");
    let resident = home.ctx();
    let night = new_rule(&resident, "夜间", &["b"]);
    let _ = resident.hop.store.now();
    let one_shot = home.ctx();
    let sid = crate::accounts::quota::rotation::new_session_id();
    preset_with(&one_shot, &sid, "claude", &night, now()).expect("一次性那一个认得常驻存的规则");
    assert_eq!(
        resident
            .hop
            .store
            .now()
            .sessions
            .get(&sid)
            .map(|s| s.source.clone()),
        Some(Source::Rule(night.clone())),
        "常驻那一个读不到一次性记的那一条"
    );
    forget_preset_with(&home.ctx(), &sid);
    assert!(
        !resident.hop.store.now().sessions.contains_key(&sid),
        "一次性撤掉的那一条常驻还看得见"
    );
}

/// 读不懂的 rotation.json 在各读答里是成功应答的一格：`reason` 只是那一句（不带解析器原话），原话进 `detail`（复制详情，命令名 · 码 `unreadable`）；
/// 往读不懂的那份写（存规则）不覆盖，失败带原话（进应答详情）。
#[test]
fn an_unreadable_rotation_file_answers_a_sentence_and_a_detail() {
    let home = Home::new("unreadable");
    let ctx = home.ctx();
    std::fs::write(home.root.join(rotation::FILE_NAME), b"{not json").expect("write");
    let raw = "key must be a string";
    let reads = [
        (
            "rotation-rules-read",
            answer_rules_read_with(&ctx).expect("read"),
        ),
        (
            "rotation-session-read",
            answer_session_read_with(&ctx, &json!({"sids": ["s-1"]}), now()).expect("read"),
        ),
        (
            "rotation-plan",
            answer_plan_with(&ctx, &json!({"machine": true}), now()).expect("plan"),
        ),
    ];
    for (cmd, v) in &reads {
        assert_eq!(v["state"], "unreadable", "{cmd}: {v}");
        let reason = v["reason"]
            .as_str()
            .unwrap_or_else(|| panic!("{cmd} 没有 reason：{v}"));
        assert!(!reason.contains(raw), "{cmd}: 原话进了那一句：{reason}");
        let detail = v["detail"]
            .as_str()
            .unwrap_or_else(|| panic!("{cmd} 没有 detail：{v}"));
        assert!(
            detail.contains(cmd) && detail.contains("unreadable") && detail.contains(raw),
            "{cmd}: {detail}"
        );
    }
    let fail = answer_rule_save_with(&ctx, &json!({"name": "night", "from": "blank"}), now())
        .expect_err("读不懂的那份不覆盖");
    assert_eq!(fail.code, "io_failed");
    assert!(!fail.message.contains(raw), "{}", fail.message);
    assert!(
        fail.raw.as_deref().is_some_and(|r| r.contains(raw)),
        "{:?}",
        fail.raw
    );
    assert_eq!(
        std::fs::read(home.root.join(rotation::FILE_NAME)).expect("read"),
        b"{not json"
    );
    let fresh = Home::new("readable");
    let ok = answer_rules_read_with(&fresh.ctx()).expect("read");
    assert!(ok["detail"].is_null(), "不在那一形多出了详情：{ok}");
    let _ = std::fs::remove_dir_all(&home.root);
    let _ = std::fs::remove_dir_all(&fresh.root);
}

/// b 的 5h 窗口在 `at` 时用了 `used`（重置在 `resets`）。
fn seen_at(ctx: &Ctx, account: &str, used: f64, at: u64, resets: u64) {
    use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
    let r = QuotaReading {
        status: Some(QuotaStatus::Allowed),
        refused: false,
        limiting: Some("five_hour".into()),
        resets_at: Some(resets),
        windows: vec![QuotaWindow {
            name: "five_hour".into(),
            used: Some(used),
            resets_at: Some(resets),
            warned_at: None,
        }],
        overage: None,
    };
    ledger::record_seen(&ctx.hop.quota, "claude-code", account, r, at);
}

/// ★ 顶行的「估」：此刻的号这一窗有两次不同的采样、最近 30 分钟在涨 ⇒ `est{at, pct, w}`，到的是这号这窗口此刻取的上限
/// （封顶 → 触发 → 满）；只有一次采样 ⇒ 不给。
#[test]
fn the_timeline_head_estimates_when_the_account_reaches_its_limit_only_with_grounds() {
    let home = Home::new("tl-est");
    let ctx = home.ctx();
    let t = now();
    let mut r = rot_json(&["b"]);
    r["cap"] = json!({"*": {"5h": 90, "7d": 90}});
    answer_set_with(&ctx, &json!({"rotation": r.clone()})).expect("set");
    home.saw(&ctx, "s-1");
    switched(
        &ctx,
        "s-1",
        "a",
        "b",
        t - 3600,
        SwitchWhy::Threshold {
            n: 90,
            w: Some("5h".into()),
        },
    );
    let resets = t + 5 * 3600;
    seen_at(&ctx, "b", 0.40, t - 900, resets);
    let ask = json!({"sid": "s-1", "view": "24h"});
    let got = answer_plan_with(&ctx, &ask, t).expect("ok");
    assert!(
        got["head"].get("est").is_none(),
        "只有一次采样就给了估：{got}"
    );
    seen_at(&ctx, "b", 0.46, t - 300, resets);
    let got = answer_plan_with(&ctx, &ask, t).expect("ok");
    // 600 秒涨 6 点 ⇒ 到 90% 还差 44 点 ＝ 4400 秒。
    let est = &got["head"]["est"];
    assert_eq!(
        (est["at"].clone(), est["pct"].clone(), est["w"].clone()),
        (json!(t - 300 + 4_400), json!(90), json!("5h")),
        "{got}"
    );
    assert!(est["atText"].is_string());
    r["cap"] = json!({"b": {"5h": 70}});
    answer_set_with(&ctx, &json!({"rotation": r})).expect("set");
    let got = answer_plan_with(&ctx, &ask, t).expect("ok");
    let est = &got["head"]["est"];
    assert_eq!(
        (est["at"].clone(), est["pct"].clone()),
        (json!(t - 300 + 2_400), json!(70)),
        "到的应是封顶：{got}"
    );
    assert!(
        answer_plan_with(&ctx, &ask, t + 1_800).expect("ok")["head"]
            .get("est")
            .is_none(),
        "涨在 30 分钟以前还给估"
    );
}

fn seed_parent(ctx: &Ctx, kid: &str, parent: &str) {
    crate::lineage::relay_saw(
        &ctx.lineage,
        Some(crate::lineage::Origin {
            token: "0a1b2c3d4e5f6071",
            parent: Some(parent),
        }),
        kid,
        "claude-code",
        now(),
    );
}

/// ★ 跟随父会话：`"parent"` 按血缘填父、生效那份 ＝ 父的；没有父 ⇒ `no_parent`、一个字节不写；`detach` 照父此刻那份拷。
#[test]
fn following_the_parent_is_set_from_lineage_and_refused_without_one() {
    let home = Home::new("parent-set");
    let ctx = home.ctx();
    let id = new_rule(&ctx, "夜间", &["b"]);
    home.saw(&ctx, "p-1");
    home.saw(&ctx, "k-1");
    answer_session_set_with(
        &ctx,
        &json!({"sids": ["p-1"], "rotation": {"rule": id}}),
        now(),
    )
    .expect("ok");
    let before = ctx.hop.store.now();
    let e = answer_session_set_with(&ctx, &json!({"sids": ["k-1"], "rotation": "parent"}), now())
        .expect_err("没有父却收了");
    assert_eq!(e.code, "no_parent");
    assert_eq!(ctx.hop.store.now(), before, "拒了还写了");
    seed_parent(&ctx, "k-1", "p-1");
    answer_session_set_with(&ctx, &json!({"sids": ["k-1"], "rotation": "parent"}), now())
        .expect("ok");
    let b = ctx.hop.store.now();
    assert_eq!(b.sessions["k-1"].source, Source::Parent("p-1".into()));
    assert_eq!(b.rule_of(&b.sessions["k-1"]), Some(id.as_str()));
    answer_session_set_with(&ctx, &json!({"sids": ["k-1"], "rotation": "detach"}), now())
        .expect("ok");
    let s = ctx.hop.store.now().sessions["k-1"].clone();
    assert_eq!(
        (s.source, s.custom.expect("own").enabled),
        (Source::Custom, vec!["b".to_string()]),
        "转为本会话没照父此刻那份拷"
    );
}

/// ★ 读：血缘里有父就给 `parent`（不管来源是不是跟随它）；跟随父会话而父不在账本里 ⇒ `parentMissing`、规则名是默认那条。
#[test]
fn reading_a_session_says_its_parent_and_whether_the_parent_is_missing() {
    let home = Home::new("parent-read");
    let ctx = home.ctx();
    home.saw(&ctx, "k-1");
    seed_parent(&ctx, "k-1", "p-gone");
    let got = answer_session_read_with(&ctx, &json!({"sids": ["k-1"]}), now()).expect("ok");
    let k = &got["sessions"]["k-1"];
    assert_eq!(k["parent"], "p-gone");
    assert!(
        k.get("parentMissing").is_none(),
        "来源不是跟随父会话也说父不在"
    );
    rotation::face_change(&ctx.hop.store, |b| {
        b.sessions.get_mut("k-1").expect("k").source = Source::Parent("p-gone".into());
    })
    .expect("write");
    let got = answer_session_read_with(&ctx, &json!({"sids": ["k-1"]}), now()).expect("ok");
    let k = &got["sessions"]["k-1"];
    assert_eq!(k["source"], json!({"parent": "p-gone"}));
    assert_eq!(k["parentMissing"], true);
    assert_eq!(
        k["ruleName"],
        json!(ctx.hop.store.now().rules[&ctx.hop.store.now().default_rule].name)
    );
}

// ── 应答类型的样本（`every_command_declares_exactly_the_fields_it_puts_out` 读；每一支一个，可缺的格都填上）──

use crate::guard_support::Shaped;

fn cell_error() -> CellError {
    CellError {
        cell: "order".into(),
        code: "empty".into(),
        with: Some(1),
        said: "said".into(),
    }
}

impl Shaped for RulesRead {
    fn samples() -> Vec<Self> {
        vec![RulesRead {
            state: "unreadable",
            reason: json!("why"),
            detail: json!("detail lines"),
            path: Some("/x/rotation.json".into()),
            default_rule: "default".into(),
            rules: vec![json!({"id": "default"})],
        }]
    }
}

impl Shaped for RuleSaved {
    fn samples() -> Vec<Self> {
        vec![
            RuleSaved::Saved {
                rule: json!({"id": "r1"}),
            },
            RuleSaved::Refused {
                errors: vec![cell_error()],
            },
            RuleSaved::Conflict { rev: 3 },
        ]
    }
}

impl Shaped for RuleDeleted {
    fn samples() -> Vec<Self> {
        let mut moved = Map::new();
        moved.insert("s-1".into(), json!("follow"));
        vec![RuleDeleted { moved }]
    }
}

impl Shaped for DefaultSet {
    fn samples() -> Vec<Self> {
        vec![DefaultSet {
            default_rule: "r1".into(),
            followers: 2,
        }]
    }
}

impl Shaped for SessionRead {
    fn samples() -> Vec<Self> {
        let mut sessions = Map::new();
        sessions.insert("s-1".into(), json!({}));
        vec![SessionRead {
            state: "present",
            reason: Value::Null,
            detail: Value::Null,
            now: 1,
            sessions,
        }]
    }
}

impl Shaped for SessionSet {
    fn samples() -> Vec<Self> {
        let mut sessions = Map::new();
        sessions.insert("s-1".into(), json!({"state": "done"}));
        vec![SessionSet { sessions }]
    }
}

impl Shaped for PlanReply {
    fn samples() -> Vec<Self> {
        vec![
            PlanReply::Draft {
                errors: vec![cell_error()],
            },
            PlanReply::Plan(Box::new(Plan {
                errors: Vec::new(),
                state: "present",
                reason: Value::Null,
                detail: Value::Null,
                now: 1,
                now_text: "00:00".into(),
                from: 1,
                from_text: "00:00".into(),
                until: 2,
                plan: vec![json!({})],
                lanes: vec![json!({})],
                effective: Map::new(),
                grid: Some(json!([])),
                head: Some(json!({})),
                past: Some(vec![json!({})]),
            })),
        ]
    }
}

impl Shaped for QuotaRead {
    fn samples() -> Vec<Self> {
        vec![QuotaRead {
            state: "present",
            reason: Value::Null,
            detail: Value::Null,
            path: Some("/x/quota.json".into()),
            now: 1,
            accounts: vec![json!({})],
            unseen: vec![json!({})],
            usable_now: vec!["a".into()],
            earliest_return: Some(EarliestReturn {
                account: "b".into(),
                at: 2,
            }),
            text: Some("t".into()),
        }]
    }
}

/// ★★ 每一格错都带写好的那一句 `said`（界面照抄，不再按码取字）：名称的空 · 超长 · 重名，封顶时段的重叠（带「第几段」）· 起止相同 · 时刻 · 越界。
#[test]
fn every_cell_error_carries_its_written_sentence() {
    let home = Home::new("said");
    let ctx = home.ctx();
    let said_of = |v: &Value| -> Vec<String> {
        v["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .map(|e| e["said"].as_str().unwrap_or("<缺 said>").to_string())
            .collect()
    };
    let mut r = rot_json(&["b"]);
    r["cap"] = json!({"b": {"*": [
        {"at": "17:00-02:00", "n": 0},
        {"at": "01:00-05:00", "n": 99},
        {"at": "06:00-06:00", "n": 50},
        {"at": "9:5-12:00", "n": 50},
        {"at": "12:00-13:00", "n": 120}
    ]}});
    let got = answer_rule_save_with(&ctx, &json!({"name": "x", "rotation": r}), now()).expect("ok");
    assert_eq!(
        said_of(&got),
        [
            copy_core::copy_text("rot.capErr.overlap", &[("i", "1")]),
            copy_core::copy_text("rot.capErr.same", &[]),
            copy_core::copy_text("rot.capErr.time", &[]),
            copy_core::copy_text("rot.capErr.range", &[]),
        ]
    );
    let blank = answer_rule_save_with(
        &ctx,
        &json!({"name": "  ", "rotation": rot_json(&["b"])}),
        now(),
    )
    .expect("ok");
    assert_eq!(
        said_of(&blank),
        [copy_core::copy_text("rot.save.empty", &[])]
    );
    let long = "x".repeat(rotation::RULE_NAME_MAX + 1);
    let got = answer_rule_save_with(
        &ctx,
        &json!({"name": long, "rotation": rot_json(&["b"])}),
        now(),
    )
    .expect("ok");
    assert_eq!(
        said_of(&got),
        [copy_core::copy_text("rot.save.tooLong", &[])]
    );
}
