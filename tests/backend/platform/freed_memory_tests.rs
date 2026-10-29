//! 放掉的内存还给系统：两种分配器都要还。
//! 远端 musl 版的分配器是 mimalloc（`main.rs`），缺省在没有分配活动时不会自己把空页还回去 ⇒ 起来那一下设成当场还
//! （[`return_freed_pages_at_once`]，子进程照样先调它）；本机 glibc 版靠大读之后的 `malloc_trim`（[`return_freed_memory`]）。
//! 量的是 `/proc/self/status` 的 RssAnon（整个进程一个数）⇒ 量的那一半在自己起的子进程里单独跑，不受同进程别的判据干扰。
//! 两种形状（子进程照 [`CHILD_MODE`] 分）：一条线程自己分、自己放；几条线程各分一批，一半自己放、一半递给别的线程放，
//! 然后都闲着不再分配 —— 后端的预读池与 tokio 线程就是这样（mimalloc 每条线程一个堆，别的线程放回来的块记在原主那条线程的账上）。

use super::*;
use std::alloc::{GlobalAlloc, Layout};

const CHILD_MARK: &str = "CCM_FREED_MEMORY_CHILD";
const CHILD_TEST: &str = "platform::proc::freed_memory_tests::child_gives_freed_pages_back";
/// 子进程走哪种形状：`one`（一条线程）· `threads`（[`THREADS`] 条线程各分一批、交叉着放，放完闲着）。
const CHILD_MODE: &str = "CCM_FREED_MEMORY_MODE";
const THREADS: usize = 4;
/// 子进程只读一下两个选项的缺省值就走（[`the_option_indices_are_the_two_options`]）。
#[cfg(target_env = "musl")]
const INDEX_MODE: &str = "purge-delay-index";
/// 子进程分 [`THP_TOTAL`] 小块、整块写过，读自己有多少透明大页 ＋ 几段映射要过大页（[`the_heap_takes_no_huge_pages`]）。
#[cfg(target_env = "musl")]
const THP_MODE: &str = "thp";
#[cfg(target_env = "musl")]
const THP_TOTAL: usize = 64 << 20;

/// 合计分这么多小块（每块 [`BLOCK`] 字节），每 [`KEEP_EVERY`] 块留一块不放（堆里留洞，空页不在堆顶）。
const TOTAL: usize = 200 << 20;
const BLOCK: usize = 256;
const KEEP_EVERY: usize = 4096;
/// 还完之后至多比起点多这么多（留着的那几块 ＋ 分配器自己的账）。
const SLACK_KB: u64 = 50 << 10;

fn rss_anon_kb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").expect("读 /proc/self/status");
    s.lines()
        .find_map(|l| l.strip_prefix("RssAnon:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .expect("RssAnon 那一行")
}

/// 这个二进制的分配器：musl 版经 mimalloc（远端二进制的全局分配器；测试二进制没有换，判据直接经它分）· glibc 版是系统的。
fn allocator() -> &'static dyn GlobalAlloc {
    #[cfg(target_env = "musl")]
    {
        static A: mimalloc::MiMalloc = mimalloc::MiMalloc;
        &A
    }
    #[cfg(not(target_env = "musl"))]
    {
        &std::alloc::System
    }
}

fn layout() -> Layout {
    Layout::from_size_align(BLOCK, 16).unwrap()
}

/// 在这条线程上分 `n` 块、整块写一遍（页真的落进常驻）；回地址（`usize`：要跨线程递）。
fn alloc_blocks(n: usize, seed: usize) -> Vec<usize> {
    let a = allocator();
    (0..n)
        .map(|i| {
            // SAFETY: 布局非零大小；拿到的块整块写一遍，之后照同一个布局放回同一个分配器（`free_blocks`）。
            let p = unsafe { a.alloc(layout()) };
            assert!(!p.is_null());
            unsafe { std::ptr::write_bytes(p, ((i + seed) % 251) as u8, BLOCK) };
            p as usize
        })
        .collect()
}

/// 放回 [`alloc_blocks`] 拿的块（在哪条线程上放都行）。
fn free_blocks(ps: impl IntoIterator<Item = usize>) {
    let a = allocator();
    for p in ps {
        // SAFETY: 从同一个分配器照同一个布局拿的，只放一次。
        unsafe { a.dealloc(p as *mut u8, layout()) };
    }
}

