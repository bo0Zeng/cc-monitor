//! 第一版 → 第二版（升级链第一节）：照模块头那张换形表逐行钉。样本现编，纯函数，不碰盘。

use super::*;
use serde_json::{json, Value};

/// 一条第一版规则：`rotation` 原样给。
fn rule(name: &str, rotation: Value) -> Value {
    json!({"name": name, "rotation": rotation})
}

/// 一个第一版会话（`extra` 并进去：`source` · `follow` · `custom` · `history`）。
fn sess(extra: Value) -> Value {
    let mut s =
        json!({"agent": "claude-code", "start": "a", "current": "a", "since": 10, "seen": 20});
    for (k, v) in extra.as_object().expect("对象") {
        s[k] = v.clone();
    }
    s
}

fn run(v: Value) -> Value {
    from_v1(&v, Some("Asia/Shanghai")).expect("读得懂")
}

/// 第二版里某份配置的号列表（每行的 `id` 或 `start`）。
fn rows(cfg: &Value) -> Vec<Value> {
    cfg["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|r| r.get("id").cloned().unwrap_or(json!("<start>")))
        .collect()
}

/// 某份配置里某个号那一行的 `params`。
fn params_of<'a>(cfg: &'a Value, id: &str) -> &'a Value {
    &cfg["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("没有 {id} 那一行：{cfg}"))["params"]
}

/// ★ 每条 `rules[id]` ⇒ `configs[id]`，`strategy: "1"`、名字照留、`version: 2`、`defaultConfig` 照 `defaultRule`；
/// 没有规则 ⇒ 种子那条（`r_00000000`，名字是文案表的「默认」，内容照更早一版的 `default`）；
/// `defaultRule` 指的那条没有 ⇒ 名字最小的那条。
#[test]
fn m1_rules_become_configs_on_strategy_one() {
    let v2 = run(json!({
        "rules": {"r_1": rule("甲", json!({})), "r_2": rule("乙", json!({}))},
        "defaultRule": "r_2",
        "sessions": {}
    }));
    assert_eq!(v2["version"], 2);
    assert_eq!(v2["defaultConfig"], "r_2");
    for (id, name) in [("r_1", "甲"), ("r_2", "乙")] {
        assert_eq!(
            (&v2["configs"][id]["name"], &v2["configs"][id]["strategy"]),
            (&json!(name), &json!("1"))
        );
    }

    let seeded = run(json!({"default": {"preempt": true}, "sessions": {}}));
    assert_eq!(seeded["defaultConfig"], OLD_SEED_ID);
    let seed = &seeded["configs"][OLD_SEED_ID];
    assert_eq!(seed["name"], json!(old_seed_name()));
    assert_eq!(
        seed["params"]["back"], true,
        "种子那条没照更早一版的 default"
    );

    let lost = run(json!({
        "rules": {"r_1": rule("beta", json!({})), "r_2": rule("alpha", json!({}))},
        "defaultRule": "r_9",
        "sessions": {}
    }));
    assert_eq!(
        lost["defaultConfig"], "r_2",
        "defaultRule 指空了该取名字最小的那条"
    );
}

/// ★ `tz`：规则自己写了 ⇒ 照留；缺 ⇒ 做换形那台的本地时区；那台也说不出 ⇒ 不写、标 `tzUnknown`（不猜 UTC）。
#[test]
fn m2_a_missing_tz_takes_the_local_one_or_is_marked_unknown() {
    let book = json!({
        "rules": {"r_1": rule("有", json!({"tz": "Europe/Paris"})), "r_2": rule("缺", json!({}))},
        "defaultRule": "r_1",
        "sessions": {}
    });
    let v2 = run(book.clone());
    assert_eq!(
        (
            &v2["configs"]["r_1"]["tz"],
            &v2["configs"]["r_1"]["tzUnknown"]
        ),
        (&json!("Europe/Paris"), &json!(false))
    );
    assert_eq!(
        (
            &v2["configs"]["r_2"]["tz"],
            &v2["configs"]["r_2"]["tzUnknown"]
        ),
        (&json!("Asia/Shanghai"), &json!(false))
    );
    let blind = from_v1(&book, None).expect("读得懂");
    assert_eq!(
        (
            &blind["configs"]["r_2"]["tz"],
            &blind["configs"]["r_2"]["tzUnknown"]
        ),
        (&Value::Null, &json!(true))
    );
    assert_eq!(blind["configs"]["r_1"]["tzUnknown"], false);
}

