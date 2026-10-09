//! `plan_changed`：只认计划仓与工作区 `.env`；摘要变了才推，没变不推。

use super::*;
use crate::plan::fixture::scratch;

#[test]
fn only_the_plan_repo_and_the_workspace_env_concern_us() {
    let ws = Path::new("/w");
    assert!(concerns(ws, Path::new("/w/.planned-build/alpha/图.md")));
    assert!(concerns(ws, Path::new("/w/.env")));
    assert!(!concerns(ws, Path::new("/w/alpha/src/main.rs")));
    assert!(!concerns(ws, Path::new("/w/alpha/.env")));
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
        Some(if g.len() > 1 {
            g.remove(0)
        } else {
            g[0].clone()
        })
    });
    let mut rx = changes().subscribe();
    let _w = watch(&d, Some("r1".into()), reread).unwrap();
    let ws = d.to_string_lossy().to_string();

    std::fs::write(d.join(".planned-build/alpha/图.md"), "x").unwrap();
    // 等它读过第一次（摘要 r1 ＝ 已知的 ⇒ 不推）。
    let t0 = std::time::Instant::now();
    while calls.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        assert!(t0.elapsed().as_secs() < 10, "改了计划仓却没重读");
        std::thread::yield_now();
    }
    std::fs::write(d.join(".env"), "{}").unwrap();
    let t1 = std::time::Instant::now();
    let got = loop {
        match rx.try_recv() {
            Ok((w, rev)) if w == ws => break rev,
            Ok(_) => {}
            Err(_) => {
                assert!(t1.elapsed().as_secs() < 10, "摘要变了却没推");
                std::thread::yield_now();
            }
        }
    };
    assert_eq!(got, "r2", "第一次重读摘要没变，不许推");
}
