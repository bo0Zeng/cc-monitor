//! `changed {plan}`：只认计划仓与工作区 `.env`；摘要变了才推，没变不推。

use super::*;
use crate::plan::fixture::{dump_at, scratch, who};

/// 成品（工作区 `ws`，那一片的仓库登记在 `path`）。
fn product(ws: &Path, path: &str) -> serde_json::Value {
    crate::plan::product::make(&dump_at(&ws.to_string_lossy(), path), &who).doc
}

#[test]
fn only_the_plan_repo_the_workspace_env_and_declared_files_concern_us() {
    let ws = Path::new("/w");
    let none = Spots::default();
    assert!(concerns(
        ws,
        &none,
        Path::new("/w/.planned-build/alpha/图.md")
    ));
    assert!(concerns(ws, &none, Path::new("/w/.env")));
    assert!(!concerns(ws, &none, Path::new("/w/alpha/src/main.rs")));
    assert!(!concerns(ws, &none, Path::new("/w/alpha/.env")));
    // 仓库登记在 repos/alpha：声明过的文件 · 通往它的目录（整个挪走 / 删掉）算数；仓库里别的（编译产物）不算。
    let sp = spots(&product(ws, "repos/alpha"));
    assert!(concerns(ws, &sp, Path::new("/w/repos/alpha/src/read.txt")));
    assert!(concerns(ws, &sp, Path::new("/w/repos/alpha/src/write.txt")));
    assert!(concerns(ws, &sp, Path::new("/w/repos/alpha/src")));
    assert!(concerns(ws, &sp, Path::new("/w/repos/alpha")));
    assert!(!concerns(ws, &sp, Path::new("/w/repos/alpha/target/x.o")));
    assert!(!concerns(
        ws,
        &sp,
        Path::new("/w/repos/alpha/src/other.txt")
    ));
    assert!(!concerns(ws, &sp, Path::new("/w/alpha/src/read.txt")));
    // 盯的目录：工作区根到每份声明文件所在目录一路上的每一层（都只盯直接子项）。
    assert_eq!(
        sp.dirs,
        ["/w/repos", "/w/repos/alpha", "/w/repos/alpha/src"].map(std::path::PathBuf::from)
    );
}

/// 等一帧这个工作区的推送，回它的摘要。
fn next_rev(rx: &mut tokio::sync::broadcast::Receiver<Change>, ws: &str, what: &str) -> String {
    let t0 = std::time::Instant::now();
    loop {
        match rx.try_recv() {
            Ok((w, rev, _)) if w == ws => return rev,
            Ok(_) => {}
            Err(_) => {
                assert!(t0.elapsed() < guard_core::wall::CEILING, "{what}");
                std::thread::yield_now();
            }
        }
    }
}

/// 删掉一片仓库里声明过的一份文件 ⇒ 重读（对账跟着变「缺」）⇒ 推。
#[test]
fn deleting_a_declared_file_in_a_slice_repo_rereads() {
    let d = scratch("watch-repo");
    std::fs::create_dir_all(d.join(".planned-build/alpha")).unwrap();
    std::fs::create_dir_all(d.join("repos/alpha/src")).unwrap();
    std::fs::write(d.join("repos/alpha/src/read.txt"), "x").unwrap();
    let doc = product(&d, "repos/alpha");
    let sp = spots(&doc);
    let reread: Reread = Arc::new(move |_p: &Path| Some((("r2".to_string(), 0), sp.clone())));
    let mut rx = changes().subscribe();
    let _w = watch(&d, spots(&doc), Some(("r1".into(), 0)), reread).unwrap();
    let ws = d.to_string_lossy().to_string();
    std::fs::remove_file(d.join("repos/alpha/src/read.txt")).unwrap();
    assert_eq!(next_rev(&mut rx, &ws, "删了仓库里声明的文件却没重读"), "r2");
}

