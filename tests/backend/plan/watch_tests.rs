//! `changed {plan}`：只认计划仓（连 pb 0.5 的工作区配置 `.planned-build/.env`）与声明过的文件；摘要变了才推，没变不推。

use super::*;
use crate::plan::fixture::{dump_at, made_with, who};
use test_home::scratch;

/// 成品（工作区 `ws`，那一片的仓库登记在 `path`）。
fn product(ws: &Path, path: &str) -> cmd_wire::plan::PlanRead {
    made_with(&dump_at(&ws.to_string_lossy(), path), &who)
}

#[test]
fn only_the_plan_repo_and_declared_files_concern_us() {
    let ws = Path::new("/w");
    let none = Spots::default();
    assert!(concerns(
        ws,
        &none,
        Path::new("/w/.planned-build/alpha/图.md")
    ));
    assert!(concerns(ws, &none, Path::new("/w/.planned-build/.env")));
    assert!(
        !concerns(ws, &none, Path::new("/w/.env")),
        "pb 0.5 的工作区配置在计划仓里，工作区根上那一份不认"
    );
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
    // 盯的目录：工作区根到每份声明文件所在目录一路上的每一层（都只盯直接子项）；beta 那一片的仓库还不在（只写文档）也在名单上，
    // 出现了才挂（上一层看得见它被建出来）。
    assert_eq!(
        sp.dirs.iter().cloned().collect::<Vec<_>>(),
        [
            "/w/beta",
            "/w/repos",
            "/w/repos/alpha",
            "/w/repos/alpha/src",
            "/w/repos/alpha/tests"
        ]
        .map(std::path::PathBuf::from)
        .to_vec()
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
    std::fs::write(d.join(".planned-build/.env"), "{}").unwrap();
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

/// 一片声明了 `n` 份文件（分在几十层目录里，照 cc-monitor 那样几千份）的成品。
fn product_with_files(ws: &Path, n: usize) -> cmd_wire::plan::PlanRead {
    let mut doc = product(ws, "repos/alpha");
    let cell = doc.slices[0]
        .cells
        .iter_mut()
        .find(|c| !c.files.is_empty())
        .expect("夹具里有一格声明了文件");
    let one = cell.files[0].clone();
    cell.files = (0..n)
        .map(|i| {
            let mut f = one.clone();
            f.path = format!("src/m{}/d{}/f{i}.rs", i % 97, i % 13);
            f
        })
        .collect();
    doc
}

/// ★ 大规模声明下不退化：5000 份声明算一次名单，与它的八分之一比，耗时不许照平方涨（平方 ⇒ 64 倍；集合 ⇒ 十倍上下）。
/// 每档取几次里最快的那一次，挡掉机器忙的抖动。
#[test]
fn spots_scale_near_linearly_with_the_declared_files() {
    let ws = Path::new("/w");
    let (small, big) = (product_with_files(ws, 625), product_with_files(ws, 5000));
    let best = |doc: &cmd_wire::plan::PlanRead| {
        (0..5)
            .map(|_| {
                let t = std::time::Instant::now();
                let sp = spots(doc);
                let e = t.elapsed();
                assert!(!sp.hits.is_empty());
                e
            })
            .min()
            .unwrap()
    };
    let (ts, tb) = (best(&small), best(&big));
    assert!(spots(&big).hits.len() >= 5000, "5000 份声明都在名单上");
    let ratio = tb.as_secs_f64() / ts.as_secs_f64().max(1e-6);
    assert!(
        ratio < 24.0,
        "声明多八倍、名单慢了 {ratio:.1} 倍（{ts:?} → {tb:?}）：照平方涨了"
    );
}

/// 没在盯的工作区 ⇒ 不等、回 false（帧面照此起 pb 现读）。
#[test]
fn settled_is_false_for_a_workspace_nobody_watches() {
    assert!(!settled(Path::new("/nowhere/watched")));
}

/// ★ 已在盯：这期间没动静 ⇒ 当场 true；有动静 ⇒ 等这一趟重读完才回 true（本子里那一份才是此刻的）。
#[test]
fn settled_waits_for_the_reread_of_what_was_heard() {
    let d = scratch("watch-settled");
    std::fs::create_dir_all(d.join(".planned-build/alpha")).unwrap();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let go_rx = Mutex::new(go_rx);
    let started = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let s = Arc::clone(&started);
    let reread: Reread = Arc::new(move |_p: &Path| {
        s.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        go_rx.lock().unwrap().recv().ok();
        Some((("r2".to_string(), 0), Spots::default()))
    });
    arm(&d, || (Spots::default(), Some(("r1".into(), 0))), reread);
    assert!(settled(&d), "没动静 ⇒ 当场追上");
    std::fs::write(d.join(".planned-build/alpha/图.md"), "x").unwrap();
    let t0 = std::time::Instant::now();
    while started.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        assert!(
            t0.elapsed() < guard_core::wall::CEILING,
            "改了计划仓却没重读"
        );
        std::thread::yield_now();
    }
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let dn = Arc::clone(&done);
    let ws = d.clone();
    let waiter = std::thread::spawn(move || {
        let got = settled(&ws);
        dn.store(true, std::sync::atomic::Ordering::SeqCst);
        got
    });
    let t1 = std::time::Instant::now();
    while t1.elapsed().as_millis() < 100 {
        std::thread::yield_now();
    }
    assert!(
        !done.load(std::sync::atomic::Ordering::SeqCst),
        "重读还没完就回了：本子里那一份是旧的"
    );
    // 放行这一趟；之后还有几趟（写一份常是几条事件）也不再卡。
    go_tx.send(()).unwrap();
    drop(go_tx);
    assert!(waiter.join().unwrap(), "重读完了该回 true");
}

/// ★ 已在盯 ⇒ 不再算一遍此刻的样子与名单（5000 份声明算一次要几百毫秒，每读一次都白算）。
#[test]
fn arming_a_watched_workspace_again_does_not_compute_its_spots() {
    let d = scratch("watch-twice");
    std::fs::create_dir_all(d.join(".planned-build/alpha")).unwrap();
    let reread: Reread = Arc::new(|_p: &Path| None);
    let calls = std::cell::Cell::new(0);
    let init = || {
        calls.set(calls.get() + 1);
        (Spots::default(), None)
    };
    arm(&d, init, Arc::clone(&reread));
    arm(&d, init, reread);
    assert_eq!(calls.get(), 1, "第二次已在盯，不该再算");
}
