//! 换号那一族的帧命令（第二版）：配置表 · 策略 · 会话的副本与手动调整 · 推演 · 现在就换 · 额度显示态。临时家目录、假凭据、不起中转。

use super::*;
use crate::accounts::upstream_select::rotate::{LibAccount, Library};
use crate::faces::rotation_switch_face::{restart_args, restart_outcome, switch_ask};
use serde_json::json;

const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const B_TOKEN: &str = "fake-access-of-b";

/// 判据里的「此刻」用真钟（假凭据的过期时刻也按真钟算 ⇒ 不会因为钟拨快而去续）。
fn now() -> u64 {
    crate::accounts::quota::now_unix()
}

/// 续令牌一律发到本机一个绑着不听的口（拨了必被拒 ⇒ 续不上），绝不发到那一家的真端点。
fn dead_token_endpoint() -> crate::accounts::oauth::TokenEndpoint {
    let port = crate::refusing_port::refusing_port();
    crate::accounts::oauth::TokenEndpoint {
        base: crate::relay::Base::parse(&format!("http://127.0.0.1:{port}")).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_millis(2_000),
    }
}

struct Home {
    root: std::path::PathBuf,
    live: Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotface2-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let b = root.join("accts").join("b");
        std::fs::create_dir_all(&b).expect("mkdir");
        let far = (now() + 3600) * 1000;
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
                {
                    let live = Arc::clone(&self.live);
                    Arc::new(move || live.lock().expect("lock").clone())
                },
                Some(dead_token_endpoint()),
            ),
            rows: Box::new(|_, a| (a == "api").then_some(true)),
            tz: crate::Tz::default(),
        }
    }

    fn saw(&self, ctx: &Ctx, sid: &str) {
        rotation::face_change(&ctx.hop.store, &own_of(&ctx), |b| {
            b.saw(sid, "claude-code", "a", now())
        })
        .expect("write");
        self.live.lock().expect("lock").insert(sid.to_string());
    }

    fn end(&self, sid: &str) {
        self.live.lock().expect("lock").remove(sid);
    }
}

/// 写句柄：数据目录那一根同这份上下文里额度账所在的那一层（判据的家）。
fn own_of(ctx: &Ctx) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_data_at(
        ctx.hop
            .quota
            .path()
            .and_then(std::path::Path::parent)
            .expect("判据的额度账有落点"),
    )
}

const S1: &str = "11111111-1111-4111-8111-111111111111";
const S2: &str = "22222222-2222-4222-8222-222222222222";

// ── S14 命令登记 ─────────────────────────────────────────────────────────────────

/// ★ S14：登记表里有积木 §5 的全部命令，删掉的四条不在。
#[test]
fn the_registry_has_the_v2_commands_and_not_the_old_ones() {
    let names: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|c| c.name)
        .collect();
    for want in [
        "rotation-configs-read",
        "rotation-config-save",
        "rotation-config-rename",
        "rotation-config-delete",
        "rotation-default-set",
        "rotation-strategies-read",
        "rotation-strategy-add",
        "rotation-strategy-delete",
        "rotation-strategy-try",
        "rotation-session-read",
        "rotation-session-open",
        "rotation-session-edit",
        "rotation-session-save",
        "rotation-session-revert",
        "rotation-session-set",
        "rotation-switch",
        "rotation-plan",
    ] {
        assert!(names.contains(&want), "缺 {want}");
    }
    for gone in [
        "rotation-rules-read",
        "rotation-rule-save",
        "rotation-rule-rename",
        "rotation-rule-delete",
    ] {
        assert!(!names.contains(&gone), "{gone} 该删了（不留别名）");
    }
}

// ── 配置表 ───────────────────────────────────────────────────────────────────────

/// 配置表：内置三份、默认在最前、每份带后端写好的描述 · 策略名 · 谁开着。
#[test]
fn the_configs_read_lists_the_seeds_with_words() {
    let home = Home::new("cfg-read");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let v = answer_configs_read_with(&ctx, &json!({}), now()).expect("ok");
    assert_eq!(v["state"], "present");
    assert_eq!(v["defaultConfig"], "c_00000001");
    let cs = v["configs"].as_array().expect("configs");
    assert_eq!(cs.len(), 3);
    assert_eq!(cs[0]["id"], "c_00000001");
    assert_eq!(cs[0]["isDefault"], true);
    assert_eq!(cs[0]["inUse"], json!([S1]));
    assert_eq!(
        cs[0]["strategyName"],
        copy_text("beRotation.strategy.builtinName", &[("id", "1")])
    );
    for c in cs {
        let t = c["text"].as_str().expect("text");
        assert!(
            !t.is_empty() && !t.contains('〔') && !t.contains("@d_"),
            "{t}"
        );
    }
    let two = cs.iter().find(|c| c["id"] == "c_00000002").expect("2");
    // 「账号库里其余的订阅号」此刻是哪几个（编辑器那一行照抄）：库里没点名的订阅号，按名字排；按量号不算。
    assert_eq!(two["rest"], json!(["a", "b", "c"]));
    assert!(two["text"]
        .as_str()
        .unwrap_or("")
        .contains(&copy_text("beRotation.desc.line", &[("n", "90")])));
}

fn seed_body(ctx: &Ctx, id: &str) -> Value {
    let v = answer_configs_read_with(ctx, &json!({}), now()).expect("ok");
    let c = v["configs"]
        .as_array()
        .expect("configs")
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
        .expect("config");
    json!({
        "name": c["name"], "strategy": c["strategy"], "accounts": c["accounts"],
        "params": c["params"], "acctInit": c["acctInit"], "parked": c["parked"]
    })
}

