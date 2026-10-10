//! 「谁开着哪份文件写」的 Linux 实现：进程表那一趟 ＋ 打开 / 写完关闭那道耳朵。
//! 写者是 `sh` 造的假进程：`exec 3>>文件`（追加写开着）· `exec 3<文件`（只读开着），读 stdin 一行再往下走 ⇒ 关不关由判据控制。

use super::*;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-writers-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::canonicalize(&d).unwrap()
}

/// 一个开着 `file` 的假进程：`redir` 是 `>>`（追加写）或 `<`（只读）。读到第一行 ⇒ 关掉那个 fd；读到第二行 ⇒ 退出。
fn holder(file: &Path, redir: &str) -> Child {
    let mut c = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "exec 3{redir}\"$1\"; echo up; read _; exec 3>&-; echo closed; read _"
        ))
        .arg("sh")
        .arg(file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    wait_line(&mut c, "up");
    c
}

fn wait_line(c: &mut Child, want: &str) {
    use std::io::BufRead;
    let out = c.stdout.as_mut().unwrap();
    let mut line = String::new();
    std::io::BufReader::new(out).read_line(&mut line).unwrap();
    assert_eq!(line.trim(), want);
}

fn poke(c: &mut Child) {
    c.stdin.as_mut().unwrap().write_all(b"\n").unwrap();
}

fn pids_writing(file: &Path) -> Vec<u32> {
    let w = writers_of(&|p| p == file).expect("Linux 上判得了");
    w.get(file)
        .map(|v| v.iter().map(|w| w.pid).collect())
        .unwrap_or_default()
}

#[test]
fn a_process_holding_the_file_open_for_append_is_its_writer_and_a_reader_is_not() {
    let d = tmp("who");
    let f = d.join("rollout-a.jsonl");
    std::fs::write(&f, "{}\n").unwrap();
    let mut reader = holder(&f, "<");
    assert!(pids_writing(&f).is_empty(), "只读开着的不算写者");
    let mut writer = holder(&f, ">>");
    assert_eq!(pids_writing(&f), vec![writer.id()]);
    let w = writers_of(&|p| p == f).unwrap();
    assert!(
        w[&f][0].start.is_some(),
        "启动时刻读得到（挂 pidfd 看守时挡 PID 复用）"
    );
    // 关掉那个 fd（进程还活着）⇒ 不再是写者。
    poke(&mut writer);
    wait_line(&mut writer, "closed");
    assert!(pids_writing(&f).is_empty(), "关了就不算");
    for c in [&mut reader, &mut writer] {
        let _ = c.kill();
        let _ = c.wait();
    }
}

#[test]
fn the_open_flags_decide_writing_not_the_process_name() {
    assert!(
        opened_for_write("pos:\t0\nflags:\t02102001\nmnt_id:\t30\n"),
        "O_WRONLY|O_APPEND"
    );
    assert!(opened_for_write("flags:\t0100002\n"), "O_RDWR");
    assert!(!opened_for_write("flags:\t0100000\n"), "O_RDONLY");
    assert!(!opened_for_write("pos:\t0\n"), "没有 flags 那一行 ⇒ 不算");
}

fn ear(root: &Path) -> (OpenEar, mpsc::Receiver<OpenBatch>) {
    let (tx, rx) = mpsc::channel();
    let e = watch_opens(root, move |b| tx.send(b).is_ok()).expect("挂得上");
    (e, rx)
}

fn next(rx: &mpsc::Receiver<OpenBatch>) -> OpenBatch {
    rx.recv_timeout(Duration::from_secs(5)).expect("等到一批")
}

/// 收到 `pred` 成立的那一批为止（之前的批照收照丢）。
fn until(rx: &mpsc::Receiver<OpenBatch>, pred: impl Fn(&OpenBatch) -> bool) -> OpenBatch {
    loop {
        let b = next(rx);
        if pred(&b) {
            return b;
        }
    }
}

