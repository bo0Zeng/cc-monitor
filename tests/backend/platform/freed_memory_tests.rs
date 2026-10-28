//! 一趟大读放掉的内存还给系统（[`return_freed_memory_after`]）：两种分配器都要还。
//! 远端 musl 版的分配器是 mimalloc（`main.rs`），它在没有分配活动时不会自己把空页还回去；本机 glibc 版靠 `malloc_trim`。
//! 量的是 `/proc/self/status` 的 RssAnon（整个进程一个数）⇒ 量的那一半在自己起的子进程里单独跑，不受同进程别的判据干扰。

use super::*;
use std::alloc::{GlobalAlloc, Layout};

const CHILD_MARK: &str = "CCM_FREED_MEMORY_CHILD";
const CHILD_TEST: &str = "platform::proc::freed_memory_tests::child_gives_freed_pages_back";

/// 分这么多小块（每块 [`BLOCK`] 字节），每 [`KEEP_EVERY`] 块留一块不放（堆里留洞，空页不在堆顶）。
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

#[test]
#[ignore = "只在 gives_freed_pages_back 起的子进程里跑"]
fn child_gives_freed_pages_back() {
    if std::env::var(CHILD_MARK).is_err() {
        return;
    }
    let a = allocator();
    let layout = Layout::from_size_align(BLOCK, 16).unwrap();
    let n = TOTAL / BLOCK;
    let mut held: Vec<*mut u8> = Vec::with_capacity(n);
    let start = rss_anon_kb();
    for i in 0..n {
        // SAFETY: 布局非零大小；拿到的块整块写一遍（让页真的落进常驻），下面照同一个布局放回同一个分配器。
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null());
        unsafe { std::ptr::write_bytes(p, (i % 251) as u8, BLOCK) };
        held.push(p);
    }
    let peak = rss_anon_kb();
    let mut kept = Vec::new();
    for (i, p) in held.drain(..).enumerate() {
        if i % KEEP_EVERY == 0 {
            kept.push(p);
        } else {
            // SAFETY: 上面从同一个分配器照同一个布局拿的，只放一次。
            unsafe { a.dealloc(p, layout) };
        }
    }
    drop(held);
    return_freed_memory_after(TRIM_AFTER_BYTES);
    let after = rss_anon_kb();
    println!("start={start} peak={peak} after={after} kB");
    assert!(
        peak >= start + (150 << 10),
        "正控：分了 200 MB 常驻该涨上去（起点 {start} kB，峰 {peak} kB）"
    );
    assert!(
        after <= start + SLACK_KB,
        "放掉的 {} MB 小块没还给系统：起点 {start} kB，峰 {peak} kB，还完之后 {after} kB",
        TOTAL >> 20
    );
    for p in kept {
        // SAFETY: 同上。
        unsafe { a.dealloc(p, layout) };
    }
}

#[test]
fn gives_freed_pages_back() {
    let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([CHILD_TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD_MARK, "1")
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
        "子进程里那一趟没过（或没跑到）：\n{said}"
    );
}

#[test]
fn a_small_read_does_not_trigger_it() {
    assert!(!return_freed_memory_after(TRIM_AFTER_BYTES - 1));
    assert!(return_freed_memory_after(TRIM_AFTER_BYTES));
}
