//! `win-manifest` 的判据。要求：「两个前端包每一个链接产物都带『通用控件 v6』的进程清单；清单与编法只一处」。
//! 读回真程序那一条住各包自己的测试程序里（Windows 上）；这里判各平台都编的那几件。

use super::*;

/// 清单本身点名通用控件 v6，判法认它；不点名的判法不认。
#[test]
fn the_one_manifest_names_common_controls_v6() {
    let text = std::fs::read_to_string(manifest_path()).expect("读本 crate 的 app.manifest");
    assert!(names_common_controls_v6(&text));
    assert!(!names_common_controls_v6("<assembly/>"));
}

/// `.rc` 那一行：资源号 1、类型 24，Windows 路径里的反斜杠成对。
#[test]
fn the_rc_line_doubles_backslashes() {
    assert_eq!(
        rc_line(r"C:\a\app.manifest"),
        "1 24 \"C:\\\\a\\\\app.manifest\"\n"
    );
}

/// 两个前端包的构建脚本都调这一处编法，自己不另写一份（没有第二份 `.rc`、不直接调 embed-resource）。
#[test]
fn both_frontend_build_scripts_call_this_one_and_write_no_second_copy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for pkg in ["src/frontend/shell", "src/frontend/filewin"] {
        let b = std::fs::read_to_string(root.join(pkg).join("build.rs"))
            .unwrap_or_else(|e| panic!("{pkg}/build.rs：{e}"));
        assert!(
            b.contains("win_manifest::embed_for_every_artifact()"),
            "{pkg}/build.rs 没调 `win_manifest::embed_for_every_artifact()` —— 那个包在 Windows 上的测试程序会 0xc0000139 起不来"
        );
        assert!(
            !b.contains("embed_resource::") && !b.contains(".rc\""),
            "{pkg}/build.rs 自己又编了一份清单资源"
        );
    }
}
