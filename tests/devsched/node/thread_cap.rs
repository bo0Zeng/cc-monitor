//! 线程上限那一条单独一个测试二进制：它按「此刻账号线程数 ＋ 小余量」定线，
//! 跟别的测试同时跑，别人起停进程会让账号线程数在它跑的中途涨落、判不准；cargo 一个二进制跑完才起下一个。
#![allow(dead_code, unused_imports)]

include!("common.rs");

#[test]
fn thread_cap_refuses_or_stops_a_forking_job_and_we_can_still_spawn() {
    assert_line_tests_judgeable();
    let _one_at_a_time = line_test_lock();
    let r = TestRoot::new();
    let now = limits::account_threads().expect("读得到账号线程数");
    // 上限 ＝ 此刻账号线程数 ＋ 很小的余量（节点自己的几条线程也在里面）；余量 0
    let cap = (now + 25).to_string();
    let mut n = Node::start(&r.0, &["--nproc-limit", &cap, "--user-margin", "0"]);
    n.hello();
    // 先用内建命令印出自己的软限（/proc/$$/limits 的「Max processes」那一行），再起 300 个
    // 起成了的那几个记在测试根下，事后数一数撞没撞上线
    let pids = r.0.join("pids");
    n.job(
        1,
        &format!(
            r#"while read l; do case $l in "Max processes"*) echo "$l";; esac; done < /proc/$$/limits;
f='{}'; i=0; while [ $i -lt 300 ]; do sleep 30 & echo $! >> "$f"; i=$((i+1)); done; wait"#,
            pids.display()
        ),
    );
    // 作业跑着的时候：测试自己照样起得了新进程。
    // 账号线程在开跑前就涨到了线上 ⇒ 节点当场拒掉（没有输出，直接退出帧），也算对
    let t = Instant::now();
    let mut out = String::new();
    let exit = loop {
        assert!(
            Command::new("true").status().unwrap().success(),
            "测试自己起不了进程"
        );
        match n.recv(Duration::from_millis(100)) {
            Some(Frame::Exit(e)) if e.job == 1 => break e,
            Some(Frame::Out(o)) if o.job == 1 => out.push_str(&o.data),
            _ => {}
        }
        assert!(t.elapsed() < WALL_CEILING, "第 1 趟一直没被停");
    };
    let started: std::collections::BTreeSet<String> = std::fs::read_to_string(&pids)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    assert_job_hit_the_line(started.len(), 300);
    assert_eq!(exit.stop, StopReason::Threads, "该报线程上限：{exit:?}");
    // 这趟压在线上：命令的软限 ＝ 线，内核照它拒这趟的 fork（账号里别的进程起落大，不拿这趟的峰值比线）。
    // 命令没起来（垫片起它时就撞了线）⇒ 没有这一行
    if let Some(v) = out.lines().find_map(|l| l.strip_prefix("Max processes")) {
        let soft = v
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<u64>().ok());
        assert_eq!(
            soft,
            Some(now + 25),
            "这趟的软限该是线（上限 − 余量 0）：{out}"
        );
    }
}
