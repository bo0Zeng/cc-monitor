//! 待答请求的成品：每一种的选项按什么序 · 带不带附言 · 题目与选项的编号；答题 hook 没挂上的原因码往返与那一句；
//! 插件目录清单的拼法。夹具全是合成的（`AskIn` 手搭）。
use super::*;
use crate::agents::{AskOption, AskQuestion, AskSuggestion};

fn ask_in(kind: AskKind) -> AskIn {
    AskIn {
        sid: "s1".into(),
        kind,
        tool: "Tool".into(),
        what: Some("w".into()),
        detail: "full".into(),
        suggestions: Vec::new(),
        questions: Vec::new(),
        agent_id: None,
        agent_type: None,
        blocks_terminal: false,
        input: Value::Null,
    }
}

fn ids(a: &Ask) -> Vec<&str> {
    a.choices.iter().map(|c| c.id.as_str()).collect()
}

fn rows(a: &Ask) -> Vec<(&str, ChoiceRow)> {
    a.choices.iter().map(|c| (c.id.as_str(), c.row)).collect()
}

/// 每一项排在哪一段由这里定：拒绝一族 · 继续规划另起一段，改到终端里答 · 有题时的不回答放到最后那一段，其余照做那一段。
#[test]
fn each_choice_carries_the_row_it_goes_in() {
    use ChoiceRow::{Hold, Main, Release};
    let mut a = ask_in(AskKind::Approve);
    a.blocks_terminal = true;
    a.suggestions = vec![AskSuggestion {
        text: "r".into(),
        raw: Value::Null,
    }];
    assert_eq!(
        rows(&product_of("a", &a, 1)),
        [
            ("allow", Main),
            ("always-0", Main),
            ("deny", Hold),
            ("deny-note", Hold),
            ("deny-stop", Hold),
            ("release", Release)
        ]
    );
    let plan = product_of("a", &ask_in(AskKind::Plan), 1);
    assert_eq!(rows(&plan).last(), Some(&("plan-keep", Hold)));
    assert!(rows(&plan)[..3].iter().all(|(_, r)| *r == Main));
    assert_eq!(
        rows(&product_of("a", &ask_in(AskKind::Question), 1)),
        [("dismiss", Main)],
        "没有题 ⇒ 不回答是唯一那一项"
    );
    let mut q = ask_in(AskKind::Question);
    q.questions = vec![AskQuestion {
        header: None,
        question: "first".into(),
        multi: false,
        options: Vec::new(),
    }];
    assert_eq!(
        rows(&product_of("a", &q, 1)),
        [("answer", Main), ("dismiss", Release)]
    );
}

/// 批准：允许在前、「以后不再问」那几条按建议序居中（`always-<i>`、字里带那条建议）、三种拒在后；只有「拒并附言」要附言。
#[test]
fn approve_puts_allow_first_then_the_suggestions_then_the_denials() {
    let mut a = ask_in(AskKind::Approve);
    a.suggestions = vec![
        AskSuggestion {
            text: "rule-a".into(),
            raw: Value::Null,
        },
        AskSuggestion {
            text: "rule-b".into(),
            raw: Value::Null,
        },
    ];
    let p = product_of("ask-9", &a, 42);
    assert_eq!(
        ids(&p),
        [
            "allow",
            "always-0",
            "always-1",
            "deny",
            "deny-note",
            "deny-stop"
        ]
    );
    assert_eq!(
        p.choices[2].text.0,
        copy_text("beAsk.choice.allowAlways", &[("rule", "rule-b")])
    );
    let notes: Vec<_> = p.choices.iter().map(|c| (c.id.as_str(), c.note)).collect();
    for (id, n) in notes {
        assert_eq!(n == NoteNeed::Required, id == "deny-note", "{id}");
    }
    let warns: Vec<_> = p
        .choices
        .iter()
        .filter(|c| c.tone == Tone::Warn)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(warns, ["deny", "deny-note", "deny-stop"]);
    assert_eq!(
        (p.id.as_str(), p.since_ms, p.run.clone()),
        ("ask-9", 42, None)
    );
}

/// 批计划：四档，最后那档「留在计划里」可附一句话。
#[test]
fn plan_offers_the_three_modes_and_keep_planning() {
    let p = product_of("a", &ask_in(AskKind::Plan), 1);
    assert_eq!(
        ids(&p),
        ["plan-auto", "plan-edits", "plan-manual", "plan-keep"]
    );
    assert_eq!(p.choices[3].note, NoteNeed::Optional);
    assert!(p.questions.is_empty());
}

/// 提问：有题才有「回答」；题目与选项编号 `q<i>` · `q<i>o<j>`，每题都许自由文本。
#[test]
fn a_question_numbers_its_questions_and_options() {
    let p = product_of("a", &ask_in(AskKind::Question), 1);
    assert_eq!(ids(&p), ["dismiss"], "没有题就没有「回答」");

    let mut a = ask_in(AskKind::Question);
    a.questions = vec![
        AskQuestion {
            header: None,
            question: "first".into(),
            multi: false,
            options: Vec::new(),
        },
        AskQuestion {
            header: Some("H".into()),
            question: "second".into(),
            multi: true,
            options: vec![
                AskOption {
                    label: "x".into(),
                    description: None,
                },
                AskOption {
                    label: "y".into(),
                    description: Some("why".into()),
                },
            ],
        },
    ];
    let p = product_of("a", &a, 1);
    assert_eq!(ids(&p), ["answer", "dismiss"]);
    let q = &p.questions[1];
    assert_eq!(
        (q.id.as_str(), q.text.as_str(), q.header.as_deref(), q.multi),
        ("q1", "second", Some("H"), true)
    );
    let opts: Vec<_> = q
        .options
        .iter()
        .map(|o| (o.id.as_str(), o.text.as_str(), o.detail.as_deref()))
        .collect();
    assert_eq!(opts, [("q1o0", "x", None), ("q1o1", "y", Some("why"))]);
    assert!(p.questions.iter().all(|q| q.other));
}