/// 每 [`KEEP_EVERY`] 块留一块；回（留下的，要放的）。
fn split_kept(ps: Vec<usize>) -> (Vec<usize>, Vec<usize>) {
    let (mut kept, mut gone) = (Vec::new(), Vec::new());
    for (i, p) in ps.into_iter().enumerate() {
        if i % KEEP_EVERY == 0 {
            kept.push(p);
        } else {
            gone.push(p);
        }
    }
    (kept, gone)
}

/// 几条线程各分一批：放掉的一半自己放、一半递给主线程放；之后各自闲着（阻塞在收信上，不再分配），量完才走。
/// 回（量到的峰 · 留下的块 · 闲着的线程：丢掉发端它就走）。
fn threads_alloc_and_go_idle() -> (
    u64,
    Vec<usize>,
    Vec<(std::sync::mpsc::Sender<()>, std::thread::JoinHandle<()>)>,
) {
    let (done_tx, done_rx) = std::sync::mpsc::channel::<(Vec<usize>, Vec<usize>)>();
    let (freed_tx, freed_rx) = std::sync::mpsc::channel::<()>();
    let mut idle = Vec::new();
    for t in 0..THREADS {
        let (done_tx, freed_tx) = (done_tx.clone(), freed_tx.clone());
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let h = std::thread::spawn(move || {
            let (kept, mut mine) = split_kept(alloc_blocks(TOTAL / BLOCK / THREADS, t));
            let theirs = mine.split_off(mine.len() / 2);
            done_tx.send((kept, theirs)).unwrap();
            // 先递出去、等主线程量过峰再放自己那一半。
            let _ = go_rx.recv();
            free_blocks(mine);
            freed_tx.send(()).unwrap();
            // 闲着：不再分配，等主线程量完。
            let _ = go_rx.recv();
        });
        idle.push((go_tx, h));
    }
    drop((done_tx, freed_tx));
    let mut kept = Vec::new();
    let mut theirs = Vec::new();
    for (k, o) in done_rx.iter().take(THREADS) {
        kept.extend(k);
        theirs.extend(o);
    }
    let peak = rss_anon_kb();
    free_blocks(theirs);
    for (go, _) in &idle {
        go.send(()).unwrap();
    }
    for _ in 0..THREADS {
        freed_rx.recv().unwrap();
    }
    (peak, kept, idle)
}