/// ★ `order` ＋ `enabled` ⇒ `accounts`：按 `order` 的先后；开会话的号那一格照留；具名的只留勾上的；重复的只留头一个。
#[test]
fn m3_order_and_enabled_become_the_account_rows() {
    let v2 = run(json!({
        "rules": {"r_1": rule("x", json!({
            "order": ["c", {"start": true}, "b", "d", "c"],
            "enabled": ["b", "c"]
        }))},
        "defaultRule": "r_1",
        "sessions": {}
    }));
    assert_eq!(
        rows(&v2["configs"]["r_1"]),
        vec![json!("c"), json!("<start>"), json!("b")]
    );
}

/// ★ 线：`cap["*"]` ⇒ `acctInit.line`，并给每一行没有自己那一位的补上；`cap[号]` 的 `5h` · `7d` · `7d:<模型>`（并进 `7d` 取小的）⇒ 那一行的
/// `params.line`；两位一样写成一个数；号写死 `0` ⇒ 不进线、整天不用（`slots: [{at: "00:00-24:00", line: 0}]`）。
#[test]
fn m4_caps_become_lines() {
    let v2 = run(json!({
        "rules": {"r_1": rule("x", json!({
            "order": [{"start": true}, "b", "c", "d"],
            "enabled": ["b", "c", "d"],
            "cap": {
                "*": {"5h": 80},
                "b": {"7d": 60, "7d:opus": 40},
                "c": {"*": 70},
                "d": {"5h": 0}
            }
        }))},
        "defaultRule": "r_1",
        "sessions": {}
    }));
    let c = &v2["configs"]["r_1"];
    assert_eq!(c["acctInit"]["line"], json!({"5h": 80}));
    assert_eq!(
        c["accounts"][0]["params"]["line"],
        json!({"5h": 80}),
        "开会话的号那一行没补上 * 的线"
    );
    assert_eq!(params_of(c, "b")["line"], json!({"5h": 80, "7d": 40}));
    assert_eq!(params_of(c, "c")["line"], json!(70));
    assert_eq!(params_of(c, "d")["line"], json!({"5h": 80}));
    assert_eq!(
        params_of(c, "d")["slots"],
        json!([{"at": "00:00-24:00", "line": 0}])
    );
}

/// ★ 按时段写的上限 ⇒ 那一行的 `params.slots`（`n` ⇒ `line`）；和前面某段重叠的后来者丢掉（跨午夜的也算重叠）。
#[test]
fn m5_timed_caps_become_slots_and_overlaps_are_dropped() {
    let v2 = run(json!({
        "rules": {"r_1": rule("x", json!({
            "order": ["b"],
            "enabled": ["b"],
            "cap": {"b": {
                "5h": [{"at": "22:00-02:00", "n": 30}, {"at": "09:00-12:00", "n": 50}],
                "7d": [{"at": "01:00-03:00", "n": 10}, {"at": "12:00-13:00", "n": 60}]
            }}
        }))},
        "defaultRule": "r_1",
        "sessions": {}
    }));
    assert_eq!(
        params_of(&v2["configs"]["r_1"], "b")["slots"],
        json!([
            {"at": "22:00-02:00", "line": 30},
            {"at": "09:00-12:00", "line": 50},
            {"at": "12:00-13:00", "line": 60}
        ])
    );
}

/// ★ `stint` ⇒ 每行 `params.stint`：号自己的先，没有取整份（`*`）的；分窗口的取 `5h`，没有取 `7d`，再没有取 `*`。
#[test]
fn m6_stints_go_to_each_row() {
    let v2 = run(json!({
        "rules": {"r_1": rule("x", json!({
            "order": ["b", "c", "d"],
            "enabled": ["b", "c", "d"],
            "stint": {"*": {"*": 9}, "b": {"5h": 3, "7d": 4}, "c": {"7d": 5}}
        }))},
        "defaultRule": "r_1",
        "sessions": {}
    }));
    let c = &v2["configs"]["r_1"];
    assert_eq!(params_of(c, "b")["stint"], 3);
    assert_eq!(params_of(c, "c")["stint"], 5);
    assert_eq!(params_of(c, "d")["stint"], 9);
    assert_eq!(c["acctInit"]["stint"], 9);
}

