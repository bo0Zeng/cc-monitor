//! Codex 那一行注册的小事实：家怎么认 · 附件怎么写 · wire 上的 kind · 前台进程名 · 判活法。
//! 家那一条喂合成的环境变量（不读真环境、不碰真家目录）。

use super::*;
use std::ffi::OsString;
use std::path::PathBuf;

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| OsString::from(v))
    }
}

#[test]
fn home_is_codex_home_then_dot_codex_under_home_then_unknown() {
    assert_eq!(
        home_from(&env(&[("CODEX_HOME", "/x/codex"), ("HOME", "/h")])),
        Some(PathBuf::from("/x/codex"))
    );
    // 空的 CODEX_HOME 当没设。
    assert_eq!(
        home_from(&env(&[("CODEX_HOME", ""), ("HOME", "/h")])),
        Some(PathBuf::from("/h").join(".codex"))
    );
    assert_eq!(
        home_from(&env(&[("HOME", "/h")])),
        Some(PathBuf::from("/h").join(".codex"))
    );
    assert_eq!(home_from(&env(&[])), None);
}

fn refer_of(name: &str) -> super::super::Refer {
    let abs = format!("/w/p/{name}");
    let rel = format!("p/{name}");
    // 经通用层那一处拼（`typed_with`：图片判定 · 分派都在那里），一份附件 ⇒ 要么单独粘一次、要么正文后的一行。
    let t = super::super::typed_with(
        AGENT_KIND,
        "",
        &[super::super::AttachRef {
            name,
            abs: &abs,
            rel: &rel,
        }],
    );
    match t.pastes.as_slice() {
        [p] => super::super::Refer::Paste(p.clone()),
        _ => super::super::Refer::Line(t.text),
    }
}

#[test]
fn images_paste_the_absolute_path_and_other_files_are_a_bare_relative_line() {
    assert_eq!(
        refer_of("a.png"),
        super::super::Refer::Paste("/w/p/a.png".into())
    );
    assert_eq!(
        refer_of("B.JPeG"),
        super::super::Refer::Paste("/w/p/B.JPeG".into())
    );
    assert_eq!(
        refer_of("notes.md"),
        super::super::Refer::Line("p/notes.md".into())
    );
    // 只有扩展名的那一份不算图片。
    assert_eq!(refer_of(".png"), super::super::Refer::Line("p/.png".into()));
}

#[test]
fn codex_also_pastes_the_two_extensions_its_decoder_maps_and_not_the_formats_it_leaves_off() {
    assert_eq!(ATTACH.images_extra, Some(&["apng", "jfif"][..]));
    assert_eq!(
        refer_of("x.APNG"),
        super::super::Refer::Paste("/w/p/x.APNG".into())
    );
    assert_eq!(
        refer_of("y.jfif"),
        super::super::Refer::Paste("/w/p/y.jfif".into())
    );
    assert_eq!(
        refer_of("z.bmp"),
        super::super::Refer::Line("p/z.bmp".into())
    );
    assert_eq!(
        refer_of("t.tiff"),
        super::super::Refer::Line("p/t.tiff".into())
    );
}

#[test]
fn the_row_heads_are_the_wire_kind_the_process_name_and_held_open() {
    assert_eq!(AGENT_KIND, "codex");
    assert_eq!(PROCESS_NAMES, &["codex"]);
    assert_eq!(LIVE, crate::agents::LiveBy::HeldOpen);
}

#[test]
fn the_row_carries_the_account_faces() {
    assert_eq!(ADAPTER.account_env, Some("CODEX_HOME"));
    assert!(ADAPTER.accounts.is_some() && ADAPTER.footprint.is_some());
    assert_eq!(
        crate::agents::renew_of(AGENT_KIND),
        crate::agents::Renew::Idle
    );
    assert_eq!(
        crate::agents::account_env_of(AGENT_KIND),
        Some("CODEX_HOME")
    );
}