/// ★ S9 · S14：存一份 ⇒ 参数照声明逐格校验（越界 · 类型不对 · 不在 `of` ⇒ 逐格 `refused`）；名字空 · 重名 ⇒ 拒；
/// 撞 `rev` ⇒ 冲突；`force` ⇒ 覆盖；改名 · 删（开着的 · 本机默认不删）· 设本机默认。
#[test]
fn saving_a_config_checks_every_cell_and_its_rev() {
    let home = Home::new("cfg-save");
    let ctx = home.ctx();
    let mut body = seed_body(&ctx, "c_00000002");
    body["name"] = json!("夜里");
    body["params"]["skip"] = json!(99);
    body["params"]["all_out"] = json!("maybe");
    body["accounts"][1]["params"]["slots"] =
        json!([{"at": "17:00-02:00", "line": 80}, {"at": "01:00-03:00", "line": 70}]);
    let v =
        answer_config_save_with(&ctx, &own_of(&ctx), &json!({"config": body}), now()).expect("ok");
    assert_eq!(v["state"], "refused");
    let cells: Vec<(&str, &str)> = v["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|e| {
            (
                e["cell"].as_str().unwrap_or(""),
                e["code"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert!(cells.contains(&("params.skip", "range")), "{cells:?}");
    assert!(cells.contains(&("params.all_out", "of")), "{cells:?}");
    assert!(
        cells.contains(&("accounts[1].slots[1].at", "overlap")),
        "{cells:?}"
    );
    for e in v["errors"].as_array().expect("errors") {
        assert!(!e["said"].as_str().unwrap_or("").is_empty());
    }
    body["params"]["skip"] = json!(3);
    body["params"]["all_out"] = json!("go");
    body["accounts"][1]["params"]["slots"] = json!([]);
    let v =
        answer_config_save_with(&ctx, &own_of(&ctx), &json!({"config": body}), now()).expect("ok");
    assert_eq!(v["state"], "saved", "{v}");
    let id = v["id"].as_str().expect("id").to_string();
    assert_eq!(v["rev"], 1);
    // 重名 ⇒ 拒。
    let v =
        answer_config_save_with(&ctx, &own_of(&ctx), &json!({"config": body}), now()).expect("ok");
    assert_eq!(v["errors"][0]["code"], "dup");
    // 撞 rev ⇒ 冲突；force ⇒ 覆盖。
    body["params"]["skip"] = json!(4);
    let v = answer_config_save_with(
        &ctx,
        &own_of(&ctx),
        &json!({"id": id, "ifRev": 0, "config": body}),
        now(),
    )
    .expect("ok");
    assert_eq!(v["state"], "conflict");
    assert_eq!(v["rev"], 1);
    assert!(!v["theirs"].as_array().expect("theirs").is_empty());
    let v = answer_config_save_with(
        &ctx,
        &own_of(&ctx),
        &json!({"id": id, "ifRev": 0, "force": true, "config": body}),
        now(),
    )
    .expect("ok");
    assert_eq!(
        (v["state"].as_str(), v["rev"].as_u64()),
        (Some("saved"), Some(2))
    );
    // 改名。
    let v = answer_config_rename_with(
        &ctx,
        &own_of(&ctx),
        &json!({"id": id, "name": "白天"}),
        now(),
    )
    .expect("ok");
    assert_eq!(v["state"], "saved");
    // 开着它的不删；本机默认不删。
    home.saw(&ctx, S1);
    answer_session_open_with(&ctx, &own_of(&ctx), &json!({"sid": S1, "config": id})).expect("open");
    let v = answer_config_delete_with(&ctx, &own_of(&ctx), &json!({"id": id})).expect("ok");
    assert_eq!(
        (v["state"].as_str(), v["inUse"].clone()),
        (Some("refused"), json!([S1]))
    );
    let v =
        answer_config_delete_with(&ctx, &own_of(&ctx), &json!({"id": "c_00000001"})).expect("ok");
    assert_eq!(v["isDefault"], true);
    answer_default_set_with(&ctx, &own_of(&ctx), &json!({"id": id})).expect("ok");
    assert_eq!(ctx.hop.store.now().default_config, id);
    answer_session_open_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "config": "c_00000003"}),
    )
    .expect("open");
    let v =
        answer_config_delete_with(&ctx, &own_of(&ctx), &json!({"id": "c_00000002"})).expect("ok");
    assert_eq!(v["state"], "done");
    assert!(answer_default_set_with(&ctx, &own_of(&ctx), &json!({"id": "nope"})).is_err());
}

// ── 策略 ─────────────────────────────────────────────────────────────────────────

