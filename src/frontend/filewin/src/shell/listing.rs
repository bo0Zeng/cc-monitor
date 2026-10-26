//! 列目录这件事的共享落点（[`Listing`]：UI 线程读、tokio 那条写）与落地那两个口（只认当前这一趟的结果）。

use super::*;

/// 列目录这件事的**共享落点** —— 一次列目录要写的东西全在这儿。
///
/// 🔴 抽成一个结构是因为它要被**两条线程**看：UI 线程读，tokio 那条写。
/// 散成四个 `Arc` 时「哪几个字段必须一起更新」只写在注释里，没有人守着。
#[derive(Clone)]
pub struct Listing {
    pub rows: Arc<Mutex<Vec<Listed>>>,
    /// 手上那一份（上一次回包的记号 ＋ 排好的那几行，线上那一形）：[`Self::rows`] 由它摊开；重问带它的记号，
    /// 回来只换变了的那几行。🔴 判号与换它在同一把锁里（[`Self::start_held`] 也拿这把）：先发的那一趟晚到，不会盖到后发那一趟的底上。
    pub hand: Arc<Mutex<crate::source::Shown>>,
    /// 列不出来那一句 ＋ 复制详情（[`crate::source::Failed`]：后端拒了 ⇒ 那台写的；通道没走通 ⇒ 窗口写的；这一侧的话 ⇒ 空）。
    pub error: Arc<Mutex<Option<crate::source::Failed>>>,
    /// 🔴 **换目录的序号。** 每次 [`FileWindow::navigate_to`] +1。
    ///
    /// 异步回来的那一份**带着它出发时的号**：号不对就整份丢掉。
    /// 没有它的失效形状是真的会出现的：远端目录 A 慢、B 快 ⇒ 用户点进 A 立刻退回 B，
    /// A 的响应后到，把 B 的内容**盖成 A 的**，而路径栏上写着 B。
    pub epoch: Arc<AtomicU64>,
    /// 还有几趟列目录在飞。0 = 列完了。
    pub inflight: Arc<AtomicU64>,
    // 这里原先还有一格「上一趟走没走成主路」（退路那一句话的落点）。
    // 退路整条拿掉之后（`D11`，理由住 `source.rs` 那一节）那一格没有可说的了，一起摘掉。
    /// 🔴 上一趟**被后端截断了吗**。
    ///
    /// 它必须画出来：「这个目录里就这么多」与「后端只给了前 N 条」
    /// 在屏幕上长得一样，而用户会据此以为某个文件不存在。
    pub truncated: Arc<std::sync::atomic::AtomicBool>,
    /// 隐藏文件切成不显示时，收起来的那几行，各带它在后端那一屏里的位置（不在 [`Self::rows`] 里，下标不受它们影响）。
    pub hidden: Arc<Mutex<Vec<(usize, Listed)>>>,
    /// 隐藏文件显示着吗（缺省 `true`）。一屏落地时按它分。
    pub show_hidden: Arc<std::sync::atomic::AtomicBool>,
    /// 上一趟有几项读不出来（后端照数，没列出）。
    pub unreadable: Arc<AtomicU64>,
    /// 上一趟目录里一共读到几项（截断时那一句要它）。
    pub total: Arc<AtomicU64>,
    /// 上一趟目录打不开：哪一种（后端的码）＋ 系统原话（进「复制详情」）。
    pub open_fail: Arc<Mutex<Option<(crate::source::OpenFail, String)>>>,
    /// 摆着的这一屏是什么时候列到的（UNIX 秒；0 ＝ 还没列到过）。断线条「离线 · 采样 13:40」那个时刻。
    pub landed: Arc<AtomicU64>,
    /// 计划反查：这个目录落在某一片的仓库里 ⇒ 每份文件归哪一格（后端 `plan-files`）；不在 / 还没问到 ⇒ `None`。
    pub plan: Arc<Mutex<Option<crate::plan::PlanDir>>>,
}

impl Default for Listing {
    fn default() -> Self {
        Self {
            rows: Arc::new(Mutex::new(Vec::new())),
            hand: Arc::new(Mutex::new(Default::default())),
            error: Arc::new(Mutex::new(None)),
            epoch: Arc::new(AtomicU64::new(0)),
            inflight: Arc::new(AtomicU64::new(0)),
            truncated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            hidden: Arc::new(Mutex::new(Vec::new())),
            show_hidden: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            unreadable: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
            open_fail: Arc::new(Mutex::new(None)),
            landed: Arc::new(AtomicU64::new(0)),
            plan: Arc::new(Mutex::new(None)),
        }
    }
}

/// 落一趟**经后端**的列目录：号对得上 ⇒ 照回包换手上那一份（[`Listing::hand`]）、摊成一屏，连同截断那几格一起落。
/// 问不成 ⇒ 手上那一份作废（下一趟整份要）。号对不上 ⇒ 整趟丢掉、手上那一份不动。
pub fn store_listed_if_current(
    l: &Listing,
    mine: u64,
    r: Result<cmd_wire::files::LsReply, crate::source::Failed>,
) -> bool {
    // 判号与换手上那一份在同一把锁里（[`Listing::start_held`] 拿同一把）。
    let mut hand = l.hand.lock().unwrap();
    if l.epoch.load(Ordering::SeqCst) != mine {
        drop(hand);
        return store_if_current(l, mine, Ok(Vec::new()));
    }
    match r {
        Ok(p) => {
            hand.apply(p);
            let (rows, cut) = crate::source::screen_of(&hand);
            let kept = store_if_current(l, mine, Ok(rows));
            if kept {
                l.truncated.store(cut.truncated, Ordering::SeqCst);
                l.unreadable.store(cut.unreadable, Ordering::SeqCst);
                l.total.store(cut.total, Ordering::SeqCst);
                *l.open_fail.lock().unwrap() = None;
            }
            kept
        }
        Err(e) => {
            *hand = Default::default();
            store_if_current(l, mine, Err(e))
        }
    }
}

