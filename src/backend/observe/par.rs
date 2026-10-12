//! observe 内部把一串互不相干的活分给几条线程做的那一套（历史清单按记录目录扫 · 全文搜索一批批重读常驻装不下的那几份 · 记录预读的常驻线程池）。
//! 住 observe：两个调用点同属 observe，不够 `common/` 的「≥2 个上层用」门槛；不住 `observe/fs.rs`：它不碰文件。

/// 〔perfC〕一串互不相干的活分给至多 [`PAR_WORKERS`] 条线程做（每条从同一个计数器领下一件），结果**按原序**交回 ——
/// 与单线程逐件做逐字相同。历史清单按记录目录扫、全文搜索一批批带常驻装不下的那几份，同这一套（只此一处）。
/// 某一件 panic ⇒ 原样抛到调用方（不悄悄少一件）。
pub(crate) fn par_in_order<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let workers = here().min(items.len());
    if workers <= 1 {
        return items.into_iter().map(f).collect();
    }
    let slots: Vec<std::sync::Mutex<Option<T>>> = items
        .into_iter()
        .map(|t| std::sync::Mutex::new(Some(t)))
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(slot) = slots.get(i) else {
                            break;
                        };
                        let item = slot.lock().ok().and_then(|mut g| g.take());
                        if let Some(item) = item {
                            mine.push((i, f(item)));
                        }
                    }
                    mine
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)))
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// 这台机器上起几条（[`par_workers`] 按此刻可用的并行数算；问机器只此一处）。
fn here() -> usize {
    par_workers(std::thread::available_parallelism().ok().map(usize::from))
}

/// [`par_in_order`] 最多几条线程。
const PAR_WORKERS: usize = 8;

/// 〔perfC4〕[`par_in_order`] 起几条线程（只此一处算）：min([`PAR_WORKERS`], 这台机器可用的并行数)，至少一条；
/// 问不出可用数 ⇒ 一条。远端可能是一两核的小机器 ⇒ 不按上限起满。
pub(crate) fn par_workers(available: Option<usize>) -> usize {
    available.unwrap_or(1).clamp(1, PAR_WORKERS)
}

// ─── 常驻的那一种：有上限的线程池（read8a：活会话的记录预读各会话并行，宣告不等它）───
//
// 线程只阻塞在任务队列上（`recv()`，无期限）：没有任务就一直睡在内核里，不定时、不自己醒（`no_timer_guard` 登记了这一处）。
// 池子丢了 ⇒ 队列的发送端没了 ⇒ 各线程的 `recv()` 回错、退出。线程数照 [`par_workers`]（只此一处算）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

/// 一个有上限的线程池：`n` 条线程，各自从同一条队列里取任务跑。
pub(crate) struct Pool<J> {
    tx: mpsc::Sender<J>,
    load: Arc<Load>,
    n: usize,
}

/// 此刻在跑几份 · 同时在跑最多到过几份（判据读它：不许超过线程数）。
#[derive(Default)]
pub(crate) struct Load {
    running: AtomicUsize,
    high: AtomicUsize,
}

impl Load {
    pub(crate) fn running(&self) -> usize {
        self.running.load(Ordering::SeqCst)
    }

    pub(crate) fn high(&self) -> usize {
        self.high.load(Ordering::SeqCst)
    }
}

impl<J: Send + 'static> Pool<J> {
    /// 起 [`par_workers`] 条线程（这台机器的并行数，至多 [`PAR_WORKERS`]），每条名叫 `name`；`work` 在线程里跑每一份任务。
    pub(crate) fn new(name: &str, work: impl Fn(J) + Send + Sync + 'static) -> Self {
        let n = here();
        let (tx, rx) = mpsc::channel::<J>();
        let rx = Arc::new(Mutex::new(rx));
        let work = Arc::new(work);
        let load = Arc::new(Load::default());
        for _ in 0..n {
            let (rx, work, load) = (rx.clone(), work.clone(), load.clone());
            std::thread::Builder::new()
                .name(name.to_string())
                .spawn(move || worker(&rx, &*work, &load))
                .expect("spawn pool thread");
        }
        Pool { tx, load, n }
    }

    /// 排一份任务（池子的线程都忙 ⇒ 排着，先来先跑）。
    pub(crate) fn submit(&self, job: J) {
        let _ = self.tx.send(job);
    }

    /// 几条线程（同时在跑的上限）。
    pub(crate) fn size(&self) -> usize {
        self.n
    }

    pub(crate) fn load(&self) -> Arc<Load> {
        self.load.clone()
    }
}

/// 一条线程：取一份、跑一份；队列关了就走。
fn worker<J>(rx: &Mutex<mpsc::Receiver<J>>, work: &dyn Fn(J), load: &Load) {
    loop {
        let job = {
            let q = rx.lock().unwrap_or_else(|e| e.into_inner());
            q.recv()
        };
        let Ok(job) = job else {
            return;
        };
        let now = load.running.fetch_add(1, Ordering::SeqCst) + 1;
        load.high.fetch_max(now, Ordering::SeqCst);
        work(job);
        load.running.fetch_sub(1, Ordering::SeqCst);
    }
}