#[test]
fn the_ear_hears_create_open_and_close_after_write_but_not_appends() {
    let d = tmp("ear");
    let day = d.join("2026").join("10").join("10");
    std::fs::create_dir_all(&day).unwrap();
    let old = day.join("rollout-old.jsonl");
    std::fs::write(&old, "{}\n").unwrap();
    let (_e, rx) = ear(&d);
    // 起步时已有的文件当「打开了」报一次（挂上之前写进去的那一下听不见）。
    let first = next(&rx);
    assert_eq!(first.opened, vec![old.clone()]);
    // 新的一天的目录长出来、里面新建一份 ⇒ 报。
    let day2 = d.join("2026").join("10").join("11");
    std::fs::create_dir_all(&day2).unwrap();
    let new = day2.join("rollout-new.jsonl");
    let mut w = holder(&new, ">>");
    let b = until(&rx, |b| b.opened.contains(&new));
    assert!(b.closed.is_empty());
    // 追加写不引出任何动静（只订新建 · 打开 · 写完关闭）。
    std::fs::OpenOptions::new()
        .append(true)
        .open(&old)
        .unwrap()
        .write_all(b"{}\n")
        .unwrap();
    let b = until(&rx, |b| b.closed.contains(&old));
    assert!(!b.opened.contains(&old), "开了又关的那一下不报打开");
    // 写者把它关了 ⇒ 报关闭。
    poke(&mut w);
    let b = until(&rx, |b| !b.closed.is_empty());
    assert_eq!(b.closed, vec![new.clone()]);
    let _ = w.kill();
    let _ = w.wait();
}

/// 一次读 100 份旧记录（历史清单 · 按偏移读那一类）：读完即关 ⇒ 同一批里抵掉，一个候选都不报 ⇒ 调用方不扫进程表。
#[test]
fn a_hundred_reads_that_open_and_close_in_one_batch_report_nothing() {
    let mut fold = Fold::default();
    for i in 0..100 {
        let p = PathBuf::from(format!("/r/rollout-{i:03}.jsonl"));
        fold.apply(p.clone(), libc::IN_OPEN);
        fold.apply(p, libc::IN_CLOSE_NOWRITE);
    }
    let b = fold.finish_with(std::iter::empty());
    assert!(b.is_empty(), "读的人不引出扫描：{b:?}");
}

/// 100 份被打开、这一批读完时还开着 ⇒ 一批报齐（调用方一趟进程表认完）；之后关掉的那一批里不再报打开。
#[test]
fn a_hundred_files_left_open_come_as_one_batch() {
    let mut fold = Fold::default();
    let files: Vec<PathBuf> = (0..100)
        .map(|i| PathBuf::from(format!("/r/rollout-{i:03}.jsonl")))
        .collect();
    for p in &files {
        fold.apply(p.clone(), libc::IN_OPEN);
    }
    let b = fold.finish_with(std::iter::empty());
    assert_eq!(b.opened, files);
    assert!(b.closed.is_empty());
    for p in &files {
        fold.apply(p.clone(), libc::IN_CLOSE_WRITE);
    }
    let b = fold.finish_with(std::iter::empty());
    assert!(b.opened.is_empty());
    assert_eq!(b.closed, files, "写完关闭照报（调用方复核还有没有人写）");
}

/// 写者一直开着、读的人来了又走：这份文件还开着 ⇒ 报（调用方认得它就跳过，不扫）；开着的次数跨批记着。
#[test]
fn a_reader_passing_a_held_file_leaves_it_held() {
    let mut fold = Fold::default();
    let p = PathBuf::from("/r/rollout-held.jsonl");
    fold.apply(p.clone(), libc::IN_CREATE);
    fold.apply(p.clone(), libc::IN_OPEN);
    assert_eq!(fold.finish_with(std::iter::empty()).opened, vec![p.clone()]);
    fold.apply(p.clone(), libc::IN_OPEN);
    fold.apply(p.clone(), libc::IN_CLOSE_NOWRITE);
    assert_eq!(fold.finish_with(std::iter::empty()).opened, vec![p.clone()]);
    // 挂上之前就开着的那一份被关掉：不减成负的，也不报打开。
    let q = PathBuf::from("/r/rollout-before.jsonl");
    fold.apply(q.clone(), libc::IN_CLOSE_WRITE);
    let b = fold.finish_with(std::iter::empty());
    assert!(b.opened.is_empty());
    assert_eq!(b.closed, vec![q]);
}

#[test]
fn a_root_that_appears_later_is_heard_once_it_does() {
    let d = tmp("late");
    let root = d.join("codex").join("sessions");
    let (_e, rx) = ear(&root);
    std::fs::create_dir_all(root.join("2026")).unwrap();
    let f = root.join("2026").join("rollout-late.jsonl");
    std::fs::write(&f, "{}\n").unwrap();
    let b = until(&rx, |b| b.opened.contains(&f) || b.closed.contains(&f));
    assert!(
        b.closed.contains(&f) || b.opened.contains(&f),
        "根出现之后里面的文件听得见"
    );
}

#[test]
fn dropping_the_ear_stops_its_thread() {
    let d = tmp("drop");
    let (e, rx) = ear(&d);
    drop(e);
    // 线程退出 ⇒ 发送端随之丢掉 ⇒ 收的这一头断开（不是超时）。
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
}