/// ★ `preempt` · `wait` · `fallback` · `atLimit` ⇒ `params.back` · `params.standby_wait`（缺省 40）· 那一行 `mode: "standby"` ·
/// `params.all_out`（`stop` ⇒ `stop`，别的 ⇒ `go`）；`params.delay` 恒 0。
#[test]
fn m7_switch_knobs_become_params() {
    let v2 = run(json!({
        "rules": {
            "r_1": rule("x", json!({"order": ["b", "c"], "enabled": ["b", "c"], "preempt": true, "wait": 15,
                                    "fallback": ["c"], "atLimit": "stop"})),
            "r_2": rule("y", json!({"atLimit": "continue"}))
        },
        "defaultRule": "r_1",
        "sessions": {}
    }));
    let x = &v2["configs"]["r_1"];
    assert_eq!(
        x["params"],
        json!({"back": true, "delay": 0, "standby_wait": 15, "all_out": "stop"})
    );
    assert_eq!(
        (
            params_of(x, "b")["mode"].as_str(),
            params_of(x, "c")["mode"].as_str()
        ),
        (Some("normal"), Some("standby"))
    );
    assert_eq!(
        v2["configs"]["r_2"]["params"],
        json!({"back": false, "delay": 0, "standby_wait": 40, "all_out": "go"})
    );
}

/// ★ 会话：`follow`（或没写 `source`、`follow` 不为否）⇒ 默认那份；`rule` ⇒ 那一份（没有了 ⇒ 默认那份）；`custom` ⇒ 默认那份 ＋ 副本
/// （副本与默认那份一样 ⇒ 不带；副本的时区随默认那份）；`parent` ⇒ 追到的那个会话打开的那一份（不带它的副本），追不到 ⇒ 默认那份。
/// `config.rev` 恒 1；`agent` · `start` · `current` · `since` · `seen` 照搬。
#[test]
fn m8_sessions_open_the_config_their_source_points_at() {
    let v2 = run(json!({
        "rules": {"r_1": rule("默认那份", json!({"tz": "UTC"})), "r_2": rule("另一份", json!({}))},
        "defaultRule": "r_1",
        "sessions": {
            "s_follow": sess(json!({"source": "follow"})),
            "s_bare": sess(json!({})),
            "s_rule": sess(json!({"source": {"rule": "r_2"}})),
            "s_gone": sess(json!({"source": {"rule": "r_9"}})),
            "s_custom": sess(json!({"source": "custom", "custom": {"preempt": true}})),
            "s_same": sess(json!({"source": "custom", "custom": {"tz": "UTC"}})),
            "s_oldcustom": sess(json!({"follow": false, "custom": {"preempt": true}})),
            "s_child": sess(json!({"source": {"parent": "s_mid"}})),
            "s_mid": sess(json!({"source": {"parent": "s_rule"}})),
            "s_orphan": sess(json!({"source": {"parent": "nobody"}}))
        }
    }));
    let s = &v2["sessions"];
    let opened = |sid: &str| s[sid]["config"]["id"].as_str().map(str::to_string);
    for (sid, want) in [
        ("s_follow", "r_1"),
        ("s_bare", "r_1"),
        ("s_rule", "r_2"),
        ("s_gone", "r_1"),
        ("s_custom", "r_1"),
        ("s_same", "r_1"),
        ("s_oldcustom", "r_1"),
        ("s_child", "r_2"),
        ("s_mid", "r_2"),
        ("s_orphan", "r_1"),
    ] {
        assert_eq!(opened(sid).as_deref(), Some(want), "{sid}");
        assert_eq!(s[sid]["config"]["rev"], 1, "{sid}");
    }
    let copy = &s["s_custom"]["config"]["copy"];
    assert_eq!(copy["params"]["back"], true, "custom 的副本没带上");
    assert_eq!(
        (&copy["name"], &copy["tz"]),
        (&json!("默认那份"), &json!("UTC")),
        "副本的名字 · 时区该随默认那份"
    );
    assert!(
        s["s_oldcustom"]["config"].get("copy").is_some(),
        "follow: false 该当 custom"
    );
    for sid in ["s_same", "s_follow", "s_child"] {
        assert!(s[sid]["config"].get("copy").is_none(), "{sid} 不该带副本");
    }
    assert_eq!(
        [
            &s["s_rule"]["agent"],
            &s["s_rule"]["start"],
            &s["s_rule"]["current"],
            &s["s_rule"]["since"],
            &s["s_rule"]["seen"]
        ],
        [
            &json!("claude-code"),
            &json!("a"),
            &json!("a"),
            &json!(10),
            &json!(20)
        ]
    );
}

