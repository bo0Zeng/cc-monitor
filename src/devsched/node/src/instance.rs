//! 单实例：`<根>/state/node.lock` 文件锁。拿到锁的是主节点，锁随进程活着；拿不到的只当转发。

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

/// 拿到的锁；丢掉（或进程没了）就放。
#[derive(Debug)]
pub struct NodeLock {
    _file: File,
}

/// 这一个节点当什么。
#[derive(Debug)]
pub enum Role {
    /// 拿到锁：干活的那一个。
    Main(NodeLock),
    /// 同一个根上已有主节点：只把连接转过去。
    Forward,
}

/// 试拿锁，不等。锁文件打不开 ⇒ 错（不当主节点、也不当转发）。
pub fn acquire(lock_file: &Path) -> io::Result<Role> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_file)?;
    // 执行新程序时不带过去（std 打开的文件默认 O_CLOEXEC）
    let r = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if r == 0 {
        return Ok(Role::Main(NodeLock { _file: f }));
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(Role::Forward)
    } else {
        Err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::testdir::TestDir;
    use std::time::{Duration, Instant};

    /// 放了以后锁不一定当场空：同一进程里别的测试此刻 fork 出的子进程在 exec 之前攥着同一份打开的文件（flock 跟着它走），
    /// exec 时才关。等那份真放开（兜底期限内反复试），不看一次就判。
    fn wait_main(p: &Path) {
        let t0 = Instant::now();
        loop {
            match acquire(p).unwrap() {
                Role::Main(_) => return,
                Role::Forward if t0.elapsed() < devsched_wire::WALL_CEILING => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Role::Forward => panic!("锁放了，过了兜底期限还拿不到"),
            }
        }
    }

    /// fork 一份；账号撞了线程线（EAGAIN：机器忙、或测试本身跑在压着软限的作业里）就等一会儿再试，兜底期限内起不来才判红。
    fn fork_waiting() -> libc::pid_t {
        let t0 = Instant::now();
        loop {
            let pid = unsafe { libc::fork() };
            if pid >= 0 {
                return pid;
            }
            let e = io::Error::last_os_error();
            assert!(
                e.raw_os_error() == Some(libc::EAGAIN)
                    && t0.elapsed() < devsched_wire::WALL_CEILING,
                "fork 不成：{e}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn second_acquire_forwards_until_first_releases() {
        let d = TestDir::new("lock");
        let p = d.0.join("node.lock");
        let first = acquire(&p).unwrap();
        assert!(matches!(first, Role::Main(_)));
        assert!(
            matches!(acquire(&p).unwrap(), Role::Forward),
            "同一时刻不许两个主节点"
        );
        drop(first);
        wait_main(&p);
    }

    #[test]
    fn lock_is_held_while_a_forked_child_still_has_the_file_open() {
        // 上面那条时红时绿的来由：fork 出的子进程 exec 之前，主节点的锁跟着它
        let d = TestDir::new("lockfork");
        let p = d.0.join("node.lock");
        let first = acquire(&p).unwrap();
        let (r, w) = {
            let mut fds = [0; 2];
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            (fds[0], fds[1])
        };
        let pid = fork_waiting();
        if pid == 0 {
            // 子进程：等父进程写一个字节再走，期间攥着锁文件那一份
            let mut b = [0u8; 1];
            unsafe {
                libc::close(w);
                libc::read(r, b.as_mut_ptr().cast(), 1);
                libc::_exit(0);
            }
        }
        unsafe { libc::close(r) };
        drop(first);
        assert!(
            matches!(acquire(&p).unwrap(), Role::Forward),
            "子进程还攥着 ⇒ 锁没放"
        );
        unsafe {
            libc::write(w, [1u8].as_ptr().cast(), 1);
            libc::close(w);
            let mut st = 0;
            libc::waitpid(pid, &mut st, 0);
        }
        // 子进程走了：锁放开（同一进程里别的测试 fork 的也可能还攥着一会儿，等它）
        wait_main(&p);
    }

    #[test]
    fn unopenable_lock_is_error() {
        assert!(acquire(Path::new("/nonexistent-dir-dsn/node.lock")).is_err());
    }
}
