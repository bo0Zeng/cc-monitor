//! 窗口的样子来自开窗种子里那一套主题：名单两边对上、缺省值解得出来、映射真的落到 egui 的样子上、窗口里没有自己的色值。

use super::*;
use crate::theme::testing::{default_theme, default_tokens};
use filewin_contract::{Theme, THEME_TOKENS};

/// 主界面那一侧读的名单（`file-window.ts::FILE_WINDOW_THEME_TOKENS`）与契约那一份两向相等。
#[test]
fn the_token_list_is_the_same_on_both_sides() {
    let ts = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/file-window.ts"))
        .unwrap();
    let start = ts
        .find("FILE_WINDOW_THEME_TOKENS = [")
        .expect("file-window.ts 里那张名单不见了");
    let body = &ts[start..start + ts[start..].find("] as const").expect("名单没收尾")];
    let theirs: Vec<&str> = body.split('"').filter(|s| s.starts_with("--")).collect();
    let mut a = theirs.clone();
    a.sort_unstable();
    a.dedup();
    assert_eq!(a.len(), theirs.len(), "主界面那份名单里有重复：{theirs:?}");
    let mut b: Vec<&str> = THEME_TOKENS.to_vec();
    b.sort_unstable();
    assert_eq!(a, b, "两边的令牌名单不等");
}

/// 缺省主题解得出来；缺一格 · 多一格 · 一格写成认不得的样子都拒（三句各不相同，读不懂那句带原值），不补默认。
#[test]
fn the_default_tokens_parse_and_bad_ones_are_refused() {
    let t = default_theme();
    assert_eq!(t.bg, [0x2b, 0x2a, 0x27, 0xff]);
    assert_eq!(t.border_medium, [0xff, 0xff, 0xff, 0x20]);
    assert_eq!(t.font_base[0], "Inter");
    assert_eq!(t.size_base, 14.0);

    let mut missing = default_tokens();
    missing.remove("--accent");
    let e_missing = Theme::from_tokens(&missing).unwrap_err();

    let mut extra = default_tokens();
    extra.insert("--nope".into(), "#000".into());
    let e_extra = Theme::from_tokens(&extra).unwrap_err();

    let mut bad = default_tokens();
    bad.insert("--bg".into(), "color-mix(in srgb, red 10%, blue)".into());
    let e_bad = Theme::from_tokens(&bad).unwrap_err();
    assert!(
        e_bad.contains("color-mix(in srgb, red 10%, blue)"),
        "{e_bad}"
    );
    // 三形三句话，各不相同（不是同一句兜底）。
    assert!(e_missing != e_extra && e_extra != e_bad && e_missing != e_bad);
}

/// 用户改过的那一格真的进了 egui 的样子（换一个强调色 / 主底 ⇒ 选中底与面板底跟着变；字号跟着主界面）。
#[test]
fn the_theme_lands_on_the_egui_style() {
    let mut toks = default_tokens();
    toks.insert("--accent".into(), "#102030".into());
    toks.insert("--bg".into(), "rgb(1, 2, 3)".into());
    toks.insert("--font-size-base".into(), "17px".into());
    let t = Theme::from_tokens(&toks).unwrap();
    let ctx = egui::Context::default();
    install(&ctx, &t);
    let s = ctx.global_style();
    assert_eq!(s.visuals.panel_fill, egui::Color32::from_rgb(1, 2, 3));
    let p = palette(&ctx);
    assert_eq!(p.accent, egui::Color32::from_rgb(0x10, 0x20, 0x30));
    assert_eq!(s.visuals.selection.bg_fill, p.picked);
    assert_eq!(
        s.text_styles[&egui::TextStyle::Body].size,
        17.0,
        "正文字号没跟主界面"
    );
    assert_eq!(
        s.visuals.window_fill,
        egui::Color32::from_rgb(0x39, 0x39, 0x37)
    );
}

/// 窗口里没有自己的色值：颜色只从主题来（`theme.rs` 是把主题的数换成 egui 颜色的唯一一处）。
#[test]
fn the_window_paints_no_colour_of_its_own() {
    const NEEDLES: [&str; 8] = [
        "Color32::from_rgb(",
        "Color32::from_rgba_unmultiplied(",
        "Color32::from_rgba_premultiplied(",
        "Color32::from_gray(",
        "Color32::from_hex(",
        "Color32::RED",
        "Color32::WHITE",
        "Color32::BLACK",
    ];
    const OUTSIDE: [&str; 1] = ["theme.rs"];
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut hits = Vec::new();
    for (p, src) in guard_core::scan_tree_excluding(std::path::Path::new(dir), &["rs"], &OUTSIDE) {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let code = guard_core::production_code(&src);
        for n in NEEDLES {
            if code.contains(n) {
                hits.push(format!("{name}: {n}"));
            }
        }
    }
    assert_eq!(hits, Vec::<String>::new(), "窗口里还有自己写的色值");
    // 正控：尺子认得出 theme.rs 里那一处换算。
    let own = std::fs::read_to_string(format!("{dir}/theme.rs")).unwrap();
    assert!(guard_core::production_code(&own).contains("Color32::from_rgba_unmultiplied("));
}
