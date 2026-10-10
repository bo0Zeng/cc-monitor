//! 盯盘的唯一原语：路径随回调交出 · 名单换得动 · 踢得醒；后端生产段里挂 `notify` 监听只此一处。

use super::*;
use std::sync::{Arc, Mutex};

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-watch-file-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// 有界等：每 20 毫秒看一眼，最多 5 秒。
fn eventually(mut ok: impl FnMut() -> bool) -> bool {
    for _ in 0..250 {
        if ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

type Seen = Arc<Mutex<Vec<Vec<PathBuf>>>>;

fn recorder() -> (Seen, impl FnMut(&[PathBuf]) + Send + 'static) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let s = Arc::clone(&seen);
    (seen, move |ps: &[PathBuf]| {
        s.lock().unwrap().push(ps.to_vec())
    })
}

/// 过了过滤的那条路径随回调交出来（调用方要按路径分派的，如浏览目录的重列）。
#[test]
fn the_callback_gets_the_paths_that_moved() {
    let d = scratch("paths");
    let (seen, cb) = recorder();
    let _w = watch(
        &[(d.clone(), false)],
        |p| p.extension().is_some_and(|e| e == "json"),
        "wf-paths",
        cb,
    )
    .unwrap();
    std::fs::write(d.join("skip.txt"), "x").unwrap();
    std::fs::write(d.join("hit.json"), "x").unwrap();
    assert!(
        eventually(|| seen
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|p| p.ends_with("hit.json"))),
        "改了过得了过滤的文件，回调没收到它的路径：{:?}",
        seen.lock().unwrap()
    );
    assert!(
        !seen
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|p| p.ends_with("skip.txt")),
        "过不了过滤的路径也交出来了"
    );
}

/// 名单换得动：起的时候没盯的目录，`rearm` 之后有动静就回调；卸掉的目录不再回调。
#[test]
fn rearm_follows_the_list() {
    let a = scratch("rearm-a");
    let b = scratch("rearm-b");
    let (seen, cb) = recorder();
    let mut w = watch(&[(a.clone(), false)], |_| true, "wf-rearm", cb).unwrap();
    let failed = w.rearm(&[(b.clone(), false)]);
    assert!(failed.is_empty(), "挂不上：{failed:?}");
    assert_eq!(w.armed(), 1, "名单一个目录，真挂着的不是 1");
    std::fs::write(b.join("new"), "x").unwrap();
    assert!(
        eventually(|| seen
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|p| p.starts_with(&b))),
        "新挂上的目录有动静，回调没来"
    );
    seen.lock().unwrap().clear();
    std::fs::write(a.join("old"), "x").unwrap();
    // 不歇：卸掉的目录要是还在回调，它那一笔排在后写的这一笔前头（同一个监听、同一条收的线程按到达次序交；
    // Linux 上两个目录共用一条 inotify 队列）⇒ 等到这一笔回来，a 那一笔该来就已经来了。
    std::fs::write(b.join("after"), "x").unwrap();
    assert!(
        eventually(|| seen
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|p| p.ends_with("after"))),
        "还挂着的目录有动静，回调没来"
    );
    assert!(
        !seen
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|p| p.starts_with(&a)),
        "卸掉的目录还在回调"
    );
}

/// 踢一下：没有文件动静也回调一次（路径为空），给「自己做完了一件事、要同步一趟」的调用方。
#[test]
fn kick_runs_the_callback_once_without_paths() {
    let d = scratch("kick");
    let (seen, cb) = recorder();
    let w = watch(&[(d, false)], |_| true, "wf-kick", cb).unwrap();
    w.kick();
    assert!(
        eventually(|| seen.lock().unwrap().iter().any(Vec::is_empty)),
        "踢了一下，回调没来"
    );
}

/// 生产段里在 `notify` 上挂监听的文件 ＝ {本原语 · 会话流那一份带去抖的}，两向。
/// 会话流那一份（`observe/watcher.rs`）盯整棵记录树、按批去抖、自己分派几十种事件，是流本身，不是「一个文件变了就重读」。
#[test]
fn only_this_primitive_hangs_a_notify_watcher() {
    let root = crate::guard_support::src_root();
    let needles = [
        format!("{}::{}", "notify", "recommended_watcher"),
        format!("{}::{}", "RecommendedWatcher", "new"),
        format!("{}(", "new_debouncer"),
    ];
    let mut hits: Vec<String> = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let code = crate::guard_support::production_code(&src);
        if needles.iter().any(|n| code.contains(n.as_str())) {
            hits.push(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    hits.sort();
    assert_eq!(
        hits,
        vec![
            "observe/watcher.rs".to_string(),
            "platform/watch_file.rs".to_string()
        ],
        "「盯一个文件变了就重读」又手写了一份：改用 platform::watch_file::watch"
    );
}
