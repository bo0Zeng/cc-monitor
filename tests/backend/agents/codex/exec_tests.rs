//! Codex 一次性跑一件活那一形：交给 `codex` 的参数逐字 · 结果从结果文件读。
//! 期望值照 codex-rs rust-v0.167.0 的 `exec` 命令行手写（`--skip-git-repo-check` · `--output-last-message` · 位置参数 `PROMPT`），不起真 codex。

use super::*;

#[test]
fn the_task_args_are_exec_with_the_result_file_and_the_say_after_dashes() {
    assert_eq!(
        task_args("summarize the repo", "/x/out"),
        [
            "exec",
            "--skip-git-repo-check",
            "--output-last-message",
            "/x/out",
            "--",
            "summarize the repo"
        ]
    );
    // 以 `-` 开头的说明照样落在 `--` 之后，不被认成旗标。
    assert_eq!(
        task_args("-m o3", "/x/out").last().map(String::as_str),
        Some("-m o3")
    );
}

#[test]
fn the_launch_row_declares_this_task_shape_reading_the_result_file() {
    let face = super::super::ADAPTER
        .launch
        .and_then(|l| l.task)
        .expect("Codex 申报了一次性那一形");
    assert_eq!(
        (face.args)("summarize the repo", "/x/out"),
        task_args("summarize the repo", "/x/out")
    );
    assert!(matches!(face.text, crate::agents::TaskText::File));
    // 读法：结果文件那一份原样就是结果，标准输出不看。
    assert_eq!(
        face.text
            .read(Some("{\"ignored\":1}"), Some("all done".into())),
        Some("all done".into())
    );
    assert_eq!(face.text.read(Some("all done"), None), None);
}