/// ★ S9 · S12：策略列表：共用小函数的源码 · 内置三份（声明写好字）· 本机装一份 ⇒ 出现；与内置同 id ⇒ 不收；编不过的说第几行；
/// 有配置在用的删不了、内置的删不了。
#[test]
fn strategies_are_listed_added_tried_and_deleted() {
    let home = Home::new("strat");
    let ctx = home.ctx();
    let v = answer_strategies_read_with(&ctx, &json!({"lang": "en"}), now()).expect("ok");
    assert!(v["lib"]["source"]
        .as_str()
        .unwrap_or("")
        .contains("fn usable"));
    let list = v["strategies"].as_array().expect("list");
    assert_eq!(
        list.iter().map(|s| s["id"].clone()).collect::<Vec<_>>(),
        [json!("1"), json!("2"), json!("3")]
    );
    let mode = list[0]["acctParams"]
        .as_array()
        .expect("acct")
        .iter()
        .find(|d| d["k"] == "mode")
        .cloned()
        .expect("mode");
    assert_eq!(mode["type"], "choice");
    assert_eq!(mode["label"], "Use");
    assert_eq!(
        mode["options"][1],
        json!({"v": "first", "label": "First", "hint": "use whenever it has quota"})
    );
    assert_eq!(list[0]["usedBy"], json!(["c_00000001"]));
    // 本机装一份（照内置 3 改个名）。
    let src = strategy::BUILTIN[2]
        .1
        .replace("format: 1,", "format: 1, name: \"我的\",");
    let v = answer_strategy_add_with(&ctx, &own_of(&ctx), &json!({"id": "mine", "source": src}))
        .expect("ok");
    assert_eq!(v["state"], "done");
    let v = answer_strategies_read_with(&ctx, &json!({}), now()).expect("ok");
    let mine = v["strategies"]
        .as_array()
        .expect("list")
        .iter()
        .find(|s| s["id"] == "mine")
        .cloned()
        .expect("mine");
    assert_eq!(
        (mine["name"].as_str(), mine["builtin"].as_bool()),
        (Some("我的"), Some(false))
    );
    // 同 id ⇒ 不收；编不过 ⇒ 说第几行。
    let v = answer_strategy_add_with(&ctx, &own_of(&ctx), &json!({"id": "2", "source": src}))
        .expect("ok");
    assert_eq!(v["state"], "refused");
    let v = answer_strategy_add_with(
        &ctx,
        &own_of(&ctx),
        &json!({"id": "bad", "source": "fn meta() {\n  oops(\n}"}),
    )
    .expect("ok");
    assert_eq!(v["state"], "refused");
    assert!(v["error"]["line"].as_u64().is_some());
    assert!(
        !ctx.hop
            .store
            .strategy_dir()
            .expect("dir")
            .join("bad.rhai")
            .exists(),
        "读不懂的不写"
    );
    // 试跑：按此刻的事实、不写盘。
    let before = std::fs::read(ctx.hop.store.path().expect("p")).ok();
    let v = answer_strategy_try_with(&ctx, &json!({"config": "c_00000002", "id": "mine"}), now())
        .expect("ok");
    assert!(v["ops"].as_u64().unwrap_or(0) > 0);
    assert_eq!(v["maxOps"], strategy::MAX_OPERATIONS);
    assert!(!v["rows"].as_array().expect("rows").is_empty());
    assert_eq!(
        std::fs::read(ctx.hop.store.path().expect("p")).ok(),
        before,
        "试跑不写盘"
    );
    let v = answer_strategy_try_with(
        &ctx,
        &json!({"config": "c_00000002", "source": "fn meta( {"}),
        now(),
    )
    .expect("ok");
    assert!(v["strategyError"]["text"].as_str().is_some(), "出错照实说");
    // 有配置在用的删不了；内置的删不了；没人用 ⇒ 删掉。
    let mut body = seed_body(&ctx, "c_00000003");
    body["name"] = json!("用我的");
    body["strategy"] = json!("mine");
    let v =
        answer_config_save_with(&ctx, &own_of(&ctx), &json!({"config": body}), now()).expect("ok");
    let cid = v["id"].as_str().expect("id").to_string();
    let v = answer_strategy_delete_with(&ctx, &own_of(&ctx), &json!({"id": "mine"})).expect("ok");
    assert_eq!(
        (v["state"].as_str(), v["usedBy"].clone()),
        (Some("refused"), json!([cid.clone()]))
    );
    assert_eq!(
        answer_strategy_delete_with(&ctx, &own_of(&ctx), &json!({"id": "1"})).expect("ok")["state"],
        "refused"
    );
    answer_config_delete_with(&ctx, &own_of(&ctx), &json!({"id": cid})).expect("ok");
    assert_eq!(
        answer_strategy_delete_with(&ctx, &own_of(&ctx), &json!({"id": "mine"})).expect("ok")
            ["state"],
        "done"
    );
}

// ── 会话 ─────────────────────────────────────────────────────────────────────────

fn session(ctx: &Ctx, sid: &str) -> Value {
    answer_session_read_with(ctx, &json!({"sids": [sid]}), now()).expect("ok")["sessions"][sid]
        .clone()
}

/// ★ 一批会话：见过的给全那一份（第一行在用、往下照有效优先级、每行写好为什么）；没见过的照实标 `absent`。
#[test]
fn a_batch_read_answers_every_session_and_marks_the_unknown() {
    let home = Home::new("batch");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    answer_session_open_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "config": "c_00000002"}),
    )
    .expect("open");
    let v = answer_session_read_with(&ctx, &json!({"sids": [S1, S2]}), now()).expect("ok");
    assert_eq!(
        v["sessions"][S2],
        json!({"state": "absent", "inPlace": "noRelay"})
    );
    let s = &v["sessions"][S1];
    assert_eq!(s["state"], "present");
    assert_eq!(s["config"]["id"], "c_00000002");
    assert_eq!(s["config"]["dirty"], false);
    assert_eq!(s["strategy"]["id"], "2");
    let rows = s["rows"].as_array().expect("rows");
    assert_eq!(rows[0]["account"], "a", "第一行是在用的号");
    let ids: Vec<&str> = rows
        .iter()
        .map(|r| r["account"].as_str().unwrap_or(""))
        .collect();
    assert!(
        ids.contains(&"b") && ids.contains(&"c"),
        "其余订阅号从 rest 那一行来：{ids:?}"
    );
    assert!(!ids.contains(&"api"), "按量号不在「其余的订阅号」里");
    let c = rows.iter().find(|r| r["account"] == "c").expect("c");
    assert_eq!(c["fromRest"], true);
    assert_eq!(c["skip"], true, "c 没登录 ⇒ 事实上不能用");
    assert_eq!(c["facts"]["whyNot"], "login");
    for r in rows {
        assert!(!r["why"].as_str().unwrap_or("").contains('〔'));
    }
    assert!(!s["text"].as_str().unwrap_or("").is_empty());
    assert_eq!(s["values"]["accounts"][1]["who"], "@rest");
    assert!(v["names"].is_object(), "号名与位名那一格");
}

