//! 待答请求的成品：每一种的选项按什么序 · 带不带附言 · 题目与选项的编号；答题 hook 没挂上的原因码往返与那一句；
//! 插件目录清单的拼法。夹具全是合成的（`AskIn` 手搭）。
use super::*;
use crate::agents::{AskOption, AskQuestion, AskSuggestion, SuggestRule, SuggestScope};

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

/// 别的判据都按写得出停下的那一家搭成品（「拒绝并停下」那一项在不在只在 [`deny_stop_only_for_a_family_that_can_stop`] 判）。
const STOPS: bool = true;

fn ids(a: &Ask) -> Vec<&str> {
    a.choices.iter().map(|c| c.id.as_str()).collect()
}

fn rows(a: &Ask) -> Vec<(&str, ChoiceRow)> {
    a.choices.iter().map(|c| (c.id.as_str(), c.row)).collect()
}

fn looks(a: &Ask) -> Vec<(&str, ChoiceRow, ChoiceLook, bool)> {
    a.choices
        .iter()
        .map(|c| (c.id.as_str(), c.row, c.look, c.picks))
        .collect()
}

fn one_question(multi: bool, header: Option<&str>) -> AskIn {
    let mut q = ask_in(AskKind::Answer);
    q.questions = vec![AskQuestion {
        header: header.map(str::to_string),
        question: "first".into(),
        multi,
        options: Vec::new(),
    }];
    q
}

/// 每颗选项的样子 · 段 · 带不带 picks 只在这里判（出口照摆，不按 id · 次序自己挑）：
/// 每一种的第一颗主按钮、只这一颗；停下整轮危险；有题时的不回答弱；批计划 · 有题的提问全在一段。
#[test]
fn each_choice_carries_its_look_row_and_picks() {
    use ChoiceLook::{Danger, Ghost, Main as M, Plain};
    use ChoiceRow::{Hold, Main};
    let plan = ask_in(AskKind::Plan);
    assert_eq!(
        looks(&product_of("a", &plan, STOPS, false, None)),
        [
            ("plan-auto", Main, M, false),
            ("plan-edits", Main, Plain, false),
            ("plan-manual", Main, Plain, false),
            ("plan-keep", Main, Plain, false),
        ],
        "批计划：一段，第一颗主按钮"
    );
    let a = ask_in(AskKind::Approve);
    assert_eq!(
        looks(&product_of("a", &a, STOPS, false, None)),
        [
            ("allow", Main, M, false),
            ("deny", Hold, Plain, false),
            ("deny-note", Hold, Plain, false),
            ("deny-stop", Hold, Danger, false),
        ],
        "批准：两段"
    );
    assert_eq!(
        looks(&product_of(
            "a",
            &one_question(false, None),
            STOPS,
            false,
            None
        )),
        [("answer", Main, M, true), ("dismiss", Main, Ghost, false),],
        "有题：一段，只有提交答案带 picks"
    );
    assert_eq!(
        looks(&product_of(
            "a",
            &ask_in(AskKind::Answer),
            STOPS,
            false,
            None
        )),
        [("dismiss", Main, M, false)],
        "没有题 ⇒ 不回答是唯一那一项、就是主按钮"
    );
}

/// 「拒绝并停下」只给拒绝时写得出停下的那一家：替身家声明不能 ⇒ 成品里没有那一项（别的照旧、次序不变）；Claude 照旧有。
#[test]
fn deny_stop_only_for_a_family_that_can_stop() {
    let a = ask_in(AskKind::Approve);
    let fake = crate::agents::fake::ASK;
    assert!(!fake.stop_on_deny, "替身家声明停不下");
    assert_eq!(
        ids(&product_of("a", &a, fake.stop_on_deny, false, None)),
        ["allow", "deny", "deny-note"],
        "停不下的那一家：成品里不带「拒绝并停下」"
    );
    let claude = crate::agents::ask_face("claude").expect("claude 有答题口");
    assert!(claude.stop_on_deny, "Claude 声明停得下");
    assert_eq!(
        ids(&product_of("a", &a, claude.stop_on_deny, false, None)),
        ["allow", "deny", "deny-note", "deny-stop"],
        "Claude：照旧有「拒绝并停下」"
    );
}