/// ★ 换号记录：`history[].why` ⇒ `leave`（只留下一节认的那几种：用满 / 到线 / 关掉 / 跳过 ⇒ `unusable` · 手动热换 ⇒ `manual` · 手动重开 ⇒
/// `manualRestart` · 停发 / 转按量 ⇒ `held` · 按量 ⇒ `overage` · 别的 ⇒ `top`）；`at` · `from` · `to` · `fromResetsAt` 照搬。
/// `blockedAbove` 丢掉（出来的第二版里哪儿都没有）。
#[test]
fn m9_history_reasons_become_leaves_and_blocked_above_is_dropped() {
    let whys = [
        (json!("full"), "unusable"),
        (json!({"threshold": {"n": 80}}), "unusable"),
        (json!("off"), "unusable"),
        (json!("skipped"), "unusable"),
        (json!("manualHot"), "manual"),
        (json!("manualRestart"), "manualRestart"),
        (json!("held"), "held"),
        (json!("toOverage"), "held"),
        (json!("overage"), "overage"),
        (json!("order"), "top"),
        (json!(7), "top"),
    ];
    let history: Vec<Value> = whys
        .iter()
        .enumerate()
        .map(|(i, (why, _))| json!({"at": i, "from": "a", "to": "b", "why": why, "fromResetsAt": 99}))
        .collect();
    let v2 = run(json!({
        "rules": {"r_1": rule("x", json!({"blockedAbove": 90}))},
        "defaultRule": "r_1",
        "sessions": {"s": sess(json!({"history": history, "blockedAbove": 90}))}
    }));
    let got = v2["sessions"]["s"]["history"].as_array().expect("history");
    assert_eq!(got.len(), whys.len());
    for (i, ((why, want), h)) in whys.iter().zip(got).enumerate() {
        assert_eq!(h["leave"], json!({"k": want}), "why = {why}");
        assert_eq!(
            [&h["at"], &h["from"], &h["to"], &h["fromResetsAt"]],
            [&json!(i), &json!("a"), &json!("b"), &json!(99)]
        );
    }
    assert!(
        !v2.to_string().contains("blockedAbove"),
        "blockedAbove 该丢掉：{v2}"
    );
}

/// ★ 更早的 `when.threshold.n`（1–99）⇒ `cap["*"]` 没写的那一位的线（每一行跟着补）；`"full"` 或形状不对 ⇒ 不补。
#[test]
fn m10_an_older_threshold_fills_the_unwritten_line_positions() {
    let v2 = run(json!({
        "rules": {
            "r_1": rule("x", json!({"order": ["b"], "enabled": ["b"], "cap": {"*": {"7d": 90}},
                                    "when": {"threshold": {"n": 80}}})),
            "r_2": rule("y", json!({"when": "full"})),
            "r_3": rule("z", json!({"when": {"threshold": {"n": 100}}}))
        },
        "defaultRule": "r_1",
        "sessions": {}
    }));
    let x = &v2["configs"]["r_1"];
    assert_eq!(x["acctInit"]["line"], json!({"5h": 80, "7d": 90}));
    assert_eq!(params_of(x, "b")["line"], json!({"5h": 80, "7d": 90}));
    assert_eq!(v2["configs"]["r_2"]["acctInit"]["line"], Value::Null);
    assert_eq!(v2["configs"]["r_3"]["acctInit"]["line"], Value::Null);
}

/// ★ 读不懂（会话缺必填的格）⇒ `Err`，不出半份。
#[test]
fn m11_an_unreadable_v1_is_an_error() {
    assert!(from_v1(&json!({"sessions": {"s": {"agent": "claude-code"}}}), None).is_err());
}