/// ★ S10：拖 · 刷新策略 · 现在就换都不碰副本；改一格 ⇒ 已改；别处存了新版 ⇒ 有副本的出 `newer`；还原 ⇒ 副本丢掉。
#[test]
fn manual_adjustments_never_touch_the_copy() {
    let home = Home::new("manual");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    answer_session_open_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "config": "c_00000002"}),
    )
    .expect("open");
    let v = answer_session_edit_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "set": [{"k": "delay", "v": 5}]}),
    )
    .expect("ok");
    assert_eq!(v["dirty"], true);
    assert_eq!(v["diff"].as_array().map(Vec::len), Some(1));
    let copy = || ctx.hop.store.now().sessions[S1].config.copy.clone();
    let before = copy();
    assert_eq!(
        answer_session_set_with(
            &ctx,
            &own_of(&ctx),
            &json!({"sid": S1, "place": {"account": "c", "at": 1}}),
            now()
        )
        .expect("ok")["state"],
        "done"
    );
    assert_eq!(ctx.hop.store.now().sessions[S1].manual.place.len(), 1);
    let v = answer_session_set_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "place": {"account": "b", "at": 0}}),
        now(),
    )
    .expect("ok");
    assert_eq!(v["state"], "done", "{v}");
    let book = ctx.hop.store.now();
    let s = &book.sessions[S1];
    assert_eq!(s.current, "b");
    assert_eq!(s.manual.hot.as_ref().map(|h| h.account.as_str()), Some("b"));
    assert!(matches!(
        s.history.last().map(|h| &h.leave),
        Some(Leave::Manual)
    ));
    assert_eq!(copy(), before, "手动调整不碰副本");
    let r = session(&ctx, S1);
    assert_eq!(r["rows"][0]["account"], "b");
    assert_eq!(r["hot"], "b");
    assert_eq!(r["config"]["dirty"], true);
    answer_session_set_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "refresh": true}),
        now(),
    )
    .expect("ok");
    let s = ctx.hop.store.now().sessions[S1].clone();
    assert!(s.manual.is_empty() && s.memo.is_none());
    assert_eq!(copy(), before, "刷新策略不碰副本");
    // 设置窗存了同一份的新版 ⇒ 有副本的会话出 `newer`。
    let body = seed_body(&ctx, "c_00000002");
    let v = answer_config_save_with(
        &ctx,
        &own_of(&ctx),
        &json!({"id": "c_00000002", "config": body}),
        now(),
    )
    .expect("ok");
    assert_eq!(v["rev"], 2);
    let r = session(&ctx, S1);
    assert_eq!(r["config"]["newer"]["rev"], 2);
    assert!(!r["config"]["newer"]["text"]
        .as_str()
        .unwrap_or("")
        .is_empty());
    // 存：撞上（打开时 rev 1）⇒ 冲突；还原 ⇒ 副本丢掉、rev 跟上。
    let v = answer_session_save_with(&ctx, &own_of(&ctx), &json!({"sid": S1, "ifRev": 1}), now())
        .expect("ok");
    assert_eq!(v["state"], "conflict");
    answer_session_revert_with(&ctx, &own_of(&ctx), &json!({"sid": S1, "toLatest": true}))
        .expect("ok");
    let s = ctx.hop.store.now().sessions[S1].clone();
    assert_eq!((s.config.copy.is_none(), s.config.rev), (true, 2));
    // 另存为。
    answer_session_edit_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "set": [{"k": "delay", "v": 7}]}),
    )
    .expect("ok");
    let v = answer_session_save_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "ifRev": 2, "as": {"name": "我的"}}),
        now(),
    )
    .expect("ok");
    assert_eq!(v["state"], "saved");
    assert_eq!(
        ctx.hop.store.now().sessions[S1].config.id,
        v["id"].as_str().unwrap_or("")
    );
}

/// ★ 手动热换：结束了的会话不换（说 ended）；目标没登录 ⇒ 没成；成了不出任何提示（回包只有 done）。
#[test]
fn a_hot_switch_pins_the_session_or_says_why_not() {
    let home = Home::new("hot");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    assert_eq!(
        serde_json::to_value(hot_one(&ctx, &own_of(&ctx), S1, "c", now())).expect("json"),
        json!({"state": "failed", "code": "targetNeedsLogin"})
    );
    assert_eq!(
        serde_json::to_value(hot_one(&ctx, &own_of(&ctx), S1, "b", now())).expect("json"),
        json!({"state": "done"})
    );
    home.end(S1);
    assert_eq!(
        serde_json::to_value(hot_one(&ctx, &own_of(&ctx), S1, "a", now())).expect("json"),
        json!({"state": "skipped", "code": "ended"})
    );
    assert_eq!(
        serde_json::to_value(hot_one(&ctx, &own_of(&ctx), S2, "b", now())).expect("json"),
        json!({"state": "skipped", "code": "noRelay"})
    );
}

/// `session-new` 带配置：起之前记好打开那一份；会话一报到（中转第一次看见）照旧是它；配置不在 ⇒ `no_such_config`。
#[test]
fn a_preset_session_keeps_its_config_when_the_relay_first_sees_it() {
    let home = Home::new("preset");
    let ctx = home.ctx();
    preset_with(&ctx, &own_of(&ctx), S1, "claude", "c_00000003", now()).expect("ok");
    rotation::relay_change(&ctx.hop.store, &own_of(&ctx), |b| {
        b.saw(S1, "claude-code", "a", now())
    })
    .expect("w");
    assert_eq!(ctx.hop.store.now().sessions[S1].config.id, "c_00000003");
    assert_eq!(
        preset_with(&ctx, &own_of(&ctx), S2, "claude", "nope", now()).map_err(|e| e.0),
        Err("no_such_config")
    );
    forget_preset_with(&ctx, &own_of(&ctx), S1);
    assert!(!ctx.hop.store.now().sessions.contains_key(S1));
}

