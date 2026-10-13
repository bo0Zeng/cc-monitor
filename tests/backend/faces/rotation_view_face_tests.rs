//! 轮换的读（第五版）：策略目录 · 一个会话 / 一份策略此刻的四块 · `quota-read`（显示态）。临时家目录、假凭据、不起中转。
//! 这里的 [`Home`] 也给写那一组（`rotation_edit_face_tests`）用。

use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::upstream_select::rotate::{Hop, LibAccount, Library};
use crate::faces::rotation_face::{forget_preset_with, preset_with, quota_read_with};
use serde_json::json;
use std::sync::Arc;

pub(crate) const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
pub(crate) const B_TOKEN: &str = "fake-access-of-b";
const B_REFRESH: &str = "fake-refresh-of-b";
pub(crate) const S1: &str = "11111111-1111-4111-8111-111111111111";
pub(crate) const S2: &str = "22222222-2222-4222-8222-222222222222";

pub(crate) fn now() -> u64 {
    crate::accounts::quota::now_unix()
}

fn dead_token_endpoint() -> crate::accounts::oauth::TokenEndpoint {
    let port = crate::refusing_port::refusing_port();
    crate::accounts::oauth::TokenEndpoint {
        base: crate::relay::Base::parse(&format!("http://127.0.0.1:{port}")).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_millis(2_000),
    }
}

/// 一个家：账号库 a（开会话的号）· b（有登录）· c（没登录）· api（按量号，key 表里有）；策略目录里放三份判据夹具
/// （`by-list` · `by-reset` · `third`，用户自己的），新会话用 `by-list`。
pub(crate) struct Home {
    pub(crate) root: std::path::PathBuf,
    pub(crate) live: Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Home {
    pub(crate) fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotface5-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let far = (now() + 3600) * 1000;
        for (id, uuid) in [("a", "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa"), ("b", UUID_B)] {
            let d = root.join("accts").join(id);
            std::fs::create_dir_all(&d).expect("mkdir");
            std::fs::write(
                d.join(".credentials.json"),
                format!("{{\"claudeAiOauth\":{{\"accessToken\":\"fake-access-of-{id}\",\"refreshToken\":\"fake-refresh-of-{id}\",\"expiresAt\":{far}}}}}"),
            )
            .expect("creds");
            std::fs::write(
                d.join(".claude.json"),
                format!("{{\"oauthAccount\":{{\"accountUuid\":\"{uuid}\"}}}}"),
            )
            .expect("identity");
        }
        std::fs::create_dir_all(root.join("accts").join("c")).expect("mkdir");
        let h = Self {
            root,
            live: Arc::default(),
        };
        let read = |p: &str| {
            std::fs::read_to_string(crate::guard_support::repo_root().join(p)).expect("判据夹具")
        };
        let list = read("tests/__fixtures__/rotation-by-list.rhai");
        let third = list.replace("\"name\": \"照号牌\"", "\"name\": \"第三份\"");
        let reset = read("tests/__fixtures__/rotation-by-reset.rhai");
        for (id, text) in [("by-list", list), ("by-reset", reset), ("third", third)] {
            rotation::strategy_file_write(&h.own(), id, &text).expect("写策略");
        }
        rotation::face_change(
            &rotation::RotationStore::at(Some(h.root.join(rotation::FILE_NAME))),
            &h.own(),
            |b| b.for_new = "by-list".into(),
        )
        .expect("新会话用哪份");
        h
    }

    pub(crate) fn own(&self) -> crate::platform::fs::Own {
        crate::platform::fs::Own::issue_data_at(&self.root)
    }

    pub(crate) fn ctx(&self) -> Ctx {
        let accts = self.root.join("accts");
        Ctx {
            hop: Hop::new(
                Arc::new(RotationStore::at(Some(self.root.join(rotation::FILE_NAME)))),
                Arc::new(Ledger::at(Some(self.root.join(ledger::FILE_NAME)))),
                Some(self.root.clone()),
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
                }),
                {
                    let live = Arc::clone(&self.live);
                    Arc::new(move || live.lock().expect("lock").clone())
                },
                Some(dead_token_endpoint()),
            ),
            rows: Box::new(|_, a| (a == "api").then_some(true)),
        }
    }

    /// 中转第一次看见这个会话（开它的号 a）。
    pub(crate) fn saw(&self, ctx: &Ctx, sid: &str) {
        rotation::relay_change(&ctx.hop.store, &self.own(), |b| {
            b.saw(sid, "claude-code", "a", now())
        })
        .expect("write");
        self.live.lock().expect("lock").insert(sid.to_string());
    }

    pub(crate) fn alive(&self) -> impl Fn(&str) -> bool + '_ {
        move |sid: &str| self.live.lock().expect("lock").contains(sid)
    }
}

/// 额度账上记一条：`account` 的 5 小时窗用到 `used_5h`（0–1）；`refused_until` 有 ⇒ 被拒到那一刻。
pub(crate) fn seen(ctx: &Ctx, account: &str, used_5h: f64, refused_until: Option<u64>) {
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
    let own = crate::platform::fs::Own::issue_data_at(
        ctx.hop
            .quota
            .path()
            .and_then(std::path::Path::parent)
            .expect("有落点"),
    );
    ledger::record_seen(&ctx.hop.quota, &own, None, "claude-code", account, r, now);
}

