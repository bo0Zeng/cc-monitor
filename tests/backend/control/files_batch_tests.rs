//! `control/files_batch.rs` 的判据：同一台上一组一条（复制 · 删除 · 改权限）。全在本机临时目录上真跑。

use super::*;
use cmd_wire::batch::{Decide, ItemState, OnExists};
use cmd_wire::files::{
    ChmodItem, CopyItem, DeleteItem, FilesChmodReq, FilesCopyReq, FilesDeleteReq, WirePath,
};
use std::fs;
use std::sync::atomic::AtomicUsize;

/// 一个这次运行独占的临时目标根。
fn temp_root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-fb-{tag}-{}", std::process::id()));
    fs::remove_dir_all(&p).ok();
    fs::create_dir_all(&p).unwrap();
    p
}

fn wp(s: &str) -> WirePath {
    WirePath::from(s)
}

fn copy_req(
    root: &Path,
    items: &[(&str, &str)],
    on: OnExists,
    ticket: Option<&str>,
) -> FilesCopyReq {
    FilesCopyReq {
        root: wp(&root.to_string_lossy()),
        items: items
            .iter()
            .map(|(f, t)| CopyItem {
                from: wp(f),
                to: wp(t),
            })
            .collect(),
        on_exists: Some(on),
        decide: None,
        ticket: ticket.map(str::to_string),
    }
}

fn states(r: &BatchReply) -> Vec<ItemState> {
    r.outcomes.iter().map(|o| o.state).collect()
}

#[test]
fn a_clash_anywhere_in_the_group_leaves_every_byte_alone() {
    let root = temp_root("clash");
    fs::write(root.join("a"), b"A").unwrap();
    fs::write(root.join("b"), b"B").unwrap();
    fs::write(root.join("b2"), b"old").unwrap();
    let got = copy(
        &Face::issue(),
        copy_req(&root, &[("a", "a2"), ("b", "b2")], OnExists::Ask, None),
    );
    let Err(CopyRefused::Clash(said, c)) = got else {
        panic!("有同名而没答 ⇒ 码 exists ＋ 同名清单：{got:?}")
    };
    assert!(!said.is_empty());
    assert_eq!(c.conflicts.len(), 1);
    assert_eq!(c.conflicts[0].key, "1");
    assert!(!root.join("a2").exists(), "整组有同名时第一项也不许先做");
    assert_eq!(fs::read(root.join("b2")).unwrap(), b"old");
}

#[test]
fn decide_answers_the_clash_and_the_rest_goes_ahead() {
    let root = temp_root("decide");
    fs::write(root.join("a"), b"A").unwrap();
    fs::write(root.join("b"), b"B").unwrap();
    fs::write(root.join("b2"), b"old").unwrap();
    let mut req = copy_req(&root, &[("a", "a2"), ("b", "b2")], OnExists::Ask, None);
    req.decide = Some([("1".to_string(), Decide::Overwrite)].into_iter().collect());
    let r = copy(&Face::issue(), req).unwrap();
    assert_eq!(states(&r), vec![ItemState::Done, ItemState::Done]);
    assert_eq!(fs::read(root.join("b2")).unwrap(), b"B");
    assert_eq!(fs::read(root.join("a2")).unwrap(), b"A");
    assert!(r.said.contains('2'), "全成那一行带项数：{}", r.said);
}

#[test]
fn a_name_that_shows_up_after_the_probe_is_not_overwritten() {
    let root = temp_root("race");
    fs::write(root.join("a"), b"A").unwrap();
    // 探的时候不在（`clash: None`）、做的时候在了：照旧 `O_EXCL`，不盖，那一项 `exists`。
    fs::write(root.join("a2"), b"theirs").unwrap();
    let o = copy_item(
        &Face::issue(),
        &root,
        "0",
        Path::new("a"),
        Path::new("a2"),
        None,
    );
    assert_eq!(o.state, ItemState::Exists, "{o:?}");
    assert_eq!(fs::read(root.join("a2")).unwrap(), b"theirs");
}

#[test]
fn keep_both_walks_past_minted_names_that_are_taken() {
    let root = temp_root("mint");
    fs::write(root.join("a.txt"), b"new").unwrap();
    fs::create_dir(root.join("d")).unwrap();
    fs::write(root.join("d/a.txt"), b"old").unwrap();
    fs::write(root.join("d/a (2).txt"), b"old2").unwrap();
    let r = copy(
        &Face::issue(),
        copy_req(&root, &[("a.txt", "d/a.txt")], OnExists::KeepBoth, None),
    )
    .unwrap();
    assert_eq!(states(&r), vec![ItemState::Done]);
    assert_eq!(r.outcomes[0].new_name, Some(wp("a (3).txt")));
    assert_eq!(fs::read(root.join("d/a (3).txt")).unwrap(), b"new");
    assert_eq!(fs::read(root.join("d/a.txt")).unwrap(), b"old");
    assert_eq!(fs::read(root.join("d/a (2).txt")).unwrap(), b"old2");
}

