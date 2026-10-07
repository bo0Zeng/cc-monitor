//! 执行层里纯的那几样：撤销清单的顺序 · 备份名的字符集 · 时间戳。落盘那一半在 `faces/accounts_face_tests.rs` 真跑。
use super::*;

#[test]
fn undo_runs_restores_first_then_deletes_each_in_reverse() {
    let text =
        "DELETE\t/h/a\nRESTORE\t/h/b\nDELETE\t/h/c\nRESTORE\t/h/d\ngarbage\nRESTORE\trelative\n";
    let (steps, bad) = undo_steps(text);
    assert_eq!(
        steps,
        vec![
            Undo::Restore("/h/d".into()),
            Undo::Restore("/h/b".into()),
            Undo::Delete("/h/c".into()),
            Undo::Delete("/h/a".into()),
        ]
    );
    assert_eq!(bad, ["garbage", "RESTORE\trelative"]);
    // key 表那一行：`REKEY\t<表>\t<号的目录>` 归「还原」那一组；少一段 / 相对路径 ⇒ 认不出，原样报。
    let (steps, bad) = undo_steps(
        "DELETE\t/h/a\nREKEY\t/h/k.json\t/h/accts/k\nREKEY\t/h/k.json\nREKEY\tk.json\t/h/x\n",
    );
    assert_eq!(
        steps,
        vec![
            Undo::Rekey {
                table: "/h/k.json".into(),
                config_dir: "/h/accts/k".into()
            },
            Undo::Delete("/h/a".into()),
        ]
    );
    assert_eq!(bad, ["REKEY\t/h/k.json", "REKEY\tk.json\t/h/x"]);
}

#[test]
fn backup_names_are_timestamps_only() {
    for ok in ["20260930-120000", "20260930-120000-2", "a.b_c"] {
        assert!(backup_id_ok(ok), "{ok}");
    }
    for bad in ["", "../x", "a/b", "a..b", "x y", "a\nb"] {
        assert!(!backup_id_ok(bad), "{bad:?}");
    }
}