fn session_view(ctx: &Ctx, home: &Home, sid: &str) -> RotationViewReply {
    view_with(
        ctx,
        &RotationViewReq {
            target: RotTarget::Session { sid: sid.into() },
            hours: None,
            lang: None,
        },
        now(),
        &home.alive(),
    )
    .expect("读得到")
}

// ── 命令登记 ────────────────────────────────────────────────────────────────────

/// ★ 登记表里有第五版的全部命令，第三版那 15 条一条都不在（不留别名）。
#[test]
fn the_registry_has_the_v5_commands_and_none_of_the_old_ones() {
    let names: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|c| c.name)
        .collect();
    for want in [
        "rotation-facts",
        "rotation-try",
        "rotation-strategies",
        "rotation-view",
        "rotation-settings",
        "rotation-save",
        "rotation-revert",
        "rotation-use",
        "rotation-for-new",
        "rotation-temp",
        "rotation-use-now",
        "rotation-strategy-add",
        "rotation-strategy-copy",
        "rotation-strategy-rename",
        "rotation-strategy-delete",
        "rotation-day-paint",
        "quota-read",
    ] {
        assert!(names.contains(&want), "缺 {want}");
    }
    for gone in [
        "rotation-configs-read",
        "rotation-config-save",
        "rotation-config-rename",
        "rotation-config-delete",
        "rotation-default-set",
        "rotation-strategies-read",
        "rotation-strategy-try",
        "rotation-plan",
        "rotation-session-read",
        "rotation-session-open",
        "rotation-session-edit",
        "rotation-session-save",
        "rotation-session-revert",
        "rotation-session-set",
        "rotation-switch",
    ] {
        assert!(!names.contains(&gone), "{gone} 该删了");
    }
}

// ── rotation-strategies ─────────────────────────────────────────────────────────

/// ★ 策略目录：用户的每一份都列着；新会话用的那份带标；在用只数活会话；读不懂的照列、带第几行。
#[test]
fn the_strategy_list_shows_every_file_with_who_uses_it() {
    let home = Home::new("list");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    home.saw(&ctx, S2);
    home.live.lock().expect("lock").remove(S2);
    std::fs::write(
        home.root.join("strategies/broken.rhai"),
        "/*--- rotation\n{\"format\": 5}\n---*/\n",
    )
    .expect("w");
    let got = strategies_with(&ctx, &RotationStrategiesReq::default(), &home.alive());
    let ids: Vec<&str> = got.strategies.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["broken", "by-list", "by-reset", "third"]);
    let lo = &got.strategies[1];
    assert!(lo.for_new && lo.error.is_none() && !lo.words_bad);
    assert_eq!(lo.in_use, [S1], "走了的会话也数进去了");
    assert!(got.strategies[0].error.is_some(), "读不懂的没带原因");
    assert_eq!(got.for_new, "by-list");
}

// ── rotation-view ──────────────────────────────────────────────────────────────

/// ★ 一个会话的四块：照它用的那份策略 · 声明 · 生效的设置 · 每个号的事实（号列表的在前）· 这一判 · 推演。
/// 用的那份文件没了 ⇒ 照兜底判、提示「策略出错」。
#[test]
fn a_session_view_carries_everything_the_four_blocks_draw() {
    let home = Home::new("view");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    seen(&ctx, "a", 0.4, None);
    let v = session_view(&ctx, &home, S1);
    let st = v.strategy.as_ref().expect("在轮换");
    assert_eq!(st.id, "by-list");
    assert_eq!(v.form.len(), 4, "夹具那份的声明");
    assert_eq!((v.start.as_str(), v.current.as_str()), ("a", "a"));
    assert_eq!(v.accounts[0].account, "a", "号列表里的在前");
    assert_eq!(v.listed, 1);
    assert_eq!(
        v.decision.as_ref().and_then(|d| d.pick.as_deref()),
        Some("a")
    );
    assert!(!v.dirty, "没改过");
    assert!(
        v.hints.iter().all(|h| matches!(h, RotHint::TzUnknown)),
        "{:?}",
        v.hints
    );

    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").strategy.id = "gone".into();
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    assert!(
        v.hints
            .iter()
            .any(|h| matches!(h, RotHint::StrategyError { .. })),
        "{:?}",
        v.hints
    );
    assert_eq!(
        v.decision.as_ref().and_then(|d| d.pick.as_deref()),
        Some("a"),
        "兜底：此刻的号发得出去就照用"
    );

    // 不轮换的会话：没有策略、不判不推演，只说开会话的号 · 在用的号。
    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").strategy.id = String::new();
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    assert!(v.strategy.is_none() && v.decision.is_none() && v.timeline.is_none());
    assert_eq!((v.start.as_str(), v.current.as_str()), ("a", "a"));
}

