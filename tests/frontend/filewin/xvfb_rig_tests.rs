//! 两处起 Xvfb 用同一个起法（读仓内文本，不起服务器）：文件窗口台架与截图工具都经 `tests/scripts/xvfb-free.sh`。

/// 两处起 Xvfb 用同一个起法：台架与截图工具都经 `tests/scripts/xvfb-free.sh`（它挑号、按锁占住，Xvfb 自己报号）；
/// 两边都不自己 spawn `Xvfb`、不自己写号段。
#[test]
#[cfg(not(windows))]
fn the_rig_and_the_shots_tool_start_xvfb_the_same_way() {
    let script = include_str!("../../scripts/xvfb-free.sh");
    assert!(
        script.contains("-displayfd 1 ") && script.contains("noclobber"),
        "那份脚本不再按锁占号、或不再让 Xvfb 自己报号：{script}"
    );
    let rig = include_str!("xvfb_rig.rs");
    let shots = include_str!("../../shots/filewin.mjs");
    assert!(
        rig.contains("/../../../tests/scripts/xvfb-free.sh"),
        "台架没经那份脚本起"
    );
    assert!(
        shots.contains("\"tests/scripts/xvfb-free.sh\""),
        "截图工具没经那份脚本起"
    );
    for (who, text) in [("台架", rig), ("截图工具", shots)] {
        assert!(
            !text.contains("Command::new(\"Xvfb\")") && !text.contains("spawn(\"Xvfb\""),
            "{who}自己起了 Xvfb（号该由那份脚本挑、占）"
        );
    }
}
