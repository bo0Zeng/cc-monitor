//! 起会话面新形：权限档「档名 ⇒ 一组参数」· 换档两形 · 起会话选模型的参数 · 模型清单怎么读 · 换模型那一句带推理档。
//! 一律拿替身家（`fake::LAUNCH`，每一格与两家都不同形）喂，期望逐字手写。

use super::*;
use crate::agents::fake;

fn perm() -> PermissionFace {
    fake::LAUNCH.permission.expect("替身家声明了权限档")
}

fn owned(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// ★ 表上可选的档 ⇒ 恰那一对参数；只认不选的 · 不认得的 ⇒ 一个参数都不出（只认不选的照样有字）。
#[test]
fn a_mode_on_the_table_gives_exactly_its_pair_and_no_other_mode_gives_any() {
    let p = perm();
    assert_eq!(p.args_of("ask"), Some(owned(&["--gate", "ask"])));
    assert_eq!(p.args_of("roam"), Some(owned(&["--gate", "roam"])));
    assert_eq!(p.args_of("wild"), None, "只认不选的档不出参数");
    assert_eq!(p.args_of("nope"), None, "不认得的档不出参数");
    assert_eq!(p.args_of(""), None);
    assert!(p.offered("wild").is_none() && p.offered("ask").is_some());
    assert_eq!(p.text_of("wild").as_deref(), Some("Wild"));
    assert_eq!(p.text_of("roam").as_deref(), Some("Roam"));
    assert_eq!(p.text_of("nope"), None);
}

/// ★ 投命令那一形：可选的档 ⇒ 恰那一句；只认不选的 · 不认得的 ⇒ 不出那一句。按键那一形不出句子。
#[test]
fn the_command_form_gives_exactly_the_line_and_nothing_for_other_modes() {
    let p = perm();
    assert!(matches!(p.switch, Some(ModeSwitch::Command(_))));
    assert_eq!(p.switch_line("roam").as_deref(), Some(":gate roam"));
    assert_eq!(p.switch_line("ask").as_deref(), Some(":gate ask"));
    assert_eq!(p.switch_line("wild"), None);
    assert_eq!(p.switch_line("nope"), None);
    let cycled = PermissionFace {
        switch: Some(ModeSwitch::Cycle(ModeCycle {
            key: "f9",
            cycle_max: 2,
            read: |_| None,
        })),
        ..p
    };
    assert_eq!(cycled.switch_line("roam"), None, "按键那一形没有句子");
    let fixed = PermissionFace { switch: None, ..p };
    assert_eq!(fixed.switch_line("roam"), None);
}

/// ★ 起会话选了模型 ⇒ 那一家给的那几个参数；换模型那一句带上推理档，不带也成。
#[test]
fn model_args_and_the_switch_line_carry_the_model_and_the_effort() {
    let f = fake::LAUNCH;
    let args = f.model_args.expect("替身家起会话选得了模型");
    assert_eq!(args("f-big", None), owned(&["--brain", "f-big"]));
    assert_eq!(
        args("f-big", Some("high")),
        owned(&["--brain", "f-big", "--think", "high"])
    );
    let sw = f.model_switch.expect("替身家换得了模型");
    assert_eq!((sw.command)("f-big", Some("high")), ":brain f-big @high");
    assert_eq!((sw.command)("f-big", None), ":brain f-big");
}

/// ★ 模型清单：替身家那一形的输出读回列的那几个（那一家排的先后）、各带推理档与缺省档；读不成 ⇒ `None`。
#[test]
fn the_model_list_reads_back_the_listed_models_with_their_efforts() {
    let l = fake::LAUNCH.model_list.expect("替身家声明了模型清单");
    assert_eq!(l.args, ["brains", "--offline"]);
    let got = (l.read)("f-small|Small|low,high|low\nf-big|Big Brain||\n").expect("读得成");
    assert_eq!(
        got,
        vec![
            ListedModel {
                value: "f-small".into(),
                text: "Small".into(),
                efforts: owned(&["low", "high"]),
                default_effort: Some("low".into()),
            },
            ListedModel {
                value: "f-big".into(),
                text: "Big Brain".into(),
                efforts: vec![],
                default_effort: None,
            },
        ]
    );
    for bad in ["", "not a list", "|x||"] {
        assert_eq!((l.read)(bad), None, "{bad:?}");
    }
}

/// ★ 一个模型值写成字只这一处：清单里有 ⇒ 清单的字；清单里没有 ⇒ 静态那一张的字；「默认」那一项另写；都没有 ⇒ `None`。
#[test]
fn a_model_value_has_one_word_from_the_list_then_the_table_then_the_default() {
    let listed = [ListedModel {
        value: "f-listed".into(),
        text: "Listed Only".into(),
        efforts: vec![],
        default_effort: None,
    }];
    let f = LaunchFace {
        model_default: Some(Choice {
            value: "f-default",
            text: || "Default".to_string(),
        }),
        ..fake::LAUNCH
    };
    assert_eq!(
        f.model_word("f-listed", Some(&listed)).as_deref(),
        Some("Listed Only"),
        "清单里才有的模型写得出字"
    );
    assert_eq!(
        f.model_word("f-static", Some(&listed)).as_deref(),
        Some("Static")
    );
    assert_eq!(f.model_word("f-static", None).as_deref(), Some("Static"));
    assert_eq!(f.model_word("f-default", None).as_deref(), Some("Default"));
    assert_eq!(
        f.model_word("f-listed", None),
        None,
        "清单读不成就不认清单里的"
    );
    assert_eq!(f.model_word("nope", Some(&listed)), None);
}

/// 静态那一张里的一项 ⇒ 清单形（不分推理档），字照那一项写的。
#[test]
fn a_static_choice_becomes_a_listed_model_without_efforts() {
    let c = fake::LAUNCH.models.expect("替身家有静态的模型表")[0];
    assert_eq!(
        ListedModel::of_choice(&c),
        ListedModel {
            value: "f-static".into(),
            text: "Static".into(),
            efforts: vec![],
            default_effort: None,
        }
    );
}

/// ★ 一件活的结果文本照那一家说的地方读：结果文件那一形只认文件（标准输出不看）；标准输出那一形照它的读法（文件不看）；读不到 ⇒ `None`。
#[test]
fn the_task_result_text_is_read_from_where_the_family_says() {
    let file = TaskText::File;
    assert_eq!(
        file.read(Some("stdout noise"), Some("from the file".into()))
            .as_deref(),
        Some("from the file")
    );
    assert_eq!(
        file.read(None, Some("from the file".into())).as_deref(),
        Some("from the file"),
        "标准输出超了上限不碍结果文件那一形"
    );
    assert_eq!(
        file.read(Some("stdout noise"), None),
        None,
        "结果文件没写出 ⇒ 没有"
    );
    let out = TaskText::Stdout(|s| s.strip_prefix("R:").map(str::to_string));
    assert_eq!(
        out.read(Some("R:said"), Some("a file".into())).as_deref(),
        Some("said")
    );
    assert_eq!(
        out.read(Some("unreadable"), Some("a file".into())),
        None,
        "读不成 ⇒ 没有，不拿文件顶"
    );
    assert_eq!(
        out.read(None, Some("a file".into())),
        None,
        "超了上限 ⇒ 没有，不拿文件顶"
    );
}

/// ★ 没声明一次性那一形的家（连认不出的家）⇒ 不能派活；声明了 ⇒ 交回它那一份。
#[test]
fn only_a_family_that_declares_the_task_form_can_be_given_work() {
    let declared = crate::agents::Adapter {
        kind: "fake-task",
        launch: Some(fake::LAUNCH),
        ..fake::bare_row(fake::home)
    };
    let undeclared = crate::agents::Adapter {
        kind: "fake-notask",
        launch: Some(LaunchFace {
            task: None,
            ..fake::LAUNCH
        }),
        ..fake::bare_row(fake::home)
    };
    let reg = [declared, undeclared];
    let got = task_face_among(&reg, "fake-task").expect("声明了就有");
    assert_eq!(
        (got.args)("say", "/out"),
        (fake::LAUNCH.task.expect("替身家声明了").args)("say", "/out")
    );
    assert!(task_face_among(&reg, "fake-notask").is_none());
    assert!(task_face_among(&reg, "nobody").is_none());
}

/// ★ Claude 一次性那一形：`claude -p --output-format json -- <说明>`（说明照原样作位置参数、`--` 之后，不被认成旗标）；
/// 结果在标准输出那一份 JSON 的 `result` 里；不是那一形 ⇒ 读不成。结果文件它不写、不交。
#[test]
fn claude_runs_one_task_in_print_mode_and_reads_result_from_its_json() {
    let t = crate::agents::claudecode::LAUNCH
        .task
        .expect("Claude 声明了一次性那一形");
    assert_eq!(
        (t.args)("-fix it", "/tmp/out.txt"),
        owned(&["-p", "--output-format", "json", "--", "-fix it"])
    );
    let TaskText::Stdout(read) = t.text else {
        panic!("Claude 的结果在标准输出");
    };
    assert_eq!(
        read(r#"{"type":"result","subtype":"success","is_error":false,"result":"done.","session_id":"s"}"#)
            .as_deref(),
        Some("done.")
    );
    for bad in ["", "done.", "{}", r#"{"result":3}"#] {
        assert_eq!(read(bad), None, "{bad:?}");
    }
}

/// 说话一方的短名只经 `speaker_said_among`：有起会话面 ⇒ 它的 `speaker_name`；没有起会话面 ⇒ 那一家对人的叫法；
/// 叫法没声明 · 注册表里认不出 ⇒ 照实那一句（同 `family_said_among`），不露 kind（替身家）。
#[test]
fn the_speaker_falls_back_to_the_familys_name_not_its_kind() {
    let reg = [
        fake::family_row("fake", "CCM_FAKE_A", "fakea", true, fake::home),
        fake::unlaunched_row("stand-said", Some("替身丙")),
        fake::unlaunched_row("stand-unsaid", None),
    ];
    assert_eq!(speaker_said_among(&reg, "fake"), fake::LAUNCH.speaker_name);
    assert_ne!(
        fake::LAUNCH.speaker_name,
        fake::said_of("fake"),
        "夹具前提：短名与家名不同"
    );
    for k in ["stand-said", "stand-unsaid", "nope"] {
        assert_eq!(
            speaker_said_among(&reg, k),
            family_said_among(&reg, k),
            "{k}"
        );
    }
}
