//! 起会话照那一家的表出参数：权限档「档名 ⇒ 一组参数」· 模型 `model_args`。不认哪一家 —— 一律拿替身家（`fake::LAUNCH`）喂，期望逐字手写。

use super::*;
use crate::agents::fake;

fn owned(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn batch(face: Option<crate::agents::LaunchFace>) -> Batch {
    Batch {
        agent: fake::AGENT_KIND.to_string(),
        launcher: String::new(),
        default_launcher: fake::DEFAULT_COMMAND.to_string(),
        models: Default::default(),
        face,
    }
}

fn item(mode: Option<&str>, model: Option<&str>, say: Option<&str>) -> Item {
    Item {
        sid: "00000000-0000-4000-8000-0000000000f6".into(),
        cwd: "/w".into(),
        account: AccountAsk::Follow,
        say: say.map(str::to_string),
        model: model.map(str::to_string),
        permission_mode: mode.map(str::to_string),
    }
}

/// ★ 替身家的档 ⇒ 交给那一家的那一串恰带那一对；选了模型 ⇒ 接着带 `model_args` 给的那几个；什么都没选 ⇒ 一个不带。
#[test]
fn a_mode_and_a_model_give_exactly_the_familys_args() {
    let f = fake::LAUNCH;
    assert_eq!(
        launch_args(Some(&f), Some("roam"), Some("f-big"), None),
        owned(&["--gate", "roam", "--brain", "f-big"])
    );
    assert_eq!(
        launch_args(Some(&f), Some("ask"), None, None),
        owned(&["--gate", "ask"])
    );
    assert_eq!(
        launch_args(Some(&f), None, Some("f-big"), None),
        owned(&["--brain", "f-big"])
    );
    assert!(launch_args(Some(&f), None, None, None).is_empty());
}

/// ★ 选了模型又选了推理档 ⇒ 推理档的参数照那一家的 `model_args` 出、恰一次；不带推理档 ⇒ 一个不出；
/// 没选模型 · 推理档不是档名 ⇒ 推理档不出（模型照出）。
#[test]
fn an_effort_rides_the_familys_model_args_exactly_once() {
    let f = fake::LAUNCH;
    let got = launch_args(Some(&f), Some("roam"), Some("f-big"), Some("high"));
    assert_eq!(
        got,
        owned(&["--gate", "roam", "--brain", "f-big", "--think", "high"])
    );
    assert_eq!(got.iter().filter(|a| *a == "--think").count(), 1);
    assert!(!launch_args(Some(&f), None, Some("f-big"), None)
        .iter()
        .any(|a| a == "--think"));
    assert!(launch_args(Some(&f), None, None, Some("high")).is_empty());
    assert_eq!(
        launch_args(Some(&f), None, Some("f-big"), Some("hi gh;")),
        owned(&["--brain", "f-big"])
    );
}

/// ★ 恢复那一形照记录起：不认得的档 · 只认不选的档 · 不是模型名 · 那一家起会话选不了模型 ⇒ 那一样一个参数不带。
#[test]
fn what_the_family_does_not_take_is_left_out_on_resume() {
    let f = fake::LAUNCH;
    assert!(launch_args(Some(&f), Some("nope"), None, None).is_empty());
    assert!(launch_args(Some(&f), Some("wild"), None, None).is_empty());
    assert!(launch_args(Some(&f), None, Some("bad model;"), None).is_empty());
    let bare = crate::agents::LaunchFace {
        permission: None,
        model_args: None,
        ..f
    };
    assert!(launch_args(Some(&bare), Some("roam"), Some("f-big"), Some("high")).is_empty());
    assert!(launch_args(None, Some("roam"), Some("f-big"), Some("high")).is_empty());
}

/// ★ 起新会话那一形：给选的档 ⇒ 收；只认不选的 · 不认得的 ⇒ 拒（说是不给选）；那一家没有权限档 ⇒ 拒（说没有）。
#[test]
fn a_new_session_refuses_a_mode_the_family_does_not_offer() {
    let f = fake::LAUNCH;
    assert!(new_mode_ok(&f, Some("roam")).is_ok());
    assert!(new_mode_ok(&f, None).is_ok());
    for m in ["wild", "nope"] {
        let e = new_mode_ok(&f, Some(m)).unwrap_err();
        assert_eq!(e.code.as_deref(), Some("bad_args"), "{m}");
        assert!(e.said.contains("permissionMode"), "{m}: {}", e.said);
    }
    let bare = crate::agents::LaunchFace {
        permission: None,
        ..f
    };
    let e = new_mode_ok(&bare, Some("roam")).unwrap_err();
    assert!(e.said.contains("no permission modes"), "{}", e.said);
}

/// ★ 恢复那一项的那一半末尾：档 · 模型的参数在前，`--` ＋ 那一句在后；交给渲染那一步的 `model` 不带（不出第二遍）。
#[test]
fn the_resume_tail_carries_the_args_then_the_first_say() {
    let b = batch(Some(fake::LAUNCH));
    let it = item(Some("ask"), Some("f-small"), Some("接着做"));
    assert_eq!(
        tail_args(&it, &b),
        owned(&["--gate", "ask", "--brain", "f-small", "--", "接着做"])
    );
    let l = resume_launch(&it, &b, None);
    assert_eq!(l.model, None);
    assert_eq!(l.preset_args, tail_args(&it, &b));
}

/// 判好的号拆开：交给渲染那一步的不带偏好模型，偏好模型交回来照那一家的表出；不是号库里的号 ⇒ 原样、没有偏好模型。
#[test]
fn the_accounts_model_is_taken_apart_for_the_family_table() {
    let a = Settled::Account(LaunchedAccount {
        name: "w".into(),
        config_dir: "/h/.w".into(),
        model: Some("f-big".into()),
    });
    let (rendered, picked) = model_apart(&a);
    assert_eq!(picked.as_deref(), Some("f-big"));
    assert_eq!(
        rendered,
        Settled::Account(LaunchedAccount {
            name: "w".into(),
            config_dir: "/h/.w".into(),
            model: None,
        })
    );
    assert_eq!(model_apart(&Settled::Base), (Settled::Base, None));
    assert_eq!(model_apart(&Settled::Unsaid), (Settled::Unsaid, None));
}