/// 读不懂的那份：照实说（`unreadable` ＋ 一句 ＋ 复制详情），照内置三份回、不覆盖。
#[test]
fn an_unreadable_rotation_file_answers_a_sentence_and_a_detail() {
    let home = Home::new("unread");
    let ctx = home.ctx();
    std::fs::create_dir_all(&home.root).expect("mkdir");
    std::fs::write(home.root.join(rotation::FILE_NAME), "{not json").expect("w");
    let v = answer_configs_read_with(&ctx, &json!({}), now()).expect("ok");
    assert_eq!(v["state"], "unreadable");
    assert!(v["reason"].as_str().is_some() && v["detail"].as_str().is_some());
    assert_eq!(v["configs"].as_array().map(Vec::len), Some(3));
    assert_eq!(
        std::fs::read_to_string(home.root.join(rotation::FILE_NAME)).expect("r"),
        "{not json"
    );
}

/// 策略文件坏了 ⇒ 会话那一份照内置策略 1 排、带「策略出错」那一句；描述后面接「照内置策略 1 排」。
#[test]
fn a_broken_strategy_is_said_on_the_session() {
    let home = Home::new("broken");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let dir = ctx.hop.store.strategy_dir().expect("dir");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = strategy::BUILTIN[1].1.replace(
        "fn priority(ctx) {",
        "fn priority(ctx) {\n  throw \"boom\";",
    );
    std::fs::write(dir.join("boom.rhai"), src).expect("w");
    let mut body = seed_body(&ctx, "c_00000002");
    body["name"] = json!("会坏");
    body["strategy"] = json!("boom");
    let v =
        answer_config_save_with(&ctx, &own_of(&ctx), &json!({"config": body}), now()).expect("ok");
    let id = v["id"].as_str().expect("id").to_string();
    answer_session_open_with(&ctx, &own_of(&ctx), &json!({"sid": S1, "config": id})).expect("open");
    let s = session(&ctx, S1);
    assert_eq!(s["strategyError"]["kind"], "failed");
    assert!(
        s["strategyError"]["text"]
            .as_str()
            .unwrap_or("")
            .contains("boom")
            || s["strategyError"]["line"].is_number()
    );
    assert!(
        !s["rows"].as_array().expect("rows").is_empty(),
        "照内置 1 照样排"
    );
}

// ── 推演 ─────────────────────────────────────────────────────────────────────────

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
    ledger::record_seen(&ctx.hop.quota, &own_of(ctx), "claude-code", account, r, now);
}

/// ★ 推演：此刻之后的段都是估的；到线那一段（`line`）只在策略声明了 `line` 时出；视窗起止 · 刻度带字。
#[test]
fn the_plan_marks_future_segments_and_line_bars() {
    let home = Home::new("plan");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    answer_session_open_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "config": "c_00000002"}),
    )
    .expect("open");
    seen(&ctx, "b", 0.95, None);
    let v = answer_plan_with(&ctx, &json!({"sid": S1, "view": "12h"}), now()).expect("ok");
    assert_eq!(v["view"], "12h");
    assert_eq!(
        v["until"].as_u64().unwrap_or(0) - v["from"].as_u64().unwrap_or(0),
        12 * 3600
    );
    let plan = v["plan"].as_array().expect("plan");
    assert!(plan.iter().any(|p| p["est"] == true), "此刻之后的是估的");
    assert!(plan
        .iter()
        .all(|p| p["fromText"].is_string() && p["toText"].is_string()));
    let b = v["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["account"] == "b")
        .cloned()
        .expect("b");
    assert!(
        b["bars"]
            .as_array()
            .expect("bars")
            .iter()
            .any(|x| x["k"] == "line"),
        "b 95% 过了 90 线 ⇒ 到线段"
    );
    assert!(!v["grid"].as_array().expect("grid").is_empty());
    // 一份不声明 `line` 的策略 ⇒ 没有到线段。
    let src = "fn meta() { #{ format: 1, params: [], acct_params: [] } }\nfn texts() { #{ zh: #{ w: \"字\" }, en: #{ w: \"w\" } } }\nfn describe(p, a) { [\"w\"] }\nfn priority(ctx) { let o = #{}; for a in ctx.accounts { o[a.id] = #{ p: if a.ok { 1 } else { () }, why: \"w\" }; } o }";
    let v =
        answer_plan_with(&ctx, &json!({"config": "c_00000002", "source": src}), now()).expect("ok");
    for r in v["rows"].as_array().expect("rows") {
        assert!(r["bars"]
            .as_array()
            .expect("bars")
            .iter()
            .all(|x| x["k"] != "line"));
    }
}

// ── 现在就换的入参 · 重启换的结局（照旧）─────────────────────────────────────────────

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
    .map(|(k, r)| (k, serde_json::to_value(restart_outcome(&r)).expect("json")))
    .collect()
}

/// 跨语言金样：重启换那几格（TS `decodeRestartOutcomes` 读同一份）。
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
    let golden: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("金样")).expect("json");
    assert_eq!(got, golden);
}

// ── 额度显示态 ─────────────────────────────────────────────────────────────────────

