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

/// 判据的「此刻」：钉死一刻（2026-01-15 03:00 UTC，不在整两点上：过去那一截里要有一格刻度），不读墙钟。往前两小时 · 往后十二小时（01:00–15:00） · 停发到一小时后都落在同一天（UTC），
/// 钟面一律只写 `HH:MM`；读墙钟时每天 UTC 23 点那一小时「停发到几点」跨了零点、带上日期，判据就红。
/// 产品那一侧的钟都从入口交进来（`view_with` · `preset_with` · `record_seen` 的 `now`）。
pub(crate) fn now() -> u64 {
    1_768_446_000
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
        let m = crate::accounts::quota::rules::upgrade::material;
        let (list, third, reset) = (m(0).to_string(), m(0).to_string(), m(1).to_string());
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
    seen_resetting(ctx, account, used_5h, refused_until, now() + 3_600);
}

/// 同 [`seen`]，5 小时窗在 `resets_at` 重置。
pub(crate) fn seen_resetting(
    ctx: &Ctx,
    account: &str,
    used_5h: f64,
    refused_until: Option<u64>,
    resets_at: u64,
) {
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
            resets_at: Some(resets_at),
            windows: vec![QuotaWindow {
                name: "five_hour".into(),
                used: Some(used_5h),
                resets_at: Some(resets_at),
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
        },
        now(),
        &crate::Tz::default(),
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
        "rotation-strategy-tz",
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
    assert_eq!(
        v.form.len(),
        5,
        "照号列表那份的声明（用量上限 · 上限类型 · 耗尽后启用 · 启用延迟 · 单次用量上限）"
    );
    assert_eq!((v.start.as_str(), v.current.as_str()), ("a", "a"));
    assert_eq!(v.accounts[0].fact.account, "a", "号列表里的在前");
    // 每个号带号色与号名（界面不再另问账号清单去对）：号色同 `accounts-list` 那一格的规则（库里没给 ⇒ 按名字算），号名同额度那几处。
    assert_eq!(
        (v.accounts[0].color, v.accounts[0].label.0.as_str()),
        (crate::accounts::manage::model::color_fallback("a"), "a")
    );
    assert_eq!(v.listed, 1);
    assert_eq!(
        v.decision.as_ref().and_then(|d| d.pick.as_deref()),
        Some("a")
    );
    assert!(!v.dirty, "没改过");
    assert!(
        v.hints
            .iter()
            .all(|h| matches!(h.hint, RotHint::TzUnknown { .. })),
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
            .any(|h| matches!(h.hint, RotHint::StrategyError { .. }) && h.text.contains("gone")),
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
            rev: None,
        },
        hours: Some(24),
    };
    let draft = json!({"caps": {}, "capKind": "hard", "accounts": [{"start": true}, {"id": "b"}]});
    let v = view_with(
        &ctx,
        &req(Some(draft.clone()), "by-list"),
        now(),
        &crate::Tz::default(),
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
        &crate::Tz::default(),
        &home.alive(),
    ) {
        Err(RotationViewErr::Refused(_, r)) => assert_eq!(r.cells[0].cell, "nope"),
        other => panic!("该拒：{other:?}"),
    }
    assert!(matches!(
        view_with(
            &ctx,
            &req(None, "nope"),
            now(),
            &crate::Tz::default(),
            &home.alive()
        ),
        Err(RotationViewErr::NoStrategy(_))
    ));
    assert!(matches!(
        view_with(
            &ctx,
            &RotationViewReq {
                target: RotTarget::Session { sid: S2.into() },
                hours: None,
            },
            now(),
            &crate::Tz::default(),
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

/// ★ 新会话框选「不轮换」：起之前写成不轮换的会话（策略 id 空），不去策略目录里找。
#[test]
fn a_preset_session_can_be_written_as_not_rotating() {
    let home = Home::new("preset-none");
    let ctx = home.ctx();
    preset_with(&ctx, &home.own(), S1, "claude", "", now()).expect("ok");
    home.saw(&ctx, S1);
    assert_eq!(ctx.hop.store.now().sessions[S1].strategy.id, "");
    let v = session_view(&ctx, &home, S1);
    assert!(v.strategy.is_none(), "起成了不轮换的会话");
}

/// ★ 时刻的字由后端写（界面不换算时刻）：此刻 · 刻度 · 换号时刻 · 每号行每段的起点 · 停发到几点；每段还带剩多少（事实）。
#[test]
fn a_session_view_writes_every_time_it_carries() {
    let home = Home::new("view-texts");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    seen(&ctx, "a", 0.4, None);
    let v = session_view(&ctx, &home, S1);
    let hm = |t: &str| t.len() == 5 && t.as_bytes()[2] == b':';
    assert!(hm(&v.now_text), "{}", v.now_text);
    let tl = v.timeline.as_ref().expect("推演");
    assert!(tl.ticks.len() >= 5, "12 小时每 2 小时一格：{:?}", tl.ticks);
    for t in &tl.ticks {
        assert!(
            hm(&t.text)
                && t.text.ends_with(":00")
                && t.at > v.past.as_ref().map_or(v.now, |p| p.from),
            "{t:?}"
        );
    }
    for w in tl.switches.iter() {
        assert!(hm(&w.at_text), "{w:?}");
    }
    let seg = &tl.lanes[0].segs[0];
    assert!(hm(&seg.from_text), "{seg:?}");
    assert!(
        (seg.left - 60.0).abs() < 1e-6,
        "剩 ＝ 100 − 两窗里用得多的：{seg:?}"
    );

    // 停发：到几点那一刻也写好。
    seen(&ctx, "a", 1.0, Some(now() + 3600));
    let v = session_view(&ctx, &home, S1);
    let held = v.hints.iter().find_map(|h| match &h.hint {
        RotHint::Hold { until_text, .. } => Some((until_text.clone(), h.text.clone())),
        _ => None,
    });
    let (at, said) = held.expect("停发");
    let at = at.expect("到几点说得出");
    assert!(hm(&at), "{at}");
    assert!(
        said.contains(&at) && said.contains("{dur}"),
        "停发那一句写好了到几点、留着「还有多久」给界面填：{said}"
    );
}

/// 会话那一形，带看的那一台的时区。
fn session_view_in(ctx: &Ctx, home: &Home, sid: &str, tz: &crate::Tz) -> RotationViewReply {
    view_with(
        ctx,
        &RotationViewReq {
            target: RotTarget::Session { sid: sid.into() },
            hours: None,
        },
        now(),
        tz,
        &home.alive(),
    )
    .expect("读得到")
}

/// ★ 时间轴过去那一截（只会话有）：往前推演时长的 1/6；最上那条照换号记录切段（停发停到 `until`）；
/// 每号一行只画说得出的：窗口开在这一截里 ⇒ 重置那段；此刻用满、记录里它被换下 ⇒ 那一刻起发不出去；刻度从这一截开头起。
#[test]
fn a_session_view_draws_what_already_happened() {
    let home = Home::new("view-past");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let n = now();
    seen_resetting(&ctx, "a", 0.3, None, n + 4 * 3600);
    seen(&ctx, "b", 1.0, None);
    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        let s = b.sessions.get_mut(S1).expect("s1");
        s.strategy.draft = Some(json!({"caps": {}, "capKind": "hard",
            "accounts": [{"start": true}, {"id": "b"}]}));
        s.log = vec![
            book::Log {
                at: n - 9000,
                from: "a".into(),
                to: "b".into(),
                cause: book::Cause::Order,
                why: None,
            },
            book::Log {
                at: n - 5400,
                from: "b".into(),
                to: "a".into(),
                cause: book::Cause::Unusable,
                why: None,
            },
            book::Log {
                at: n - 1800,
                from: "a".into(),
                to: "a".into(),
                cause: book::Cause::Hold {
                    until: Some(n - 900),
                },
                why: None,
            },
        ];
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    let p = v.past.as_ref().expect("会话有过去那一截");
    let near = |x: u64, y: u64| x.abs_diff(y) <= 5;
    assert!(near(p.from, n - 7200), "12 小时往前 2 小时：{}", p.from);
    let tops: Vec<(Option<&str>, bool, bool)> = p
        .uses
        .iter()
        .map(|u| {
            (
                u.account.as_deref(),
                near(u.from, n - 7200)
                    || near(u.from, n - 5400)
                    || near(u.from, n - 1800)
                    || near(u.from, n - 900),
                near(u.to, v.now)
                    || near(u.to, n - 5400)
                    || near(u.to, n - 1800)
                    || near(u.to, n - 900),
            )
        })
        .collect();
    assert_eq!(
        tops.iter().map(|t| t.0).collect::<Vec<_>>(),
        [Some("b"), Some("a"), None, Some("a")],
        "{:?}",
        p.uses
    );
    assert!(tops.iter().all(|t| t.1 && t.2), "{:?}", p.uses);
    let lane = |id: &str| p.lanes.iter().find(|l| l.account == id).expect("那一行");
    let a = lane("a");
    assert_eq!(a.segs.len(), 1, "{:?}", a.segs);
    assert!(a.segs[0].reset && a.segs[0].sendable && near(a.segs[0].from, n - 3600));
    assert!(a.segs[0].from_text.len() == 5, "{:?}", a.segs[0]);
    let b = lane("b");
    assert_eq!(
        b.segs.iter().map(|s| s.sendable).collect::<Vec<_>>(),
        [true, false],
        "{:?}",
        b.segs
    );
    assert!(near(b.segs[1].from, n - 5400));
    let tl = v.timeline.as_ref().expect("推演");
    assert!(
        tl.ticks.iter().any(|t| t.at < v.now && t.at > p.from),
        "刻度从过去那一截起：{:?}",
        tl.ticks
    );

    // 设置页没有会话 ⇒ 没有过去那一截。
    let s = view_with(
        &ctx,
        &RotationViewReq {
            target: RotTarget::Strategy {
                strategy: "by-list".into(),
                settings: None,
                rev: None,
            },
            hours: None,
        },
        now(),
        &crate::Tz::default(),
        &home.alive(),
    )
    .expect("ok");
    assert!(s.past.is_none());
}

/// ★ 「时区不明」总带上一个时区（「设时区」设成它）：信封里说了 ⇒ 那个名；`local` ⇒ 这台系统的时区名；
/// 那一侧说不出（没带 · 认不得 · 这台系统也读不出）⇒ `UTC`（分时段此刻就是按它算的，设成它只是把默认写明）。
#[test]
fn the_unknown_zone_hint_always_names_a_zone() {
    let home = Home::new("view-hint-tz");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let tz_hint = |tz: &crate::Tz| {
        session_view_in(&ctx, &home, S1, tz)
            .hints
            .into_iter()
            .find(|h| matches!(h.hint, RotHint::TzUnknown { .. }))
            .expect("时区不明那一条")
    };
    let named = tz_hint(&crate::Tz::named("Europe/Paris").expect("tz"));
    assert_eq!(
        named.hint,
        RotHint::TzUnknown {
            zone: "Europe/Paris".into()
        }
    );
    assert_eq!(named.text, copy_core::copy_text("beRotation.hint.tz", &[]));
    for unsaid in [serde_json::Value::Null, serde_json::json!("Mars/Base")] {
        let n = tz_hint(&crate::Tz::of(&unsaid));
        assert_eq!(
            n.hint,
            RotHint::TzUnknown { zone: "UTC".into() },
            "{unsaid}：说不出 ⇒ 照算的那一个"
        );
        assert_eq!(n.text, copy_core::copy_text("beRotation.hint.tz", &[]));
    }
    let here = tz_hint(&crate::Tz::named(crate::common::time::TZ_HERE).expect("local"));
    assert_eq!(
        here.hint,
        RotHint::TzUnknown {
            zone: crate::platform::local_tz::iana_name().unwrap_or_else(|| "UTC".into())
        },
        "「这一台」⇒ 这台系统的时区名"
    );
}

/// ★ 提示带上按钮要的东西：出错 ⇒ 那份文件的路径；时区不明 ⇒ 看的那一台的时区名；旧格式 ⇒ 哪几份、在哪。
/// 时刻的字照看的那一台的时区写。
#[test]
fn the_hints_carry_what_their_buttons_need() {
    let home = Home::new("view-hint-acts");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    std::fs::create_dir_all(home.root.join("strategies/v3")).expect("mkdir");
    std::fs::write(
        home.root.join("strategies/v3/old-one.rhai"),
        "fn score() {}",
    )
    .expect("w");
    let tz = crate::Tz::iana("Asia/Shanghai").expect("tz");
    let v = session_view_in(&ctx, &home, S1, &tz);
    assert_eq!(
        v.now_text,
        crate::common::time::hm(tz.local(i64::try_from(v.now).expect("i64"))),
        "此刻的字按看的那一台的时区"
    );
    assert!(
        v.hints.iter().any(|h| h.hint
            == RotHint::TzUnknown {
                zone: "Asia/Shanghai".into()
            }),
        "{:?}",
        v.hints
    );
    let old = v.hints.iter().find_map(|h| match &h.hint {
        RotHint::OldScripts { n, files } => Some((*n, files.clone())),
        _ => None,
    });
    let (n, files) = old.expect("旧格式那一条");
    assert_eq!(n, 1);
    assert_eq!(files[0].name, "old-one.rhai");
    assert!(
        files[0].path.ends_with("old-one.rhai") && files[0].path.contains("v3"),
        "{:?}",
        files
    );

    std::fs::write(
        home.root.join("strategies/broken.rhai"),
        "/*--- rotation\n{\"format\": 5}\n---*/\n",
    )
    .expect("w");
    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").strategy.id = "broken".into();
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    let path = v.hints.iter().find_map(|h| match &h.hint {
        RotHint::StrategyError { path, .. } => Some(path.clone()),
        _ => None,
    });
    let path = path.expect("出错那一条").expect("带着文件的路径");
    assert!(
        path.ends_with("broken.rhai") && path.contains("strategies"),
        "{path}"
    );
}

/// ★ 策略出错那一句照文案表：第几行（文件里的）· 哪一种（说法，不是 Rhai 的原话）· 改按默认优先级；原话进复制详情。
/// 文件不在 ⇒ 说读不到这份文件（没有行）。
#[test]
fn a_strategy_error_is_said_in_words_with_its_line_and_the_raw_text_as_detail() {
    let home = Home::new("view-error-words");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    rotation::strategy_file_write(
        &home.own(),
        "div",
        "// 说明\n/*--- rotation\n{\"format\": 5, \"settings\": {\"accounts\": [{\"start\": true}]}}\n---*/\nfn decide(ctx) {\n  let x = 1 / 0;\n  #{ order: [] }\n}\n",
    )
    .expect("写策略");
    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").strategy.id = "div".into();
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    let n = v
        .hints
        .iter()
        .find(|h| matches!(h.hint, RotHint::StrategyError { .. }))
        .unwrap_or_else(|| panic!("出错那一条：{:?}", v.hints));
    let why = copy_core::copy_text("beRotation.why.divZero", &[]);
    assert_eq!(
        n.text,
        copy_core::copy_text(
            "beRotation.hint.errorLine",
            &[("name", "div"), ("line", "6"), ("why", &why)]
        ),
        "第 6 行（说明 1 行 ＋ 头 3 行 ＋ fn 那 1 行）"
    );
    assert!(!n.text.contains("Division"), "上屏不带原话：{}", n.text);
    let detail = n.detail.as_deref().expect("带复制详情");
    assert!(detail.contains("Division by zero"), "{detail}");

    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").strategy.id = "gone".into();
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    let n = v
        .hints
        .iter()
        .find(|h| matches!(h.hint, RotHint::StrategyError { .. }))
        .expect("出错那一条");
    assert_eq!(
        n.text,
        copy_core::copy_text(
            "beRotation.hint.error",
            &[
                ("name", "gone"),
                ("why", &copy_core::copy_text("beRotation.why.gone", &[]))
            ]
        )
    );
    assert!(n.detail.is_some(), "原话照样进复制详情");
    // 别的提示没有复制详情。
    assert!(v
        .hints
        .iter()
        .filter(|h| !matches!(h.hint, RotHint::StrategyError { .. }))
        .all(|h| h.detail.is_none()));
}

/// ★ 别处改了：会话有草稿、草稿基于的版本跟文件此刻的不同 ⇒ 提示；没草稿 ⇒ 不提示（照新的走）。
/// 设置页同理（草稿基于的版本随 `target.rev` 交来）。`baseRev` ＝ 写的时候拿去当 `ifRev` 的那个。
#[test]
fn a_draft_on_an_older_version_is_flagged_as_changed_elsewhere() {
    let home = Home::new("view-newer");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let newer = |v: &RotationViewReply| v.hints.iter().any(|h| h.hint == RotHint::Newer);
    let file_rev = session_view(&ctx, &home, S1).strategy.expect("在轮换").rev;
    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").strategy.rev = "h:000000000000".into();
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    assert!(!newer(&v), "没草稿不该提示");
    assert_eq!(v.base_rev, file_rev);
    rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").strategy.draft =
            Some(json!({"caps": {}, "capKind": "hard", "accounts": [{"start": true}]}));
    })
    .expect("w");
    let v = session_view(&ctx, &home, S1);
    assert!(newer(&v), "{:?}", v.hints);
    assert!(v
        .hints
        .iter()
        .any(|h| h.hint == RotHint::Newer && h.text.contains("别处")));
    assert_eq!(v.base_rev, "h:000000000000");

    let page = |rev: Option<&str>| {
        view_with(
            &ctx,
            &RotationViewReq {
                target: RotTarget::Strategy {
                    strategy: "by-list".into(),
                    settings: Some(
                        json!({"caps": {}, "capKind": "hard", "accounts": [{"start": true}]}),
                    ),
                    rev: rev.map(str::to_string),
                },
                hours: None,
            },
            now(),
            &crate::Tz::default(),
            &home.alive(),
        )
        .expect("ok")
    };
    assert!(newer(&page(Some("h:000000000000"))));
    assert!(!newer(&page(Some(&file_rev))));
    assert_eq!(page(Some(&file_rev)).base_rev, file_rev);
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