/// ★ 设置页那一形：照交来的草稿判、推演（不写盘）；草稿不合声明 ⇒ `refused` 带那几格；没有那一份 ⇒ `no_strategy`。
#[test]
fn a_strategy_view_takes_a_draft_and_checks_it() {
    let home = Home::new("draft");
    let ctx = home.ctx();
    let req = |settings: Option<serde_json::Value>, id: &str| RotationViewReq {
        target: RotTarget::Strategy {
            strategy: id.into(),
            settings,
        },
        hours: Some(24),
        lang: None,
    };
    let draft = json!({"caps": {}, "capKind": "hard", "accounts": [{"start": true}, {"id": "b"}]});
    let v = view_with(
        &ctx,
        &req(Some(draft.clone()), "by-list"),
        now(),
        &home.alive(),
    )
    .expect("ok");
    assert!(v.dirty);
    assert_eq!(v.settings, draft);
    assert!(home.root.join(rotation::FILE_NAME).exists());
    match view_with(
        &ctx,
        &req(Some(json!({"nope": 99})), "by-list"),
        now(),
        &home.alive(),
    ) {
        Err(RotationViewErr::Refused(_, r)) => assert_eq!(r.cells[0].cell, "nope"),
        other => panic!("该拒：{other:?}"),
    }
    assert!(matches!(
        view_with(&ctx, &req(None, "nope"), now(), &home.alive()),
        Err(RotationViewErr::NoStrategy(_))
    ));
    assert!(matches!(
        view_with(
            &ctx,
            &RotationViewReq {
                target: RotTarget::Session { sid: S2.into() },
                hours: None,
                lang: None
            },
            now(),
            &home.alive()
        ),
        Err(RotationViewErr::NoSession(_))
    ));
}

// ── session-new 起之前那一笔 ─────────────────────────────────────────────────────

/// `session-new` 带策略：起之前记好用那一份；会话一报到照旧是它；策略不在 ⇒ `no_strategy`；起不成撤掉。
#[test]
fn a_preset_session_keeps_its_strategy_when_the_relay_first_sees_it() {
    let home = Home::new("preset");
    let ctx = home.ctx();
    preset_with(&ctx, &home.own(), S1, "claude", "third", now()).expect("ok");
    home.saw(&ctx, S1);
    assert_eq!(ctx.hop.store.now().sessions[S1].strategy.id, "third");
    assert_eq!(
        preset_with(&ctx, &home.own(), S2, "claude", "nope", now()).map_err(|e| e.0),
        Err("no_strategy")
    );
    forget_preset_with(&ctx, &home.own(), S1);
    assert!(!ctx.hop.store.now().sessions.contains_key(S1));
}

// ── quota-read ────────────────────────────────────────────────────────────────

/// ★ `quota-read`：每条加显示态（「快满」按固定的 80%：线是策略的设置，后端说不出「到线」）；没出过数的号另列；
/// 应答里零令牌、零账号身份原值。
#[test]
fn quota_read_adds_the_display_state() {
    let home = Home::new("quota");
    let ctx = home.ctx();
    seen(&ctx, "a", 0.5, None);
    seen(&ctx, "b", 0.86, None);
    seen(&ctx, "api", 0.0, Some(now() + 600));
    let got =
        serde_json::to_value(quota_read_with(&ctx, now(), &crate::Tz::default())).expect("json");
    let row = |acct: &str| {
        got["accounts"]
            .as_array()
            .expect("accounts")
            .iter()
            .find(|r| r["account"] == acct)
            .cloned()
            .expect("row")
    };
    assert_eq!(row("a")["state"], "ok");
    assert_eq!(row("b")["state"], "near");
    assert_eq!(
        row("b")["subId"],
        crate::accounts::quota::show::sub_id_of(UUID_B)
    );
    assert_eq!(row("api")["state"], "refused");
    assert_eq!(got["unseen"][0]["account"], "c");
    let text = got.to_string();
    for secret in [UUID_B, B_TOKEN, B_REFRESH] {
        assert!(!text.contains(secret), "{secret} 漏进了应答");
    }
    assert!(!text.contains("atLine"), "「到线」那一格该删了");
}

/// ★ `quota-read` 的 `candidates`：出过数的在前、没出过的在后；`_`（起会话时没说是哪个号）出过数也不在里面。
#[test]
fn quota_read_hands_over_the_candidates_without_the_unnamed_account() {
    let home = Home::new("cands");
    let ctx = home.ctx();
    seen(&ctx, "_", 0.1, None);
    seen(&ctx, "a", 0.5, None);
    seen(&ctx, "api", 0.0, None);
    let got =
        serde_json::to_value(quota_read_with(&ctx, now(), &crate::Tz::default())).expect("json");
    let cands = got["candidates"].as_array().expect("candidates");
    let ids: Vec<&str> = cands
        .iter()
        .map(|c| c["account"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(ids, ["a", "api", "b", "c"], "{got}");
}

// ── 应答类型的样本（`every_command_declares_exactly_the_fields_it_puts_out` 读）──

impl crate::guard_support::Shaped for crate::faces::quota_read::QuotaRead {
    fn samples() -> Vec<Self> {
        vec![crate::faces::quota_read::specimen()]
    }
}