/// 把一趟列目录的结果落进 [`Listing`] —— **号对不上就丢掉**。
///
/// 回值 = 真的落盘了。**自由函数**（不吃 `FileWindow`）⇒ 不用开窗、不用网络就判得动，
/// 而它正是生产那条路上唯一写 `rows` 的地方（见 [`Listing::start`]）。
pub fn store_if_current(
    l: &Listing,
    mine: u64,
    r: Result<Vec<Listed>, crate::source::Failed>,
) -> bool {
    // ⚠ 先减在飞数，再判号 —— 不管这一份要不要，它都已经落地了。
    l.inflight.fetch_sub(1, Ordering::SeqCst);
    if l.epoch.load(Ordering::SeqCst) != mine {
        return false;
    }
    match r {
        Ok(v) => {
            let (shown, hidden) = if l.show_hidden.load(Ordering::SeqCst) {
                (v, Vec::new())
            } else {
                split_hidden(v)
            };
            *l.rows.lock().unwrap() = shown;
            *l.hidden.lock().unwrap() = hidden;
            *l.error.lock().unwrap() = None;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            l.landed.store(now, Ordering::SeqCst);
        }
        Err(e) => {
            *l.rows.lock().unwrap() = Vec::new();
            l.hidden.lock().unwrap().clear();
            *l.error.lock().unwrap() = Some(e.into());
        }
    }
    true
}

/// 一屏分成显示的与收起来的（隐藏文件）；收起来的记下它在那一屏里的位置，插回时照原样。
pub(super) fn split_hidden(v: Vec<Listed>) -> (Vec<Listed>, Vec<(usize, Listed)>) {
    let mut shown = Vec::with_capacity(v.len());
    let mut gone = Vec::new();
    for (i, r) in v.into_iter().enumerate() {
        if crate::kind::is_hidden(&r.name) {
            gone.push((i, r));
        } else {
            shown.push(r);
        }
    }
    (shown, gone)
}

impl Listing {
    /// 开一趟。回的是这一趟的号：每趟一个新号，只收**最后发出的那一趟**
    /// （同一个目录两趟在飞，后发的先回、先发的后回 ⇒ 先发的那份丢掉，不把刚删的文件又摆回来）。
    pub fn start(&self) -> u64 {
        self.inflight.fetch_add(1, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 开一趟带记号的列目录：这一趟的号 ＋ 此刻手上那一份的记号（同一把锁里取：落地那一侧判号与换也在这把锁里）。
    pub fn start_held(&self) -> (u64, Option<cmd_wire::sync::Held>) {
        let hand = self.hand.lock().unwrap();
        (self.start(), hand.held.clone())
    }

    /// 收下一份已经列好的（开窗前那一屏）：手上那一份换成它、摊成一屏。
    pub fn adopt(&self, first: crate::source::Shown) {
        let mine = self.start();
        let (rows, cut) = crate::source::screen_of(&first);
        *self.hand.lock().unwrap() = first;
        store_if_current(self, mine, Ok(rows));
        self.truncated.store(cut.truncated, Ordering::SeqCst);
        self.unreadable.store(cut.unreadable, Ordering::SeqCst);
        self.total.store(cut.total, Ordering::SeqCst);
    }

    /// 换目录：号 +1（在飞的那些从此全部作废）、内容清空。
    ///
    /// ⚠ **清空是刻意的**：留着上一个目录的行、只改路径栏，是最贵的那种假象
    /// —— 用户会对着 B 的路径删 A 的文件。
    pub fn invalidate(&self) {
        let mut hand = self.hand.lock().unwrap();
        self.epoch.fetch_add(1, Ordering::SeqCst);
        *hand = Default::default();
        drop(hand);
        self.rows.lock().unwrap().clear();
        self.hidden.lock().unwrap().clear();
        *self.error.lock().unwrap() = None;
        *self.plan.lock().unwrap() = None;
    }

    /// 切隐藏文件显不显示：显示 ⇒ 收起来的那几行插回它们在后端那一屏里的位置；不显示 ⇒ 挪到一边。
    pub fn set_show_hidden(&self, on: bool) {
        self.show_hidden.store(on, Ordering::SeqCst);
        let mut rows = self.rows.lock().unwrap();
        let mut hidden = self.hidden.lock().unwrap();
        if on {
            for (at, r) in std::mem::take(&mut *hidden) {
                let at = at.min(rows.len());
                rows.insert(at, r);
            }
        } else {
            let (shown, gone) = split_hidden(std::mem::take(&mut *rows));
            *rows = shown;
            *hidden = gone;
        }
    }

    pub fn is_loading(&self) -> bool {
        self.inflight.load(Ordering::SeqCst) > 0
    }
}