/// ★ `quota-read`：每条加显示态；「快满」按本机默认那份配置里这个号那一行的线（默认配置 1 用满才换 ⇒ 落 80%）；
/// 没出过数的号另列；应答里零令牌、零账号身份原值。
#[test]
fn quota_read_adds_the_display_state_and_follows_the_default_config_lines() {
    let home = Home::new("quota");
    let ctx = home.ctx();
    seen(&ctx, "a", 0.5, None);
    seen(&ctx, "b", 0.86, None);
    seen(&ctx, "api", 0.0, Some(now() + 600));
    let read =
        || serde_json::to_value(quota_read_with(&ctx, now(), &crate::Tz::default())).expect("json");
    let row = |got: &Value, acct: &str| {
        got["accounts"]
            .as_array()
            .expect("accounts")
            .iter()
            .find(|r| r["account"] == acct)
            .cloned()
            .expect("row")
    };
    let got = read();
    assert_eq!(row(&got, "b")["state"], "near");
    assert_eq!(row(&got, "b")["subId"], show::sub_id_of(UUID_B));
    assert_eq!(row(&got, "api")["state"], "refused");
    assert_eq!(got["unseen"][0]["account"], "c");
    let text = got.to_string();
    for secret in [UUID_B, B_TOKEN, "fake-refresh-of-b"] {
        assert!(!text.contains(secret), "{secret} 漏进了应答");
    }
    // 本机默认换成默认配置 2（其余订阅号 90%）⇒ b 86% 不再算快满。
    answer_default_set_with(&ctx, &own_of(&ctx), &json!({"id": "c_00000002"})).expect("ok");
    assert_eq!(row(&read(), "b")["state"], "ok");
}

// ── 应答类型的样本（`every_command_declares_exactly_the_fields_it_puts_out` 读；每一支一个，可缺的格都填上）──

use crate::guard_support::Shaped;

fn cell_error() -> CellError {
    rotation::cell_err("name".into(), "empty")
}

fn diff_sample() -> Diff {
    Diff {
        acct: Some("@rest".into()),
        k: "line".into(),
        from: json!(90),
        to: json!(80),
        text: "t".into(),
    }
}

fn fail_sample() -> StrategyError {
    StrategyError {
        fail: StrategyFail {
            kind: FailKind::Broke,
            line: Some(3),
            col: Some(4),
            msg: "m".into(),
        },
        text: "t".into(),
    }
}

impl Shaped for ConfigsRead {
    fn samples() -> Vec<Self> {
        vec![ConfigsRead {
            state: "unreadable".into(),
            reason: json!("why"),
            detail: json!("detail lines"),
            configs: Vec::new(),
            default_config: "c_00000001".into(),
            path: Some("/x/rotation.json".into()),
        }]
    }
}

impl Shaped for ConfigSaved {
    fn samples() -> Vec<Self> {
        vec![
            ConfigSaved::Saved {
                id: "c_1".into(),
                rev: 2,
                saved_at_text: "04:30".into(),
            },
            ConfigSaved::Refused {
                errors: vec![cell_error()],
            },
            ConfigSaved::Conflict {
                rev: 3,
                theirs: vec![diff_sample()],
            },
        ]
    }
}

impl Shaped for ConfigDropped {
    fn samples() -> Vec<Self> {
        vec![
            ConfigDropped::Done,
            ConfigDropped::Refused {
                in_use: vec!["s".into()],
                is_default: false,
                said: "s".into(),
            },
        ]
    }
}

impl Shaped for Done {
    fn samples() -> Vec<Self> {
        vec![done()]
    }
}

impl Shaped for StrategiesRead {
    fn samples() -> Vec<Self> {
        vec![StrategiesRead {
            lib: LibShown {
                source: "src".into(),
            },
            strategies: Vec::new(),
        }]
    }
}

impl Shaped for StrategyChanged {
    fn samples() -> Vec<Self> {
        vec![
            StrategyChanged::Done { id: "mine".into() },
            StrategyChanged::Refused {
                said: "s".into(),
                error: Some(fail_sample()),
                used_by: vec!["c_1".into()],
            },
        ]
    }
}

impl Shaped for TryReply {
    fn samples() -> Vec<Self> {
        vec![TryReply {
            rows: Vec::new(),
            pick: Some("a".into()),
            memo: Some(json!({})),
            ops: 1,
            max_ops: 2,
            strategy_error: Some(fail_sample()),
        }]
    }
}

impl Shaped for PlanReply {
    fn samples() -> Vec<Self> {
        vec![PlanReply {
            now: 1,
            now_text: "00:00".into(),
            from: 0,
            until: 2,
            view: "12h".into(),
            plan: Vec::new(),
            rows: Vec::new(),
            grid: Vec::new(),
            strategy_error: Some(fail_sample()),
        }]
    }
}

impl Shaped for SessionRead {
    fn samples() -> Vec<Self> {
        vec![SessionRead {
            state: "present".into(),
            reason: Value::Null,
            detail: Value::Null,
            now: 1,
            sessions: Default::default(),
        }]
    }
}

impl Shaped for SessionOpened {
    fn samples() -> Vec<Self> {
        vec![
            SessionOpened::Opened,
            SessionOpened::Dirty {
                diff: vec![diff_sample()],
            },
        ]
    }
}

impl Shaped for SessionEdited {
    fn samples() -> Vec<Self> {
        vec![SessionEdited {
            dirty: true,
            diff: vec![diff_sample()],
            errors: vec![cell_error()],
        }]
    }
}

impl Shaped for SessionSaved {
    fn samples() -> Vec<Self> {
        vec![
            SessionSaved::Saved {
                id: "c_1".into(),
                rev: 1,
            },
            SessionSaved::Conflict {
                rev: 2,
                theirs: vec![diff_sample()],
                mine: vec![diff_sample()],
            },
            SessionSaved::Refused {
                errors: vec![cell_error()],
            },
        ]
    }
}

impl Shaped for Placed {
    fn samples() -> Vec<Self> {
        vec![
            Placed::from(SwitchOutcome::Switched),
            Placed::from(SwitchOutcome::Skipped {
                code: "ended".into(),
            }),
        ]
    }
}

