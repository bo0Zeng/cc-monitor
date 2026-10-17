//! 隐藏的判定：五档先有先算（每一档一格，上一档压住下一档）· 三处带的那一格出自同一个函数。

use super::*;
use crate::history::history_annotations::Table;
use copy_core::copy_text;

fn launch(tab: Tab, alias: Option<&str>) -> LaunchTab {
    LaunchTab {
        tab,
        alias: alias.map(str::to_string),
    }
}

/// 下面四档都会让它隐藏的那一组事实（分身 · 有主会话 · 偏好两格都是隐藏 · 起会话时写了隐藏）。
fn all_hide(l: &LaunchTab) -> Facts<'_> {
    Facts {
        pin: None,
        launch: Some(l),
        bg: true,
        parent: Some("orders"),
        rules: Rules {
            child_tab: Tab::Hide,
            bg_tab: Tab::Hide,
        },
    }
}

fn by(f: &Facts) -> Option<HiddenBy> {
    judge(f).map(|h| h.by)
}

#[test]
fn tier_1_the_single_pin_beats_every_rule() {
    let hide = launch(Tab::Hide, Some("cct"));
    let show = launch(Tab::Show, None);
    // 点了「隐藏」：别的都说显示也隐藏。
    let on = Facts {
        pin: Some(Hide::On),
        launch: Some(&show),
        ..Facts::default()
    };
    assert_eq!(
        judge(&on),
        Some(Hidden {
            by: HiddenBy::Own,
            said: copy_text("beSession.hidden.self", &[])
        })
    );
    // 点了「取消隐藏」：下面四档都说隐藏也不隐藏。
    let off = Facts {
        pin: Some(Hide::Off),
        ..all_hide(&hide)
    };
    assert_eq!(judge(&off), None);
}

#[test]
fn tier_2_the_launch_option_beats_the_machine_rules() {
    // 别名写了隐藏 ⇒ 说是哪个别名（分身 · 有主会话都在也是它）。
    let alias = launch(Tab::Hide, Some("cct"));
    let h = judge(&all_hide(&alias)).expect("隐藏");
    assert_eq!(h.by, HiddenBy::Alias);
    assert_eq!(
        h.said,
        copy_text("beSession.hidden.alias", &[("name", "cct")])
    );
    // 命令行当场给的（没经别名）⇒ 另一句，不说成别名。
    let bare = launch(Tab::Hide, None);
    assert_eq!(by(&all_hide(&bare)), Some(HiddenBy::Launch));
    // 写了显示 ⇒ 分身 · 子会话的偏好都说隐藏也显示。
    let show = launch(Tab::Show, Some("cc"));
    assert_eq!(judge(&all_hide(&show)), None);
}

#[test]
fn tier_3_a_background_session_follows_the_bg_rule_not_the_child_rule() {
    let f = |bg_tab: Tab, child_tab: Tab| Facts {
        bg: true,
        parent: Some("orders"),
        rules: Rules { child_tab, bg_tab },
        ..Facts::default()
    };
    assert_eq!(by(&f(Tab::Hide, Tab::Show)), Some(HiddenBy::Bg));
    // 分身那一格说显示 ⇒ 显示（有主会话、子会话说隐藏也不看）。
    assert_eq!(judge(&f(Tab::Show, Tab::Hide)), None);
    assert_eq!(
        judge(&f(Tab::Hide, Tab::Hide)).map(|h| h.said),
        Some(copy_text("beSession.hidden.bg", &[]))
    );
}

#[test]
fn tier_4_a_child_follows_the_child_rule_and_names_its_parent() {
    let f = |child_tab: Tab| Facts {
        parent: Some("orders"),
        rules: Rules {
            child_tab,
            bg_tab: Tab::Hide,
        },
        ..Facts::default()
    };
    let h = judge(&f(Tab::Hide)).expect("隐藏");
    assert_eq!(h.by, HiddenBy::Child);
    assert_eq!(
        h.said,
        copy_text("beSession.hidden.child", &[("parent", "orders")])
    );
    assert_eq!(judge(&f(Tab::Show)), None);
}

