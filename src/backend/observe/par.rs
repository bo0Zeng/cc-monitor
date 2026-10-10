//! observe 内部把一串互不相干的活分给几条线程做的那一套（历史清单按记录目录扫 · 全文搜索一批批重读常驻装不下的那几份）。
//! 住 observe：两个调用点同属 observe，不够 `common/` 的「≥2 个上层用」门槛；不住 `observe/fs.rs`：它不碰文件。

/// 〔perfC〕一串互不相干的活分给至多 [`PAR_WORKERS`] 条线程做（每条从同一个计数器领下一件），结果**按原序**交回 ——
/// 与单线程逐件做逐字相同。历史清单按记录目录扫、全文搜索一批批带常驻装不下的那几份，同这一套（只此一处）。
/// 某一件 panic ⇒ 原样抛到调用方（不悄悄少一件）。
pub(crate) fn par_in_order<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let workers =
        par_workers(std::thread::available_parallelism().ok().map(usize::from)).min(items.len());
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

/// [`par_in_order`] 最多几条线程。
const PAR_WORKERS: usize = 8;

/// 〔perfC4〕[`par_in_order`] 起几条线程（只此一处算）：min([`PAR_WORKERS`], 这台机器可用的并行数)，至少一条；
/// 问不出可用数 ⇒ 一条。远端可能是一两核的小机器 ⇒ 不按上限起满。
pub(crate) fn par_workers(available: Option<usize>) -> usize {
    available.unwrap_or(1).clamp(1, PAR_WORKERS)
}
