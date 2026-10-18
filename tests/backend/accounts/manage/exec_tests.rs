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
    // 同一个路径先挪走（还原它）、又在原处建了链接（删它）⇒ 只还原，不在还原之后再把它删掉。
    let (steps, _) = undo_steps("RESTORE\t/h/a\nDELETE\t/h/b\nDELETE\t/h/a\n");
    assert_eq!(
        steps,
        vec![Undo::Restore("/h/a".into()), Undo::Delete("/h/b".into())]
    );
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

/// 挪（`Move`）：同一个文件系统里是改名、不丢东西 ⇒ 不先整份拷进备份，撤销记成「从挪到的那一处拷回来」（`UNMOVE\t<原处>\t<挪到的那一处>`，
/// 归「还原」那一组，原处后来建的链接不再删）；跨文件系统 · 认不出在哪个盘 ⇒ 照旧先拷备份。
#[test]
fn a_same_disk_move_is_undone_from_where_it_went_not_from_a_backup_copy() {
    assert!(!move_keeps_backup(Some(7), Some(7)));
    assert!(move_keeps_backup(Some(7), Some(8)));
    assert!(move_keeps_backup(None, Some(7)));
    assert!(move_keeps_backup(Some(7), None));
    let (steps, bad) =
        undo_steps("UNMOVE\t/h/a/p\t/h/s/p\nDELETE\t/h/s/p\nDELETE\t/h/a/p\nUNMOVE\t/h/a\n");
    assert_eq!(
        steps,
        vec![
            Undo::Back {
                at: "/h/a/p".into(),
                from: "/h/s/p".into()
            },
            Undo::Delete("/h/s/p".into()),
        ]
    );
    assert_eq!(bad, ["UNMOVE\t/h/a"]);
}