#[test]
fn tier_5_nothing_said_means_shown_and_the_defaults_hide_children_only() {
    let rules = Rules::default();
    assert_eq!((rules.child_tab, rules.bg_tab), (Tab::Hide, Tab::Show));
    let plain = Facts {
        rules,
        ..Facts::default()
    };
    assert_eq!(judge(&plain), None);
    let bg = Facts {
        bg: true,
        rules,
        ..Facts::default()
    };
    assert_eq!(judge(&bg), None, "缺省分身会话显示");
    let child = Facts {
        parent: Some("p"),
        rules,
        ..Facts::default()
    };
    assert_eq!(by(&child), Some(HiddenBy::Child), "缺省子会话隐藏");
}

#[test]
fn here_reads_pin_ledger_and_lineage_for_one_session() {
    let mut annotations = Table::new();
    annotations.insert(
        "s-pin".into(),
        crate::history::history_annotations::Entry {
            hide: Some(Hide::On),
            ..Default::default()
        },
    );
    let mut here = Here {
        rules: Rules::default(),
        ..Here::default()
    };
    here.ledger
        .tabs
        .insert("s-alias".into(), launch(Tab::Hide, Some("cct")));
    here.lineage.parents.insert(
        "s-kid".into(),
        crate::lineage::Parent {
            parent: "s-main".into(),
            agent: "claude".into(),
            at: 1,
        },
    );
    let title = |sid: &str| format!("<{sid}>");
    let hidden_of = |here: &Here,
                     t: &crate::history::history_annotations::Table,
                     sid: &str,
                     bg,
                     title: &dyn Fn(&str) -> String| {
        crate::history::session_hidden::hidden_here(here, t, sid, bg, title)
    };
    assert_eq!(
        hidden_of(&here, &annotations, "s-pin", false, &title).map(|h| h.by),
        Some(HiddenBy::Own)
    );
    assert_eq!(
        hidden_of(&here, &annotations, "s-alias", false, &title).map(|h| h.by),
        Some(HiddenBy::Alias)
    );
    assert_eq!(
        hidden_of(&here, &annotations, "s-kid", false, &title).map(|h| h.said),
        Some(copy_text(
            "beSession.hidden.child",
            &[("parent", "<s-main>")]
        ))
    );
    assert_eq!(
        hidden_of(&here, &annotations, "s-main", false, &title),
        None
    );
}

/// 三处（会话宣告那一帧 · `history-list` 每一行 · `sessions-needs` 每一行）带的那一格出自同一个函数：
/// 生产代码里拼 `Hidden { … }` 的只有本文件；那三处的源码各自经本模块判（[`judge`] 或 [`Here::hidden`]）。
#[test]
fn the_three_carriers_get_the_cell_from_this_one_function() {
    let root = guard_core::addr::backend_src_root();
    let here = root.join("control/session_hidden.rs");
    let mut builders = Vec::new();
    for (path, text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let code = guard_core::addr::production_side_of(&path, &text);
        if path != here && code.contains("Hidden {") {
            builders.push(path.display().to_string());
        }
    }
    assert!(
        builders.is_empty(),
        "这几处自己拼了隐藏那一格（判定只住 control/session_hidden.rs）：{builders:?}"
    );
    for (file, why) in [
        ("observe/watcher.rs", "会话宣告那一帧"),
        ("history/history_list.rs", "history-list 每一行"),
        ("faces/history_face.rs", "sessions-needs 每一行"),
    ] {
        let text = std::fs::read_to_string(root.join(file)).expect("读得出");
        let code = guard_core::production_code(&text);
        assert!(
            code.contains("session_hidden::"),
            "{why}（{file}）没经 control/session_hidden.rs 判隐藏"
        );
    }
}