impl Shaped for QuotaRead {
    fn samples() -> Vec<Self> {
        vec![crate::faces::quota_read::specimen()]
    }
}

/// ★ S13：每一格错都带写好的那一句；描述 · 为什么里没有引擎没写成字的键。
#[test]
fn every_cell_error_carries_its_written_sentence() {
    for code in [
        "empty", "dup", "tooLong", "type", "range", "of", "time", "overlap", "strategy", "account",
    ] {
        let e = rotation::cell_err("x".into(), code);
        assert!(!e.said.0.is_empty() && !e.said.0.contains('〔'), "{code}");
    }
}

/// ★ 测试档碰不到真家：`Ctx::here()` · `rotation::path_now()` · 额度账 · `home_dir()` 走到的都不在本账号真家里
/// （直接敲 `cargo test` 也一样：测试二进制起来时 `HOME` 若是真家就换成沙箱，`tests/backend/test_home.rs`）。
#[test]
fn test_builds_never_resolve_a_home_inside_the_real_one() {
    let Some(real) = crate::platform::paths::account_home() else {
        return;
    };
    let own = real.join(".cc-monitor");
    let outside = |p: &std::path::Path, what: &str| {
        assert!(!p.starts_with(&own), "{what} 落在真家里：{}", p.display());
    };
    assert_ne!(
        crate::platform::paths::home_dir().as_deref(),
        Some(real.as_path()),
        "home_dir 是真家"
    );
    outside(
        &rotation::path_now().expect("rotation"),
        "rotation::path_now",
    );
    outside(
        Ctx::here().hop.store.path().expect("store"),
        "Ctx::here 的账本",
    );
    outside(
        &ledger::path_from(&|k| std::env::var(k).ok()).expect("ledger"),
        "额度账",
    );
    outside(
        &Ctx::here().hop.store.strategy_dir().expect("dir"),
        "策略目录",
    );
}

/// ★ 轮换文件读不懂：会话那一份带「需手动」（一句 ＋ 复制详情），照上一份读懂的那份接着排、接着挑号；文件一字不动。
#[test]
fn an_unreadable_file_is_surfaced_on_the_session_and_the_last_good_one_keeps_working() {
    let home = Home::new("unread-session");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    answer_session_open_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "config": "c_00000002"}),
    )
    .expect("open");
    assert!(session(&ctx, S1).get("needs").is_none());
    let path = home.root.join(rotation::FILE_NAME);
    std::fs::write(&path, "{broken").expect("w");
    let s = session(&ctx, S1);
    assert_eq!(s["needs"]["kind"], "unreadable");
    assert!(!s["needs"]["reason"].as_str().unwrap_or("").is_empty());
    assert!(!s["needs"]["detail"].as_str().unwrap_or("").is_empty());
    assert_eq!(s["config"]["id"], "c_00000002", "照上一份读懂的接着用");
    assert!(!s["rows"].as_array().expect("rows").is_empty());
    assert_eq!(std::fs::read_to_string(&path).expect("r"), "{broken");
}

/// ★ 推演每行给合图要的、界面不自己算的那几格：离线还剩几点（`left` · `leftText`）· 时段那几段与它们的线（`slots`）·
/// 在用号按用速估的到线时刻（`est`）· 落在视窗外的 7d 重置（`farResets`，带字）。
#[test]
fn the_plan_rows_carry_left_slots_estimate_and_far_resets() {
    use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
    let home = Home::new("plan-rows");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    answer_session_open_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "config": "c_00000002"}),
    )
    .expect("open");
    answer_session_edit_with(
        &ctx, &own_of(&ctx),
        &json!({"sid": S1, "set": [{"acct": "b", "k": "slots", "v": [{"at": "00:00-12:00", "line": 80}, {"at": "12:00-24:00", "line": 0}]}]}),
    )
    .expect("edit");
    let n = now();
    let r = |used: f64| QuotaReading {
        status: Some(QuotaStatus::Allowed),
        refused: false,
        limiting: Some("five_hour".into()),
        resets_at: Some(n + 3_600),
        windows: vec![
            QuotaWindow {
                name: "five_hour".into(),
                used: Some(used),
                resets_at: Some(n + 3_600),
                warned_at: None,
            },
            QuotaWindow {
                name: "seven_day".into(),
                used: Some(0.3),
                resets_at: Some(n + 5 * 86_400),
                warned_at: None,
            },
        ],
        overage: None,
    };
    ledger::record_seen(
        &ctx.hop.quota,
        &own_of(&ctx),
        "claude-code",
        "a",
        r(0.50),
        n - 600,
    );
    ledger::record_seen(
        &ctx.hop.quota,
        &own_of(&ctx),
        "claude-code",
        "a",
        r(0.56),
        n,
    );
    let v = answer_plan_with(&ctx, &json!({"sid": S1}), n).expect("ok");
    let row = |a: &str| {
        v["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|x| x["account"] == a)
            .cloned()
            .expect(a)
    };
    let a = row("a");
    assert_eq!(a["left"], 34, "线 90 − 用了 56");
    assert_eq!(
        a["leftText"],
        copy_text("beRotation.plan.left", &[("n", "34")])
    );
    let est = a["est"]["at"].as_u64().expect("在用号有用速 ⇒ 估的到线");
    assert!(est > n && est <= n + 3_500, "{est}");
    assert!(a["est"]["atText"].is_string());
    let far = a["farResets"].as_array().expect("far");
    assert_eq!(far[0]["w"], "7d");
    assert_eq!(far[0]["at"], n + 5 * 86_400);
    assert!(!far[0]["text"].as_str().unwrap_or("").is_empty());
    let b = row("b");
    let slots = b["slots"].as_array().expect("slots");
    assert!(!slots.is_empty());
    for s in slots {
        assert!(s["from"].as_u64() < s["to"].as_u64());
        let text = s["text"].as_str().unwrap_or("");
        assert!(
            text == copy_text("beRotation.plan.slotLine", &[("n", "80")])
                || text == copy_text("beRotation.plan.slotOff", &[]),
            "{text}"
        );
    }
    assert!(row("b").get("est").is_none(), "不在用的号没有估的到线");
}

