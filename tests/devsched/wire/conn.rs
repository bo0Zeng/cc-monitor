//! 连接的两个实现：进程内一对内存管道、子进程管道。

use devsched_wire::conn::{ChildConn, Conn, MemConn, Recv};
use devsched_wire::{samples, Frame};
use std::time::{Duration, Instant};

const T: Duration = devsched_wire::WALL_CEILING;

#[test]
fn mem_pair_carries_frames_both_ways() {
    let (mut a, mut b) = MemConn::pair();
    for f in samples() {
        a.send(&f).unwrap();
        match b.recv_timeout(T) {
            Recv::Frame(g) => assert_eq!(g, f),
            other => panic!("{other:?}"),
        }
    }
    b.send(&Frame::Beat).unwrap();
    assert!(matches!(a.recv_timeout(T), Recv::Frame(Frame::Beat)));
}

#[test]
fn mem_recv_times_out_then_sees_close() {
    let (mut a, mut b) = MemConn::pair();
    let t0 = Instant::now();
    assert!(matches!(
        b.recv_timeout(Duration::from_millis(50)),
        Recv::Timeout
    ));
    assert!(t0.elapsed() >= Duration::from_millis(40));
    a.close();
    assert!(matches!(b.recv_timeout(T), Recv::Closed));
    // 关了之后一直是关了，不挂住
    assert!(matches!(b.recv_timeout(T), Recv::Closed));
    // 往关了的那头写报错
    assert!(a.send(&Frame::Beat).is_err());
}

#[test]
fn mem_drop_closes_the_other_end() {
    let (a, mut b) = MemConn::pair();
    drop(a);
    assert!(matches!(b.recv_timeout(T), Recv::Closed));
}

#[test]
fn child_cat_echoes_frames() {
    let mut c = ChildConn::spawn(&["cat".to_string()]).unwrap();
    for f in samples() {
        c.send(&f).unwrap();
        match c.recv_timeout(T) {
            Recv::Frame(g) => assert_eq!(g, f),
            other => panic!("{other:?}"),
        }
    }
    c.close();
    assert!(matches!(c.recv_timeout(T), Recv::Closed));
}

#[test]
fn child_bad_line_then_close() {
    let argv: Vec<String> = ["sh", "-c", "echo garbage; echo '{\"t\":\"beat\"}'"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut c = ChildConn::spawn(&argv).unwrap();
    assert!(matches!(c.recv_timeout(T), Recv::Bad(_)));
    assert!(matches!(c.recv_timeout(T), Recv::Frame(Frame::Beat)));
    assert!(matches!(c.recv_timeout(T), Recv::Closed));
    c.close();
}

#[test]
fn child_close_kills_one_that_ignores_stdin() {
    // 不理标准输入、睡够了才留一个记号：close_within 过了时限不杀它，就会等到它睡完、记号就在
    let mark = std::env::temp_dir().join(format!("dsw-close-{}", std::process::id()));
    let _ = std::fs::remove_file(&mark);
    let script = format!("sleep 30; touch {}", mark.display());
    let argv: Vec<String> = vec!["sh".into(), "-c".into(), script];
    let mut c = ChildConn::spawn(&argv).unwrap();
    c.close_within(Duration::from_millis(200));
    assert!(c.exited());
    assert!(!mark.exists(), "过了时限该杀掉，不该等它自己睡完");
}

#[test]
fn child_spawn_failure_is_an_error() {
    assert!(ChildConn::spawn(&["/nonexistent/devsched-xyz".to_string()]).is_err());
    assert!(ChildConn::spawn(&[]).is_err());
}
