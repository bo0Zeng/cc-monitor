//! 图片扩展名一张通用表 ＋ 一家额外申报：通用层判一份附件是不是图片只问 `is_image`，拼一句照它挑那一家的 `image` · `file` 那一形。
//! 两行替身（`fake::attach_rows`）：一行多申报一种通用表里没有的扩展名，一行不申报；期望逐字手写。

use super::*;
use crate::agents::fake;

fn faces() -> (AttachRefer, AttachRefer) {
    let [extra, plain] = fake::attach_rows();
    assert_eq!(
        (extra.kind, plain.kind),
        (fake::AGENT_KIND, fake::PLAIN_KIND)
    );
    (
        extra.attach.expect("申报的那一行有附件写法"),
        plain.attach.expect("不申报的那一行有附件写法"),
    )
}

/// ★ 一家多申报的那一种只对它算图片；不申报的那一行只认通用表。
#[test]
fn an_extension_one_family_adds_is_an_image_for_it_alone() {
    let (extra, plain) = faces();
    assert!(
        !PASTED_IMAGES.contains(&"bmp"),
        "夹具申报的那一种要在通用表外"
    );
    assert!(is_image(&extra, "shot.bmp"));
    assert!(is_image(&extra, "SHOT.BMP"), "扩展名不分大小写");
    assert!(!is_image(&plain, "shot.bmp"));
}

/// ★ 通用表里的每一种两行都认（大小写都算）；没扩展名 · 只有点开头的 · 别的扩展名都不算。
#[test]
fn every_extension_in_the_common_table_is_an_image_for_both() {
    let (extra, plain) = faces();
    for face in [&extra, &plain] {
        for e in PASTED_IMAGES {
            assert!(is_image(face, &format!("a.{e}")), "{e}");
            assert!(is_image(face, &format!("a.{}", e.to_uppercase())), "{e}");
        }
        for name in ["png", ".png", "a.txt", "a.png.txt", "a."] {
            assert!(!is_image(face, name), "{name}");
        }
    }
}

/// ★ 拼一句时照通用层的判法挑形：图片走那一家的 `image`（单独粘），别的走 `file`（正文后面一行）；
/// 同一份 `.bmp` 对申报的那一行是图、对不申报的那一行是文件。
#[test]
fn typing_a_say_picks_the_image_or_file_form_by_the_common_judge() {
    let rows = fake::attach_rows();
    let refs = [
        AttachRef {
            name: "a.bmp",
            abs: "/w/.ccm-attach/1/a.bmp",
            rel: ".ccm-attach/1/a.bmp",
        },
        AttachRef {
            name: "b.PNG",
            abs: "/w/.ccm-attach/2/b.PNG",
            rel: ".ccm-attach/2/b.PNG",
        },
    ];
    let got = typed_among(&rows, fake::AGENT_KIND, "看图", &refs);
    assert_eq!(
        got.pastes,
        ["/w/.ccm-attach/1/a.bmp", "/w/.ccm-attach/2/b.PNG"]
    );
    assert_eq!(got.text, "看图");
    let got = typed_among(&rows, fake::PLAIN_KIND, "看图", &refs);
    assert_eq!(got.pastes, ["/w/.ccm-attach/2/b.PNG"]);
    assert_eq!(got.text, "看图\n<.ccm-attach/1/a.bmp>");
}