/// 不论哪一种、有没有题（在这里答的那几种）：恰好一颗主按钮，就是照做那一段的第一颗。
#[test]
fn every_ask_has_exactly_one_main_and_it_leads_the_main_row() {
    let mut cases: Vec<AskIn> = [AskKind::Approve, AskKind::Plan, AskKind::Answer]
        .into_iter()
        .map(ask_in)
        .collect();
    cases.push(one_question(true, Some("H")));
    for a in &cases {
        let p = product_of("a", a, STOPS, false, None);
        let mains: Vec<_> = p
            .choices
            .iter()
            .filter(|c| c.look == ChoiceLook::Main)
            .collect();
        let lead = p.choices.iter().find(|c| c.row == ChoiceRow::Main);
        assert_eq!(mains.len(), 1, "{:?}", looks(&p));
        assert_eq!(Some(mains[0]), lead, "{:?}", looks(&p));
    }
}

/// 题头那一句由这里写好：多选带「多选」那一说法（有题头 · 没题头两种），单选照题头原样、没有就没有。
#[test]
fn a_question_head_is_written_here() {
    let head = |multi, header| {
        product_of("a", &one_question(multi, header), STOPS, false, None).questions[0]
            .head
            .clone()
    };
    assert_eq!(
        head(true, Some("H")),
        Some(copy_text("beAsk.question.multi", &[("header", "H")]))
    );
    assert_eq!(
        head(true, None),
        Some(copy_text("beAsk.question.multiBare", &[]))
    );
    assert_eq!(head(false, Some("H")), Some("H".to_string()));
    assert_eq!(head(false, None), None);
}

/// 每一项排在哪一段由这里定：批准分两段（照做 · 拒绝一族）；批计划 · 有题的提问全在照做那一段。
#[test]
fn each_choice_carries_the_row_it_goes_in() {
    use ChoiceRow::{Hold, Main};
    let mut a = ask_in(AskKind::Approve);
    a.suggestions = vec![allow_dirs(&["/r"])];
    assert_eq!(
        rows(&product_of("a", &a, STOPS, false, None)),
        [
            ("allow", Main),
            ("always-0", Main),
            ("deny", Hold),
            ("deny-note", Hold),
            ("deny-stop", Hold),
        ]
    );
    let plan = product_of("a", &ask_in(AskKind::Plan), STOPS, false, None);
    assert!(rows(&plan).iter().all(|(_, r)| *r == Main));
    assert_eq!(
        rows(&product_of(
            "a",
            &ask_in(AskKind::Answer),
            STOPS,
            false,
            None
        )),
        [("dismiss", Main)],
        "没有题 ⇒ 不回答是唯一那一项"
    );
    let mut q = ask_in(AskKind::Answer);
    q.questions = vec![AskQuestion {
        header: None,
        question: "first".into(),
        multi: false,
        options: Vec::new(),
    }];
    assert_eq!(
        rows(&product_of("a", &q, STOPS, false, None)),
        [("answer", Main), ("dismiss", Main)]
    );
}

/// 批准：允许在前、「以后不再问」那几条按建议序居中（`always-<i>`、字里带那条建议）、三种拒在后；只有「拒并附言」要附言。
#[test]
fn approve_puts_allow_first_then_the_suggestions_then_the_denials() {
    let mut a = ask_in(AskKind::Approve);
    a.suggestions = vec![allow_dirs(&["/a"]), allow_dirs(&["/b"])];
    let p = product_of("ask-9", &a, STOPS, false, None);
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
        copy_text(
            "beAsk.choice.allowAlways",
            &[("rule", &allow_dirs(&["/b"]).said())]
        )
    );
    let notes: Vec<_> = p.choices.iter().map(|c| (c.id.as_str(), c.note)).collect();
    for (id, n) in notes {
        assert_eq!(n == NoteNeed::Required, id == "deny-note", "{id}");
    }
    let looks: Vec<_> = p.choices.iter().map(|c| c.look).collect();
    use ChoiceLook::{Danger, Main, Plain};
    assert_eq!(
        looks,
        [Main, Plain, Plain, Plain, Plain, Danger],
        "拒绝并停下标红：出口照 look，不认 id"
    );
    assert_eq!((p.id.as_str(), p.run.clone()), ("ask-9", None));
}

