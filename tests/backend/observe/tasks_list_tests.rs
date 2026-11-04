//! `tasks-list` 的成品句：每格的状态字 `statusText`、整份的 `chip` · `hint` 由后端照状态写好，界面只摆。夹具只造结构。

use super::*;
use copy_core::copy_text;

fn t(id: &str, status: &str) -> Task {
    task_entry(&serde_json::json!({"id": id, "subject": format!("s{id}"), "status": status}))
        .unwrap()
}

#[test]
fn each_task_says_its_status_and_the_list_says_its_counts() {
    let list = tasks_list(vec![
        t("1", "completed"),
        t("2", "in_progress"),
        t("3", "weird"),
    ]);
    let words: Vec<&str> = list.tasks.iter().map(|x| x.status_text.as_str()).collect();
    assert_eq!(
        words,
        [
            copy_text("runs.state.done", &[]),
            copy_text("tasksPanel.status.inProgress", &[]),
            copy_text("tasksPanel.status.other", &[]),
        ]
    );
    assert_eq!(
        list.chip,
        copy_text("statusBar.tasks.chip", &[("done", "1"), ("total", "3")])
    );
    assert_eq!(
        list.hint,
        copy_text(
            "statusBar.tasks.hint",
            &[("done", "1"), ("active", "1"), ("open", "1")]
        )
    );
    assert_eq!(
        t("4", "pending").status_text,
        copy_text("tasksPanel.status.pending", &[])
    );
    assert_eq!(
        t("5", "deleted").status_text,
        copy_text("tasksPanel.status.failed", &[])
    );
}

#[test]
fn an_empty_list_says_nothing() {
    let list = tasks_list(Vec::new());
    assert_eq!((list.chip.as_str(), list.hint.as_str()), ("", ""));
}