#[test]
fn a_dir_clash_never_merges_even_when_told_to_overwrite() {
    let root = temp_root("dirclash");
    fs::create_dir_all(root.join("src/x")).unwrap();
    fs::write(root.join("src/x/f"), b"1").unwrap();
    fs::create_dir(root.join("dst")).unwrap();
    let r = copy(
        &Face::issue(),
        copy_req(&root, &[("src", "dst")], OnExists::Overwrite, None),
    )
    .unwrap();
    assert_eq!(states(&r), vec![ItemState::Skipped]);
    assert!(!root.join("dst/x").exists());
}

#[test]
fn asking_again_with_the_same_ticket_hands_back_the_same_outcome_without_redoing() {
    let root = temp_root("ticket");
    fs::write(root.join("a"), b"A").unwrap();
    let t = format!("fb-ticket-{}", std::process::id());
    let first = copy(
        &Face::issue(),
        copy_req(&root, &[("a", "a2")], OnExists::Ask, Some(&t)),
    )
    .unwrap();
    assert_eq!(states(&first), vec![ItemState::Done]);
    // 那一问的应答丢了（连接断了）；盘上又变了：再问不许重做。
    fs::remove_file(root.join("a2")).unwrap();
    let again = copy(
        &Face::issue(),
        copy_req(&root, &[("a", "a2")], OnExists::Ask, Some(&t)),
    )
    .unwrap();
    assert_eq!(again, first);
    assert!(!root.join("a2").exists(), "带同一张票再问 ⇒ 不重做");
}

#[test]
fn a_clash_answer_does_not_burn_the_ticket() {
    let root = temp_root("clashticket");
    fs::write(root.join("a"), b"A").unwrap();
    fs::write(root.join("a2"), b"old").unwrap();
    let t = format!("fb-clash-{}", std::process::id());
    assert!(matches!(
        copy(
            &Face::issue(),
            copy_req(&root, &[("a", "a2")], OnExists::Ask, Some(&t))
        ),
        Err(CopyRefused::Clash(..))
    ));
    let mut req = copy_req(&root, &[("a", "a2")], OnExists::Ask, Some(&t));
    req.decide = Some([("0".to_string(), Decide::Skip)].into_iter().collect());
    let r = copy(&Face::issue(), req).unwrap();
    assert_eq!(states(&r), vec![ItemState::Skipped]);
}

#[test]
fn stopping_finishes_the_item_in_hand_and_cancels_the_rest() {
    let root = temp_root("stop");
    for n in ["a", "b", "c", "d"] {
        fs::write(root.join(n), n.as_bytes()).unwrap();
    }
    let t = format!("fb-stop-{}", std::process::id());
    let seen = Arc::new(AtomicUsize::new(0));
    let s2 = seen.clone();
    let tt = t.clone();
    let _ear = listen(
        &t,
        Box::new(move |p| {
            s2.fetch_add(1, Ordering::SeqCst);
            if p.done == 1 {
                stop(&tt);
            }
        }),
    );
    let items = FilesDeleteReq {
        root: wp(&root.to_string_lossy()),
        items: ["a", "b", "c", "d"]
            .iter()
            .map(|n| DeleteItem {
                rel: wp(n),
                recursive: None,
                expect: None,
            })
            .collect(),
        ticket: Some(t.clone()),
    };
    let r = delete(&Face::issue(), items).unwrap();
    assert_eq!(
        states(&r),
        vec![
            ItemState::Done,
            ItemState::Done,
            ItemState::Cancelled,
            ItemState::Cancelled
        ],
        "撤的旗在第 1 项做完时立上：手上那一项（第 2 项）做完，剩下的回 cancelled"
    );
    assert!(root.join("c").exists() && root.join("d").exists());
    assert!(seen.load(Ordering::SeqCst) >= 2, "听的人收到了进度");
    assert!(!stop(&t), "做完了 ⇒ 撤回 false");
}

#[test]
fn one_failed_item_does_not_fail_the_group_and_carries_its_reason() {
    let root = temp_root("partial");
    fs::write(root.join("a"), b"A").unwrap();
    let r = chmod(
        &Face::issue(),
        FilesChmodReq {
            root: wp(&root.to_string_lossy()),
            items: vec![ChmodItem { rel: wp("a") }, ChmodItem { rel: wp("missing") }],
            mode: 0o640,
            ticket: None,
        },
    )
    .unwrap();
    assert_eq!(states(&r), vec![ItemState::Done, ItemState::Failed]);
    let bad = &r.outcomes[1];
    assert!(
        bad.code.is_some() && bad.reason.is_some() && bad.detail.is_some(),
        "{bad:?}"
    );
    assert!(r.said.contains(" · "), "部分成那一行逐样数：{}", r.said);
}