/// 批计划：四档，最后那档「留在计划里」可附一句话。
#[test]
fn plan_offers_the_three_modes_and_keep_planning() {
    let p = product_of("a", &ask_in(AskKind::Plan), STOPS, false, None);
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
    let p = product_of("a", &ask_in(AskKind::Answer), STOPS, false, None);
    assert_eq!(ids(&p), ["dismiss"], "没有题就没有「回答」");

    let mut a = ask_in(AskKind::Answer);
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
    let p = product_of("a", &a, STOPS, false, None);
    assert_eq!(ids(&p), ["answer", "dismiss"]);
    let q = &p.questions[1];
    assert_eq!(
        (q.id.as_str(), q.text.as_str(), q.multi),
        ("q1", "second", true)
    );
    let opts: Vec<_> = q
        .options
        .iter()
        .map(|o| (o.id.as_str(), o.text.as_str(), o.detail.as_deref()))
        .collect();
    assert_eq!(opts, [("q1o0", "x", None), ("q1o1", "y", Some("why"))]);
    assert!(p.questions.iter().all(|q| q.other));
}

/// 挡着终端的请求（终端里有框、在终端里答）：每种都一颗按钮不给；子 agent 发的照样带上它的 id 与写好的「谁要批」。
#[test]
fn a_terminal_blocking_ask_offers_no_choices_and_a_run_carries_the_agent() {
    for kind in [AskKind::Approve, AskKind::Plan, AskKind::Answer] {
        let mut a = ask_in(kind);
        a.blocks_terminal = true;
        a.agent_id = Some("ag".into());
        a.agent_type = Some("general".into());
        let p = product_of("a", &a, STOPS, false, None);
        assert!(p.choices.is_empty(), "{kind:?}：{:?}", ids(&p));
        let at = |ask| Asked { ask, since_ms: 0 }.manual().map(|w| w.0);
        assert_eq!(
            at(p.clone()),
            Some(copy_text("beAsk.manual.inTerminal", &[])),
            "答的位置写在终端里答"
        );
        a.blocks_terminal = false;
        assert_eq!(
            at(product_of("a", &a, STOPS, false, None)),
            None,
            "在这里答得了 ⇒ 不写"
        );
        assert_eq!(
            p.run,
            Some(AskRun {
                id: "ag".into(),
                text: Words(copy_text("askBlock.tag.run", &[("kind", "general")])),
            })
        );
    }
}

/// 「谁要批」那一句：名 · 类别缺哪样少哪样，都缺 ⇒ 只写 agent；主会话发的没有这一格。
#[test]
fn who_asks_names_the_agent_and_its_kind_with_what_is_known() {
    let mut a = ask_in(AskKind::Approve);
    assert_eq!(
        product_of("a", &a, STOPS, false, Some("l")).run,
        None,
        "主会话发的不带 run"
    );
    a.agent_id = Some("ag".into());
    let text = |a: &AskIn, label: Option<&str>| {
        product_of("a", a, STOPS, false, label).run.unwrap().text.0
    };
    let named = copy_text("speaker.agent.title", &[("label", "Audit")]);
    assert_eq!(text(&a, Some("Audit")), named);
    assert_eq!(text(&a, None), copy_text("beProfile.slot.agent", &[]));
    a.agent_type = Some("Explore".into());
    assert_eq!(
        text(&a, Some("Audit")),
        format!("{named}{}Explore", copy_text("kit.text.sep", &[]))
    );
}