/// ★ `_`（起会话时没说是哪个号）不是一个能放的号：它不再顶账号 0 的登录，放上去只能回「需登录」。
/// 会话那一份的候选里没有它（起的号是 `_`、配置里有 `@start` 那一行也一样）；手动放号 · 现在就换点名 `_` ⇒ `bad_args`。
#[test]
fn the_unnamed_account_is_never_a_manual_target() {
    let home = Home::new("unnamed");
    let ctx = home.ctx();
    rotation::face_change(&ctx.hop.store, &own_of(&ctx), |b| {
        b.saw(S1, "claude-code", "_", now())
    })
    .expect("write");
    home.live.lock().expect("lock").insert(S1.to_string());
    answer_session_open_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "config": "c_00000002"}),
    )
    .expect("open");
    let r = session(&ctx, S1);
    assert_eq!(r["state"], "present", "{r}");
    let ids: Vec<&str> = r["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|x| x["account"].as_str().unwrap_or(""))
        .collect();
    assert!(!ids.is_empty(), "候选不该是空的：{r}");
    assert!(!ids.contains(&"_"), "候选里不该有 `_`：{ids:?}");
    for at in [0, 1] {
        let got = answer_session_set_with(
            &ctx,
            &own_of(&ctx),
            &json!({"sid": S1, "place": {"account": "_", "at": at}}),
            now(),
        );
        assert_eq!(
            got.as_ref().err().map(|e| e.code.as_str()),
            Some("bad_args"),
            "放到 `_`（at {at}）应回 bad_args：{got:?}"
        );
    }
    assert!(
        ctx.hop.store.now().sessions[S1].manual.is_empty(),
        "被拒的那两条不该记下任何手动调整"
    );
    assert!(switch_ask(&json!({"sessions": [S1], "target": "_", "mode": "hot"})).is_err());
}

/// ★ 「另 N 会话也在用」只数此刻活着的会话（判活同 `rotation-session-read` 那一处 `ctx.live`）：
/// 自己之外 3 个活的 ＋ 5 个走了的都在 a 上 ⇒ 数 3（自己不算）；走了的会话不算进去。
#[test]
fn also_used_counts_only_live_sessions() {
    let home = Home::new("alsoused");
    let ctx = home.ctx();
    let sid = |i: u32| format!("{i:08x}-0000-4000-8000-000000000000");
    for i in 1..=9 {
        home.saw(&ctx, &sid(i));
    }
    for i in 5..=9 {
        home.end(&sid(i));
    }
    let s = session(&ctx, &sid(1));
    let a = s["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["account"] == "a")
        .expect("a")
        .clone();
    assert_eq!(a["alsoUsed"], 3, "另三个活会话在 a 上；走了的五个不算：{a}");
}

/// ★ 拖一行（`place {account, at}`，`at` 是面板上那张列表的第几行）⇒ 再读，那个号就在第 `at` 行，别的行先后不变。
/// 面板上的列表 ＝ 这一份回的 `rows`（第一行在用、`_` 不在里头），拖的位置也按它算：每一对（从哪行 → 到哪行）都试，一次接一次地拖。
/// 三份内置配置各试一遍；起会话的号有两种：点名了号（a）· 没说是哪个号（`_`，它不进面板那张列表）。
#[test]
fn a_drag_lands_on_the_row_it_was_dropped_on() {
    for start in ["a", crate::accounts::quota::name_words::HOME_ACCOUNT] {
        for cfg in ["c_00000001", "c_00000002", "c_00000003"] {
            drag_every_pair(start, cfg);
        }
    }
}

fn drag_every_pair(start: &str, cfg: &str) {
    let home = Home::new(&format!("dragland-{cfg}-{}", start.replace('_', "u")));
    let ctx = home.ctx();
    rotation::face_change(&ctx.hop.store, &own_of(&ctx), |b| {
        b.saw(S1, "claude-code", start, now())
    })
    .expect("write");
    home.live.lock().expect("lock").insert(S1.to_string());
    answer_session_open_with(&ctx, &own_of(&ctx), &json!({"sid": S1, "config": cfg}))
        .expect("open");
    answer_session_edit_with(
        &ctx,
        &own_of(&ctx),
        &json!({"sid": S1, "add": ["b", "c", "api"]}),
    )
    .expect("add");
    let order = || -> Vec<String> {
        session(&ctx, S1)["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|r| r["account"].as_str().unwrap_or("").to_string())
            .collect()
    };
    let n = order().len();
    assert!(
        n >= 3,
        "{cfg} · 起会话的号 {start}：要至少三行才拖得出名堂：{:?}",
        order()
    );
    for from in 1..n {
        for to in 1..n {
            if from == to {
                continue;
            }
            let before = order();
            let moved = before[from].clone();
            let got = answer_session_set_with(
                &ctx,
                &own_of(&ctx),
                &json!({"sid": S1, "place": {"account": moved, "at": to}}),
                now(),
            )
            .expect("ok");
            assert_eq!(got["state"], "done", "{got}");
            let mut want = before.clone();
            let x = want.remove(from);
            want.insert(to, x);
            assert_eq!(
                order(),
                want,
                "{cfg} · 起会话的号 {start}：把第 {from} 行（{moved}）拖到第 {to} 行；拖之前 {before:?}"
            );
        }
    }
}