/// 挡着终端的请求每种都多一项「改到终端里答」，排最后；子 agent 发的带上它的 id 与种类。
#[test]
fn a_terminal_blocking_ask_gets_release_last_and_a_run_carries_the_agent() {
    for kind in [AskKind::Approve, AskKind::Plan, AskKind::Question] {
        let mut a = ask_in(kind);
        a.blocks_terminal = true;
        a.agent_id = Some("ag".into());
        a.agent_type = Some("general".into());
        let p = product_of("a", &a, 1);
        assert_eq!(ids(&p).last(), Some(&"release"), "{kind:?}");
        assert_eq!(ids(&p).iter().filter(|i| **i == "release").count(), 1);
        assert_eq!(
            p.run,
            Some(AskRun {
                id: "ag".into(),
                kind: Some("general".into())
            })
        );
    }
}

/// 成品的线上形状：驼峰、不认识的格拒收（格目录样本往返原样）。
#[test]
fn the_product_is_camel_case_and_strict() {
    let s = specimen();
    let v = serde_json::to_value(&s).unwrap();
    assert!(v.get("sinceMs").is_some() && v.get("since_ms").is_none());
    assert_eq!(serde_json::from_value::<Ask>(v.clone()).unwrap(), s);
    let mut extra = v;
    extra
        .as_object_mut()
        .unwrap()
        .insert("x".into(), Value::Bool(true));
    assert!(serde_json::from_value::<Ask>(extra).is_err());
}

/// 原因码：每一种写进去再读回来是同一种、码两两不同；认不出的码 ⇒ 没原因（只说没接上，不编）。
#[test]
fn hook_off_codes_round_trip_and_unknown_codes_carry_no_reason() {
    let all = [
        HookOff::Settings(SettingsWhy::Unread),
        HookOff::Settings(SettingsWhy::NotJson),
        HookOff::Settings(SettingsWhy::Shape),
        HookOff::Launcher,
        HookOff::Policy,
        HookOff::Version,
        HookOff::EnvLost,
    ];
    let mut codes = std::collections::BTreeSet::new();
    for o in all {
        assert!(o.env_value().is_ascii());
        assert!(codes.insert(o.env_value()), "码重了：{}", o.env_value());
        assert_eq!(HookOff::from_env_value(o.env_value()), Some(o));
    }
    assert_eq!(HookOff::from_env_value(""), None);
    assert_eq!(HookOff::from_env_value("settings"), None);
    assert_eq!(
        hook_off_words("??").0,
        copy_text("beAsk.manual.offUnknown", &[])
    );
}

/// 那一句照原因说；设置读不出 / 不是 JSON / 形状不对三种各嵌各的原因。
#[test]
fn hook_off_words_name_the_reason() {
    assert_eq!(
        hook_off_words("launcher").0,
        copy_text("beAsk.manual.offLauncher", &[])
    );
    assert_eq!(
        hook_off_words("env-lost").0,
        copy_text("beAsk.manual.offEnvLost", &[])
    );
    for (code, why) in [
        ("settings-unread", "beAsk.manual.whyUnread"),
        ("settings-json", "beAsk.manual.whyJson"),
        ("settings-shape", "beAsk.manual.whyShape"),
    ] {
        let why = copy_text(why, &[]);
        assert_eq!(
            hook_off_words(code).0,
            copy_text("beAsk.manual.offSettings", &[("why", &why)]),
            "{code}"
        );
    }
}

/// 插件目录清单：空 / 没有 ⇒ 就是这一项；已有 ⇒ 不重复加；别的 ⇒ 按这台的分隔符接在后面。按整项比，不按子串。
#[test]
fn plugin_dir_list_appends_once_and_matches_whole_entries() {
    let sep = DIRS_SEP;
    assert_eq!(dirs_with(None, "/p"), "/p");
    assert_eq!(dirs_with(Some(""), "/p"), "/p");
    let two = format!("/a{sep}/p");
    assert_eq!(dirs_with(Some(&two), "/p"), two);
    assert_eq!(dirs_with(Some("/a"), "/p"), two);
    assert!(dirs_have(&two, "/p") && dirs_have(&two, "/a"));
    assert!(!dirs_have(&format!("/a{sep}/pp"), "/p"), "子串不算");
    let d = plugin_dir_under("/h");
    assert!(d.starts_with("/h") && d.len() > "/h/".len(), "{d}");
    assert_eq!(
        std::path::Path::new(&d),
        std::path::Path::new("/h").join(relay_route_core::ANSWER_PLUGIN_DIR_REL)
    );
}