/// 成品的线上形状：驼峰、不认识的格拒收（格目录样本往返原样）。
#[test]
fn the_product_is_camel_case_and_strict() {
    let s = specimen();
    let v = serde_json::to_value(&s).unwrap();
    let back = serde_json::from_value::<Ask>(v.clone()).unwrap();
    assert_eq!(back, s);
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
        HookOff::Elsewhere,
        HookOff::NoAnswer,
        HookOff::Overridden,
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
    assert_eq!(
        hook_off_words("no-answer").0,
        copy_text("beAsk.manual.offNoAnswer", &[])
    );
    assert_eq!(
        hook_off_words("overridden").0,
        copy_text("beAsk.manual.offOverridden", &[])
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
    let d = plugin_dir_under(std::path::Path::new("/h")).expect("家说得出");
    assert!(
        d.starts_with("/h") && d.as_os_str().len() > "/h/".len(),
        "{d:?}"
    );
    assert_eq!(
        std::path::Path::new(&d),
        std::path::Path::new("/h").join(relay_route_core::ANSWER_PLUGIN_DIR_REL)
    );
}

/// 插件目录只在家说得出时才有（判只在 `platform::paths::under_home`）：空家照实拒、不拼成相对路径；
/// 家名里有非 UTF-8 的字节 ⇒ 原样留着（不经字符串转一道、拼出另一个目录）。
#[test]
fn plugin_dir_refuses_an_empty_home_and_keeps_the_homes_bytes() {
    let said = plugin_dir_under(std::path::Path::new("")).expect_err("空家拼成了相对路径");
    assert_eq!(
        said,
        crate::platform::paths::under_home(None, relay_route_core::ANSWER_PLUGIN_DIR_REL)
            .unwrap_err(),
        "拒的那一句不是 `under_home` 那一句"
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let home = std::path::Path::new(std::ffi::OsStr::from_bytes(b"/h\xffome"));
        let d = plugin_dir_under(home).expect("家说得出");
        assert!(d.starts_with(home), "非 UTF-8 的家读偏了：{d:?}");
        assert_eq!(
            plugin_dir_in_env(home),
            None,
            "环境变量那一串装不下，不换字节硬塞"
        );
    }
    // 环境变量里那一项：家说不出 ⇒ 没有；说得出 ⇒ 就是插件目录那一串。
    assert_eq!(plugin_dir_in_env(std::path::Path::new("")), None);
    assert_eq!(
        plugin_dir_in_env(std::path::Path::new("/h")).as_deref(),
        plugin_dir_under(std::path::Path::new("/h"))
            .unwrap()
            .to_str()
    );
}

/// 一条「这次会话里允许访问这几个目录」的建议（码 ＋ 空原样）。
fn allow_dirs(dirs: &[&str]) -> AskSuggestion {
    AskSuggestion {
        scope: SuggestScope::Session,
        rule: SuggestRule::Dirs(dirs.iter().map(|d| d.to_string()).collect()),
        raw: Value::Null,
    }
}

/// ★ 「以后不再问」那一段只在核心写（`AskSuggestion::said`）：带上记在哪一层与放宽的东西。
#[test]
fn a_suggestion_is_said_by_the_core_from_its_codes() {
    let s = allow_dirs(&["/a", "/b"]);
    assert_eq!(
        s.said(),
        copy_text(
            "beAsk.rule.dirs",
            &[
                ("dirs", "/a, /b"),
                ("scope", &copy_text("beAsk.scope.session", &[]))
            ]
        )
    );
    let m = AskSuggestion {
        scope: SuggestScope::User,
        rule: SuggestRule::Allow(vec!["Bash(ls)".into()]),
        raw: Value::Null,
    };
    assert_eq!(
        m.said(),
        copy_text(
            "beAsk.rule.rules",
            &[
                ("rules", "Bash(ls)"),
                ("scope", &copy_text("beAsk.scope.user", &[]))
            ]
        )
    );
}

/// ★ 挡着终端、那一家认得出终端里那个框（app 里经读屏答）⇒ 只给「允许 · 拒绝」两项：附话 · 以后不再问 · 停下这一轮终端里按不出来；
/// 允许是主按钮；界面答得了 ⇒ 不写「在终端里答」。认不得（`on_screen` 假）⇒ 照旧一项都不给。
#[test]
fn a_terminal_ask_answered_on_screen_offers_allow_and_deny_only() {
    let mut a = ask_in(AskKind::Approve);
    a.blocks_terminal = true;
    a.suggestions = vec![AskSuggestion {
        scope: SuggestScope::Session,
        rule: SuggestRule::AcceptEdits,
        raw: Value::Null,
    }];
    let p = product_of("a", &a, STOPS, true, None);
    assert_eq!(ids(&p), ["allow", "deny"]);
    assert_eq!(p.choices[0].look, ChoiceLook::Main);
    assert!(p.choices.iter().all(|c| c.note == NoteNeed::None));
    assert_eq!(
        Asked {
            ask: p,
            since_ms: 0
        }
        .manual(),
        None
    );
    assert!(product_of("a", &a, STOPS, false, None).choices.is_empty());
}
