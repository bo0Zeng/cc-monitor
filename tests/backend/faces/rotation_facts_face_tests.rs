//! `rotation-facts`：每个号此刻的事实，逐字段对得上；入参多一格 ⇒ `bad_args`。临时家目录、假凭据、不起中转。

use super::*;
use crate::accounts::quota::facts::{FactBlocked, FactWhy, WinFact};
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::quota::show::LoginState;
use crate::accounts::upstream_select::rotate::{Hop, LibAccount, Library};
use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
use std::sync::Arc;

const NOW: u64 = 4_000_000_000;

fn home(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ccm-rotfacts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let b = root.join("accts").join("b");
    std::fs::create_dir_all(&b).expect("mkdir");
    std::fs::write(
        b.join(".credentials.json"),
        "{\"claudeAiOauth\":{\"accessToken\":\"fake\",\"refreshToken\":\"fake-r\",\"expiresAt\":9999999999999}}",
    )
    .expect("creds");
    std::fs::write(
        b.join(".claude.json"),
        "{\"oauthAccount\":{\"accountUuid\":\"bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb\"}}",
    )
    .expect("identity");
    std::fs::create_dir_all(root.join("accts").join("c")).expect("mkdir");
    root
}

/// 账号库：b（有登录）· c（没登录）· api（按量号，key 表里有）。
fn ctx(root: &std::path::Path) -> crate::faces::rotation_face::Ctx {
    let accts = root.join("accts");
    crate::faces::rotation_face::Ctx {
        setup: Arc::new(crate::accounts::quota::setup::uses::Setup::of(Arc::new(
            RotationStore::at(Some(root.join(rotation::FILE_NAME))),
        ))),
        hop: Hop::new(
            Arc::new(RotationStore::at(Some(root.join(rotation::FILE_NAME)))),
            &crate::faces::rotation_face::adoptions_at(),
            Arc::new(Ledger::at(Some(root.join(ledger::FILE_NAME)))),
            Some(root.to_path_buf()),
            Arc::new(move |_| Library {
                enabled: true,
                accounts: [("b", false), ("c", false), ("api", true)]
                    .iter()
                    .map(|(id, api)| LibAccount {
                        id: id.to_string(),
                        dir: Some(accts.join(id)),
                        api: *api,
                    })
                    .collect(),
            }),
            Arc::new(Default::default),
            None,
        ),
        rows: Box::new(|_, a| (a == "api").then_some(true)),
    }
}

fn reading(used_5h: f64, refused: bool) -> QuotaReading {
    QuotaReading {
        status: Some(if refused {
            QuotaStatus::Rejected
        } else {
            QuotaStatus::Allowed
        }),
        refused,
        limiting: Some("five_hour".into()),
        resets_at: Some(NOW + 3_600),
        windows: vec![QuotaWindow {
            name: "five_hour".into(),
            used: Some(used_5h),
            resets_at: Some(NOW + 3_600),
            warned_at: None,
        }],
        overage: None,
    }
}

fn closed() -> WinFact {
    WinFact {
        open: false,
        used: 0.0,
        resets: None,
        resets_text: None,
        rate: None,
        tokens_per_pct: None,
    }
}