/// 重读出来仓库换了位置（`pb repo` 搬家）⇒ 盯的名单跟着换：新位置里的文件删了照样重读。
#[test]
fn the_watched_repos_follow_the_registered_path() {
    let d = scratch("watch-move");
    std::fs::create_dir_all(d.join(".planned-build/alpha")).unwrap();
    for r in ["repos/alpha", "repos/beta"] {
        std::fs::create_dir_all(d.join(r).join("src")).unwrap();
        std::fs::write(d.join(r).join("src/read.txt"), "x").unwrap();
    }
    let moved = spots(&product(&d, "repos/beta"));
    let revs = Arc::new(Mutex::new(vec!["r2".to_string(), "r3".to_string()]));
    let r = Arc::clone(&revs);
    let reread: Reread = Arc::new(move |_p: &Path| {
        let mut g = r.lock().unwrap();
        let rev = if g.len() > 1 {
            g.remove(0)
        } else {
            g[0].clone()
        };
        Some(((rev, 0), moved.clone()))
    });
    let mut rx = changes().subscribe();
    let _w = watch(
        &d,
        spots(&product(&d, "repos/alpha")),
        Some(("r1".into(), 0)),
        reread,
    )
    .unwrap();
    let ws = d.to_string_lossy().to_string();
    std::fs::remove_file(d.join("repos/alpha/src/read.txt")).unwrap();
    assert_eq!(next_rev(&mut rx, &ws, "旧位置删了文件却没重读"), "r2");
    std::fs::remove_file(d.join("repos/beta/src/read.txt")).unwrap();
    assert_eq!(
        next_rev(&mut rx, &ws, "搬到新位置后，那里删了文件却没重读"),
        "r3"
    );
}

/// 改计划仓 ⇒ 重读；摘要第一次与已知的一样 ⇒ 不推；第二次变了 ⇒ 推一帧 `(工作区, 新摘要)`。
#[test]
fn a_change_pushes_only_when_the_rev_moves() {
    let d = scratch("watch-push");
    std::fs::create_dir_all(d.join(".planned-build/alpha")).unwrap();
    let revs = Arc::new(Mutex::new(vec!["r1".to_string(), "r2".to_string()]));
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (r, c) = (Arc::clone(&revs), Arc::clone(&calls));
    let reread: Reread = Arc::new(move |_p: &Path| {
        c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut g = r.lock().unwrap();
        let rev = if g.len() > 1 {
            g.remove(0)
        } else {
            g[0].clone()
        };
        Some(((rev, 0), Spots::default()))
    });
    let mut rx = changes().subscribe();
    let _w = watch(&d, Spots::default(), Some(("r1".into(), 0)), reread).unwrap();
    let ws = d.to_string_lossy().to_string();

    std::fs::write(d.join(".planned-build/alpha/图.md"), "x").unwrap();
    // 等它读过第一次（摘要 r1 ＝ 已知的 ⇒ 不推）。
    let t0 = std::time::Instant::now();
    while calls.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        assert!(
            t0.elapsed() < guard_core::wall::CEILING,
            "改了计划仓却没重读"
        );
        std::thread::yield_now();
    }
    std::fs::write(d.join(".env"), "{}").unwrap();
    let t1 = std::time::Instant::now();
    let got = loop {
        match rx.try_recv() {
            Ok((w, rev, _)) if w == ws => break rev,
            Ok(_) => {}
            Err(_) => {
                assert!(t1.elapsed() < guard_core::wall::CEILING, "摘要变了却没推");
                std::thread::yield_now();
            }
        }
    };
    assert_eq!(got, "r2", "第一次重读摘要没变，不许推");
}

/// 摘要没变、要你看的数变了（会话停了 · 认可了）⇒ 也推，带新的数。
#[test]
fn a_change_in_the_need_count_alone_also_pushes() {
    let d = scratch("watch-needs");
    std::fs::create_dir_all(d.join(".planned-build/alpha")).unwrap();
    let reread: Reread = Arc::new(move |_p: &Path| Some((("r1".to_string(), 2), Spots::default())));
    let mut rx = changes().subscribe();
    let _w = watch(&d, Spots::default(), Some(("r1".into(), 1)), reread).unwrap();
    let ws = d.to_string_lossy().to_string();
    std::fs::write(d.join(".planned-build/alpha/图.md"), "x").unwrap();
    let t0 = std::time::Instant::now();
    let got = loop {
        match rx.try_recv() {
            Ok((w, rev, n)) if w == ws => break (rev, n),
            Ok(_) => {}
            Err(_) => {
                assert!(t0.elapsed() < guard_core::wall::CEILING, "数变了却没推");
                std::thread::yield_now();
            }
        }
    };
    assert_eq!(got, ("r1".to_string(), 2));
}
