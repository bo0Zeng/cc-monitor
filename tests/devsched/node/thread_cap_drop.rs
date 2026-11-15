//! 线程上限的另一条，单独一个测试二进制（理由同 thread_cap.rs：它按此刻账号线程数定线）。
//! 命令自己 fork 撞了这趟的线、自己退非 0，到它退出时账号线程已经落回线下几条（别的进程这几毫秒里走了）：
//! 照样报线程上限，不报「跑完了、退 2」。这里让「落回去」确定发生：撞线之后作业自己停掉 8 个 sleep 再退 2。
#![allow(dead_code, unused_imports)]

include!("common.rs");

#[test]
fn command_that_hit_the_line_reports_threads_even_if_the_account_dropped_back() {
    assert_line_tests_judgeable();
    let _one_at_a_time = line_test_lock();
    let r = TestRoot::new();
    let now = limits::account_threads().expect("读得到账号线程数");
    let cap = (now + 60).to_string();
    let mut n = Node::start(&r.0, &["--nproc-limit", &cap, "--user-margin", "0"]);
    n.hello();
    // 子 shell 起 sleep 直到 fork 被拒（子 shell 退 2）；外层只用内建命令（不 fork）：
    // 停掉其中 8 个、空转一下等垫片收掉它们，再退 2
    // 起成了的那几个记在测试根下（作业的临时目录收尾就删），事后数一数撞没撞上线
    let pids = r.0.join("pids");
    n.job(
        1,
        &format!(
            r#"f='{}'; ( i=0; while [ $i -lt 300 ]; do sleep 30 & echo $! >> "$f"; i=$((i+1)); done );
k=0; while read p; do kill $p; k=$((k+1)); [ $k -ge 8 ] && break; done < "$f";
i=0; while [ $i -lt 2000 ]; do i=$((i+1)); done; exit 2"#,
            pids.display()
        ),
    );
    let t = Instant::now();
    let exit = loop {
        if let Some(Frame::Exit(e)) = n.recv(Duration::from_millis(100)) {
            if e.job == 1 {
                break e;
            }
        }
        assert!(t.elapsed() < WALL_CEILING, "第 1 趟一直没退");
    };
    let started = std::fs::read_to_string(&pids)
        .unwrap_or_default()
        .lines()
        .count();
    assert_job_hit_the_line(started, 300);
    assert_eq!(exit.stop, StopReason::Threads, "该报线程上限：{exit:?}");
}