/// ★ 三个号各一种：b 有登录、5 小时用了 42%；c 没登录；api 按量号。号照账号库的先后。
#[test]
fn each_account_comes_back_with_its_facts_field_for_field() {
    let root = home("fields");
    let c = ctx(&root);
    let own = crate::platform::fs::Own::issue_data_at(&root);
    ledger::record_seen(
        &c.hop.quota,
        &own,
        None,
        "claude-code",
        "b",
        reading(0.42, false),
        NOW,
    );
    let got = facts_with(
        &c,
        &RotationFactsReq {
            agent: Some(crate::agents::AgentKind::new(
                crate::agents::claudecode::AGENT_KIND,
            )),
        },
        NOW,
        &crate::Tz::default(),
    )
    .expect("认得的 kind");
    assert_eq!(got.now, NOW);
    let names: Vec<&str> = got.accounts.iter().map(|f| f.account.as_str()).collect();
    assert_eq!(names, ["b", "c", "api"]);
    let b = &got.accounts[0];
    assert_eq!(
        (b.kind, b.login, b.seen_at, &b.blocked),
        (
            crate::accounts::quota::facts::Kind::Sub,
            LoginState::Ok,
            Some(NOW),
            &None
        )
    );
    assert!(b.w5h.open);
    assert!((b.w5h.used - 42.0).abs() < 1e-9, "{}", b.w5h.used);
    assert_eq!(b.w5h.resets, Some(NOW + 3_600));
    assert!(b.w5h.resets_text.is_some(), "重置时刻带写好的字");
    assert_eq!(b.w7d, closed());
    let cc = &got.accounts[1];
    assert_eq!(cc.login, LoginState::NeedsLogin);
    assert_eq!(
        cc.blocked,
        Some(FactBlocked {
            why: FactWhy::Login,
            until: None,
            text: copy_core::copy_text("beRotation.fact.login", &[]),
        })
    );
    let api = &got.accounts[2];
    assert_eq!(
        (api.kind, api.login, &api.blocked),
        (
            crate::accounts::quota::facts::Kind::Api,
            LoginState::Ok,
            &None
        )
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// ★ 被拒：原因 `refused`、几点恢复、那一句带写好的时刻；用到 100% ⇒ `used`。
#[test]
fn a_refused_or_used_up_account_says_why_and_until_when() {
    let root = home("blocked");
    let c = ctx(&root);
    let own = crate::platform::fs::Own::issue_data_at(&root);
    ledger::record_seen(
        &c.hop.quota,
        &own,
        None,
        "claude-code",
        "b",
        reading(1.02, true),
        NOW,
    );
    let got = facts_with(&c, &RotationFactsReq::default(), NOW, &crate::Tz::default()).unwrap();
    let b = got.accounts[0].blocked.clone().expect("被拒");
    assert_eq!((b.why, b.until), (FactWhy::Refused, Some(NOW + 3_600)));
    let at = crate::common::time::fmt_at((NOW + 3_600) as i64, NOW as i64, &crate::Tz::default());
    assert_eq!(
        b.text,
        copy_core::copy_text("beRotation.fact.refused", &[("at", &at)])
    );
    ledger::record_seen(
        &c.hop.quota,
        &own,
        None,
        "claude-code",
        "b",
        reading(1.0, false),
        NOW + 1,
    );
    let got = facts_with(
        &c,
        &RotationFactsReq::default(),
        NOW + 1,
        &crate::Tz::default(),
    )
    .unwrap();
    let b = got.accounts[0].blocked.clone().expect("用满");
    assert_eq!(b.why, FactWhy::Used);
    let _ = std::fs::remove_dir_all(&root);
}

/// ★ 登记成 typed：入参多一格 ⇒ `bad_args`，不往下走。
#[test]
fn it_is_registered_typed_and_refuses_fields_it_does_not_use() {
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "rotation-facts")
        .expect("登记了");
    assert!(
        serde_json::from_value::<RotationFactsReq>(serde_json::json!({"sid": "x"})).is_err(),
        "多一格该拒（解入参那一步回 bad_args）"
    );
    assert_eq!(spec.codes, ["bad_args"]);
}

/// ★ 线上只认 kind：后端里面记账用的路由名当 `agent` 交来 ⇒ `bad_args`，不当成那一家（轮换那两条同一个认法）。
#[test]
fn a_route_name_is_not_taken_as_an_agent() {
    let route = crate::agents::claudecode::UPSTREAM.route_id;
    assert_ne!(route, crate::agents::claudecode::AGENT_KIND);
    for (cmd, args) in [
        ("rotation-facts", serde_json::json!({ "agent": route })),
        (
            "rotation-try",
            serde_json::json!({ "source": "", "agent": route }),
        ),
    ] {
        let fail = crate::wire::typed::tests::ask(cmd, args).expect_err(cmd);
        assert_eq!(fail.code, "bad_args", "{cmd}：{}", fail.message);
    }
}