#[test]
#[ignore = "只在 gives_freed_pages_back 起的子进程里跑"]
fn child_gives_freed_pages_back() {
    if std::env::var(CHILD_MARK).is_err() {
        return;
    }
    let mode = std::env::var(CHILD_MODE).unwrap_or_default();
    #[cfg(target_env = "musl")]
    if mode == INDEX_MODE {
        extern "C" {
            fn mi_option_get(option: i32) -> std::ffi::c_long;
        }
        // SAFETY: mimalloc 的公开口，只读一个整数选项。
        let (d, r) = unsafe {
            (
                mi_option_get(MI_OPTION_PURGE_DELAY),
                mi_option_get(MI_OPTION_PAGE_FULL_RETAIN),
            )
        };
        println!("start= purge_delay={d} page_full_retain={r}");
        return;
    }
    #[cfg(target_env = "musl")]
    if mode == THP_MODE {
        return_freed_pages_at_once();
        let held = alloc_blocks(THP_TOTAL / BLOCK, 0);
        let rollup =
            std::fs::read_to_string("/proc/self/smaps_rollup").expect("读 /proc/self/smaps_rollup");
        let thp_kb: u64 = rollup
            .lines()
            .find_map(|l| l.strip_prefix("AnonHugePages:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
            .expect("AnonHugePages 那一行");
        // 打过 `MADV_HUGEPAGE` 的映射在 smaps 的 VmFlags 里带 `hg`：内核此刻凑不出 2 MB 连续物理页时大页数可能是 0，标记照样在。
        let smaps = std::fs::read_to_string("/proc/self/smaps").expect("读 /proc/self/smaps");
        let flagged = smaps
            .lines()
            .filter_map(|l| l.strip_prefix("VmFlags:"))
            .filter(|f| f.split_whitespace().any(|x| x == "hg"))
            .count();
        println!(
            "start={} thp_kb={thp_kb} hg_maps={flagged} kB",
            rss_anon_kb()
        );
        free_blocks(held);
        return;
    }
    let threads = mode == "threads";
    // 照后端起来那一下（`main.rs`）先设好分配器。
    return_freed_pages_at_once();
    let start = rss_anon_kb();
    let (peak, kept, idle) = if threads {
        threads_alloc_and_go_idle()
    } else {
        let (kept, gone) = split_kept(alloc_blocks(TOTAL / BLOCK, 0));
        let peak = rss_anon_kb();
        free_blocks(gone);
        (peak, kept, Vec::new())
    };
    return_freed_memory(TRIM_AFTER_BYTES);
    let after = rss_anon_kb();
    println!("start={start} peak={peak} after={after} kB");
    assert!(
        peak >= start + (150 << 10),
        "正控：分了 200 MB 常驻该涨上去（起点 {start} kB，峰 {peak} kB）"
    );
    assert!(
        after <= start + SLACK_KB,
        "放掉的 {} MB 小块没还给系统（{}）：起点 {start} kB，峰 {peak} kB，还完之后 {after} kB",
        TOTAL >> 20,
        if threads {
            "几条线程各分各放、放完闲着"
        } else {
            "一条线程"
        }
    );
    free_blocks(kept);
    for (go, h) in idle {
        drop(go);
        h.join().unwrap();
    }
}

/// 起子进程跑 [`child_gives_freed_pages_back`] 的 `mode` 那一种形状；回它说的话。
fn run_child(mode: &str) -> String {
    let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([
            CHILD_TEST,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_MARK, "1")
        .env(CHILD_MODE, mode)
        .env("HOME", std::env::temp_dir())
        .output()
        .expect("起子进程");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.contains("start=") && said.contains("1 passed"),
        "子进程里那一趟（{mode}）没过（或没跑到）：\n{said}"
    );
    said
}

#[test]
fn gives_freed_pages_back() {
    run_child("one");
}

#[test]
fn gives_back_what_idle_threads_freed() {
    run_child("threads");
}

/// 设的那两个序号真是那两个选项：子进程里没设之前读，是 mimalloc 给的缺省（序号挪了 ⇒ 读到别的选项的值）。
#[cfg(target_env = "musl")]
#[test]
fn the_option_indices_are_the_two_options() {
    let said = run_child(INDEX_MODE);
    assert!(
        said.contains("purge_delay=1000 page_full_retain=2"),
        "两个序号读到的不是 purge_delay（缺省 1000 ms）· page_full_retain（缺省 2）（mimalloc 升级后序号挪了？）：\n{said}"
    );
}

/// 堆不要透明大页：mimalloc 缺省给 arena 打 `MADV_HUGEPAGE`，`madvise` 档的机器上半满的 2 MB 区段被并成大页，
/// 空闲常驻每次多出几十 MB、而且每次不一样 ⇒ 清单里编进 `no_thp`。只有 `madvise` 档判得出：`never` 档谁都拿不到大页，
/// `always` 档不打标记也照样给（那时 glibc 版一样，不归本条）。
#[cfg(target_env = "musl")]
#[test]
fn the_heap_takes_no_huge_pages() {
    let tier =
        std::fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled").unwrap_or_default();
    let Some(now) = tier.split_whitespace().find(|w| w.starts_with('[')) else {
        println!("判不了：读不到透明大页的档（/sys/kernel/mm/transparent_hugepage/enabled）");
        return;
    };
    if now != "[madvise]" {
        println!("判不了：透明大页的档是 {now}，只有 [madvise] 档判得出分配器要没要大页");
        return;
    }
    let said = run_child(THP_MODE);
    assert!(
        said.contains(" thp_kb=0 hg_maps=0 "),
        "分配器给堆要了透明大页（musl 版 mimalloc 没编进 `no_thp`？）：\n{said}"
    );
}

#[test]
fn a_small_read_does_not_trigger_it() {
    assert!(!return_freed_memory(TRIM_AFTER_BYTES - 1));
    assert!(return_freed_memory(TRIM_AFTER_BYTES));
}
