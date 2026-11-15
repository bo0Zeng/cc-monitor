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
        hide: None,
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
fn tier_1_the_single_hide_beats_every_rule() {
    let hide = launch(Tab::Hide, Some("cct"));
    let show = launch(Tab::Show, None);
    // 点了「隐藏」：别的都说显示也隐藏。
    let on = Facts {
        hide: Some(Hide::On),
        launch: Some(&show),
        ..Facts::default()
    };
    assert_eq!(
        judge(&on),
        Some(Hidden {
            by: HiddenBy::Own,
            said: copy_text("history.row.hidden", &[])
        })
    );
    // 点了「取消隐藏」：下面四档都说隐藏也不隐藏。
    let off = Facts {
        hide: Some(Hide::Off),
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
fn here_reads_hide_ledger_and_lineage_for_one_session() {
    let mut annotations = Table::new();
    annotations.insert(
        "s-hide".into(),
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
    let hidden_of =
        |here: &Here,
         t: &crate::history::history_annotations::Table,
         sid: &str,
         bg,
         title: &dyn Fn(&str) -> String| { hidden_here(here, t, sid, bg, title) };
    assert_eq!(
        hidden_of(&here, &annotations, "s-hide", false, &title).map(|h| h.by),
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
    let here = root.join("history/session_hidden.rs");
    let mut builders = Vec::new();
    for (path, text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let code = guard_core::addr::production_side_of(&path, &text);
        // 按词界认：`SessionHidden {`（推送那一臂）· `HiddenNow {` 不是拼隐藏那一格。
        let builds = code.match_indices("Hidden {").any(|(at, _)| {
            !code[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        });
        if path != here && builds {
            builders.push(path.display().to_string());
        }
    }
    assert!(
        builders.is_empty(),
        "这几处自己拼了隐藏那一格（判定只住 history/session_hidden.rs）：{builders:?}"
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
            "{why}（{file}）没经 history/session_hidden.rs 判隐藏"
        );
    }
}

/// 叫一条会话什么（这台的）：monitor 里改的名 ＞ agent 起的标题 ＞ 第一句；人在 Claude 里 `/rename` 写的那一条不算。
/// 照路径认得出记录的 `listing::session_title_of` 与按 sid 找到那一家的 [`title_found`] 同一条规则（带上改过的名）。
#[test]
fn the_name_here_takes_the_monitor_rename_and_never_the_agent_rename() {
    let dir = std::env::temp_dir().join(format!("ccm-title-here-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s-1.jsonl");
    std::fs::write(
        &p,
        concat!(
            r#"{"type":"user","uuid":"u1","timestamp":"2026-10-27T08:00:00Z","sessionId":"s-1","message":{"role":"user","content":"第一句"}}"#,
            "\n",
            r#"{"type":"ai-title","aiTitle":"起的","sessionId":"s-1"}"#,
            "\n",
            r#"{"type":"custom-title","customTitle":"Claude 里改的","sessionId":"s-1"}"#,
            "\n",
        ),
    )
    .unwrap();
    let of = crate::history::listing::session_title_of;
    assert_eq!(of(&p, None), "起的");
    assert_eq!(of(&p, Some("修登录".into())), "修登录");
    assert_eq!(of(&p, Some(" ".into())), "起的");
    std::fs::remove_dir_all(&dir).ok();
}

/// 按 sid 找到的那一份（判据里手拼：家 · 根不参与取标题）。
fn found_at(kind: &'static str, file: &std::path::Path) -> crate::agents::SessionAt {
    let dir = file.parent().unwrap().to_path_buf();
    crate::agents::SessionAt {
        kind,
        home: dir.clone(),
        root: dir,
        file: file.to_path_buf(),
    }
}

/// 一句人说的话（住记录树那一家的记录形）。
const SAID: &str = r#"{"type":"user","uuid":"u1","timestamp":"2026-10-09T01:02:00.000Z","message":{"role":"user","content":"整理订单服务"}}
"#;

/// ★ 标题照按 sid 找到的那一家取、不照路径另认：路径认不出时，住记录树的那一家照它的读法扫出第一句。
#[test]
fn the_name_here_reads_the_found_family_when_the_path_is_unrecognized() {
    let dir = test_home::scratch("title-here-tree-unrecognized");
    // 哪一家都不认这个文件名、也不在哪一根下 ⇒ 照路径认不出。
    let src = dir.join("s.record");
    std::fs::write(&src, SAID).unwrap();
    assert_eq!(crate::agents::record_kind_of(&src), None);
    let at = found_at(crate::agents::claudecode::AGENT_KIND, &src);
    assert_eq!(
        title_found(crate::agents::REGISTRY, Some(&at), "s", None),
        "整理订单服务"
    );
}

/// ★ 记录那一家有合成历史面、路径认不出：第一句照那一家的摘录法取（不照路径另认、不落成「未命名」）。
#[test]
fn the_name_here_reads_the_found_synth_family_when_the_path_is_unrecognized() {
    use crate::agents::fake::runs::history;
    let dir = test_home::scratch("title-here-synth-unrecognized");
    // 那一家的根在别处，这一份不在它底下。
    let src = dir.join(history::file_name("s1"));
    std::fs::write(
        &src,
        "{\"chat\":\"s1\",\"dir\":\"/w/proj\",\"said\":\"修一下夹具里的那条测试\"}\n",
    )
    .unwrap();
    fn elsewhere() -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from("/nonexistent/fake-chats-home"))
    }
    let reg = [history::row(elsewhere)];
    assert_eq!(crate::agents::record_kind_among(&reg, &src), None);
    let at = found_at(history::KIND, &src);
    assert_eq!(
        title_found(&reg, Some(&at), "s1", None),
        "修一下夹具里的那条测试"
    );
}

/// ★ 认不出那一家（注册表里没有）⇒ 标题照「说不清」落，不退到注册序头一家去读（这一份照路径认得出也不读）。
#[test]
fn an_unknown_family_names_it_untitled_not_the_first_registered_reading() {
    let dir = test_home::scratch("title-here-unknown-family");
    // 放在住记录树那一家的根下 ⇒ 照路径认得出（前提钉住：认得出也不读）。
    let src = dir.join(".claude/projects/-w-proj").join("s.jsonl");
    std::fs::create_dir_all(src.parent().unwrap()).unwrap();
    std::fs::write(&src, SAID).unwrap();
    assert!(crate::agents::record_kind_of(&src).is_some());
    let at = found_at("no-such-family", &src);
    assert_eq!(
        title_found(crate::agents::REGISTRY, Some(&at), "s", None),
        copy_text("history.row.untitled", &[])
    );
    assert_eq!(
        title_found(
            crate::agents::REGISTRY,
            Some(&at),
            "s",
            Some("改过的名".into())
        ),
        "改过的名",
        "改过的名照样优先"
    );
}

/// 启动台账 · 会话血缘读不懂 ⇒ 照旧当没有，但各出一声、带原因（不悄悄让隐藏的会话又显示出来）。
#[test]
fn an_unreadable_ledger_or_lineage_is_said_not_silently_empty() {
    let d = test_home::scratch("hidden-unreadable");
    std::fs::write(d.join(super::super::launch_book::FILE_NAME), b"{ not json").unwrap();
    std::fs::write(d.join(crate::lineage::FILE_NAME), b"{ not json").unwrap();
    let mut here = None;
    let lines = test_meters::heard(|| here = Some(Here::read_in(Some(&d), Rules::default())));
    let here = here.unwrap();
    assert!(
        here.ledger.tabs.is_empty() && here.lineage.parents.is_empty(),
        "读不懂的当没有"
    );
    let warned = |what: &str| {
        lines
            .iter()
            .any(|l| l.contains("WARN") && l.contains(what) && l.contains("reason"))
    };
    assert!(warned("launch ledger"), "启动台账读不懂没出声：{lines:?}");
    assert!(warned("lineage"), "会话血缘读不懂没出声：{lines:?}");
}
