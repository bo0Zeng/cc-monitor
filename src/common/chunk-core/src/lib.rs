//! **分块传的循环**：桌面文件窗口与手机嵌的 Rust 核心同一份（两端各写一份 ⇒ 块多大 · 摘要怎么算 · 一块失败之后怎样各走各的）。
//!
//! - 上传 [`stage`]：本机那份逐块读满（每块 [`CHUNK`] 字节）→ 依次 `files-stage-chunk` 进那台的暂存区（同一个键）→ 边送边算整份
//!   SHA-256 → 交回 [`Staged`]（块数 · 字节数 · 摘要），提交（`files-commit-upload`，同名照 `on_exists`）由调用方交。
//! - 下载 [`pull`]：`files-read-chunk` 从 0 起逐块读回（按字节寻址），每块交调用方的 [`Sink`]，读到头为止；交回总字节。
//! - 拉进内存 [`pull_capped`]：同 [`pull`]，攒成一份交回；攒过调用方给的上限就停，交 [`TooLarge`]（读了多少 · 上限）。
//! - 聊天附件草稿那一圈 [`attach`]：问收不收 → 送块（宿主照 [`stage`] 走）→ 提交，结局表 [`attach::settle`]；两端各接各的线。
//! - 问那台的那一下是 [`Ask`]（桌面经通道、手机经常驻流）；撤 · 进度 · 本机读不动怎么说都由调用方给，这里不写字。

use cmd_wire::files::{to_hex, ChunkBytes, FilesStageChunkReq, ReadChunkReq, WirePath};
use cmd_wire::transfer::Staged;
use cmd_wire::Call;
use sha2::Digest as _;
use std::future::Future;

/// 一块多少原始字节（== 后端 `files::READ_CHUNK_MAX_BYTES`；b16 翻倍 ＋ 信封仍远小于那台入方向一行的上限）。
pub const CHUNK: usize = 256 * 1024;

/// 问那台一条文件命令：成了 ⇒ 那条命令的回包；没成 ⇒ 调用方那一形的失败。
pub trait Ask {
    type Err;
    fn ask<C: Call + Sync>(&self, req: &C) -> impl Future<Output = Result<C::Reply, Self::Err>>;
}

/// 读回来的一块交给谁（写本机文件 · 再进本机的暂存区 …）。
pub trait Sink {
    type Err;
    fn put(&mut self, bytes: Vec<u8>) -> impl Future<Output = Result<(), Self::Err>>;
}

/// 撤与进度那两口（每一块之前问一次撤；每送 / 收完一块报一次进度）。
pub trait Watch {
    /// 撤了没有。
    fn stopped(&self) -> bool;
    /// 已经过去的字节 · 总数（不知道 ⇒ `None`）。
    fn moved(&mut self, done: u64, total: Option<u64>);
}

/// 一块一块往那台的暂存区里送（同一个键、块号从 0 连着来），边送边算整份摘要。
pub struct Stager {
    key: String,
    sha: sha2::Sha256,
    chunks: u64,
    bytes: u64,
}

impl Stager {
    pub fn new(key: String) -> Self {
        Stager {
            key,
            sha: sha2::Sha256::new(),
            chunks: 0,
            bytes: 0,
        }
    }

    /// 送一块（空块不送）。
    pub async fn put<A: Ask>(&mut self, ask: &A, bytes: Vec<u8>) -> Result<(), A::Err> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.sha.update(&bytes);
        let n = bytes.len() as u64;
        let req = FilesStageChunkReq {
            key: self.key.clone(),
            seq: self.chunks,
            content: ChunkBytes(bytes),
        };
        ask.ask(&req).await?;
        self.chunks += 1;
        self.bytes += n;
        Ok(())
    }

    /// 已送的字节。
    pub fn sent(&self) -> u64 {
        self.bytes
    }

    pub fn finish(self) -> Staged {
        Staged {
            key: self.key,
            chunks: self.chunks,
            bytes: self.bytes,
            sha256: to_hex(&self.sha.finalize()),
        }
    }
}

/// 一份暂存件的键（那台只收 32 位小写十六进制）：由调用方现造的 16 个随机字节写成。
pub fn key_of(random: [u8; 16]) -> String {
    to_hex(&random)
}

/// 上传那一半：`src` 逐块读满送进 `key` 那一份暂存件。撤了 ⇒ `stop()`（已送的块交那台的孤儿扫）；本机读不动 ⇒ `local(e)`。
pub async fn stage<A: Ask, R: std::io::Read>(
    ask: &A,
    key: String,
    mut src: R,
    total: Option<u64>,
    watch: &mut impl Watch,
    stop: impl Fn() -> A::Err,
    local: impl Fn(std::io::Error) -> A::Err,
) -> Result<Staged, A::Err> {
    let mut stager = Stager::new(key);
    watch.moved(0, total);
    loop {
        if watch.stopped() {
            return Err(stop());
        }
        let block = fill(&mut src).map_err(&local)?;
        if block.is_empty() {
            break;
        }
        let last = block.len() < CHUNK;
        stager.put(ask, block).await?;
        watch.moved(stager.sent(), total);
        if last {
            break;
        }
    }
    Ok(stager.finish())
}

/// 下载那一半：`path` 从 0 起逐块读回、每块交 `sink`，读到头为止；回总字节。撤了 ⇒ `stop()`。
pub async fn pull<A: Ask, S: Sink<Err = A::Err>>(
    ask: &A,
    path: WirePath,
    sink: &mut S,
    watch: &mut impl Watch,
    stop: impl Fn() -> A::Err,
) -> Result<u64, A::Err> {
    let mut off = 0u64;
    loop {
        if watch.stopped() {
            return Err(stop());
        }
        let d = ask
            .ask(&ReadChunkReq {
                path: path.clone(),
                offset: off,
                len: CHUNK as u64,
            })
            .await?;
        let body = d.content.0;
        let empty = body.is_empty();
        off += body.len() as u64;
        if !empty {
            sink.put(body).await?;
        }
        watch.moved(off, Some(d.size));
        if d.eof || empty {
            return Ok(off);
        }
    }
}

/// 拉进内存攒过了上限：已读回的字节 · 调用方给的上限（那一句由调用方照它写）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooLarge {
    pub read: u64,
    pub cap: u64,
}

/// 读回来的一块攒进内存；攒过 `cap` ⇒ `over`，不读下一块。
struct ToMemory<F> {
    bytes: Vec<u8>,
    cap: u64,
    over: F,
}

impl<E, F: Fn(TooLarge) -> E> Sink for ToMemory<F> {
    type Err = E;
    async fn put(&mut self, bytes: Vec<u8>) -> Result<(), E> {
        self.bytes.extend_from_slice(&bytes);
        let read = self.bytes.len() as u64;
        if read > self.cap {
            return Err((self.over)(TooLarge {
                read,
                cap: self.cap,
            }));
        }
        Ok(())
    }
}

/// 拉进内存那一半（读一张图 · 一小份）：同 [`pull`] 逐块读回，整份交回；攒过 `cap` 就停 ⇒ `over(TooLarge)`。撤了 ⇒ `stop()`。
pub async fn pull_capped<A: Ask>(
    ask: &A,
    path: WirePath,
    cap: u64,
    watch: &mut impl Watch,
    stop: impl Fn() -> A::Err,
    over: impl Fn(TooLarge) -> A::Err,
) -> Result<Vec<u8>, A::Err> {
    let mut sink = ToMemory {
        bytes: Vec::new(),
        cap,
        over,
    };
    pull(ask, path, &mut sink, watch, stop).await?;
    Ok(sink.bytes)
}

/// 读满一块（或读到头）。
fn fill<R: std::io::Read>(src: &mut R) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; CHUNK];
    let mut n = 0;
    while n < CHUNK {
        match src.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    buf.truncate(n);
    Ok(buf)
}

/// **聊天附件草稿那一圈**：问收不收 → 逐块送进那台暂存区 → 提交 ⇒ 编号；断了回排队、等连上；那台不收的标上。
/// 桌面（本机后端替界面走）与手机（嵌的核心）同一份；问那台的几口经 [`attach::Upload`] 由宿主接，这里不写字。
pub mod attach {
    use cmd_wire::attachment::{
        AttachCardPhase, AttachItem, AttachLocal, AttachVerdict, AttachWant, AttachWantCard,
    };
    use cmd_wire::transfer::Staged;
    use std::future::Future;
    use std::sync::{Mutex, MutexGuard};

    /// 草稿里的一张卡：键（宿主编，这一排里不重）· 名字 · 字节 · 是不是目录 · 此刻在哪一步 · 落好的编号 · 已送字节。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Card {
        pub key: String,
        pub name: String,
        pub bytes: u64,
        pub dir: bool,
        pub phase: AttachCardPhase,
        /// 落好了 ⇒ 那台回的编号（送那一句时交它）。
        pub id: Option<String>,
        /// 传输中 ⇒ 已送字节。
        pub done: u64,
    }

    /// 一步没成是怎么没成的（宿主把它那一形的失败分进这几档）。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Ended {
        /// 那台不收这一份（问收不收的答）：那台写好的一句。
        Refused { said: String },
        /// 断了：断开 · 超时 · 流撤了同一档。
        Cut,
        /// 那台的错（送块 · 提交 · 问收不收没成）：那一句。
        Failed { said: String },
        /// 本机那一份读不动（副本没了 · 开不了）。
        Unreadable,
        /// 用户撤了这一份（✕）。
        Stopped,
    }

    /// 一步的结局：这张卡落到哪一步（`None` ⇒ 卡已撤、不落）· 要不要停下等连上。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Settled {
        pub phase: Option<AttachCardPhase>,
        pub wait: bool,
    }

    /// **结局表**：同一种没成，不论在哪一步、哪一端，落同一结局。
    pub fn settle(end: &Ended) -> Settled {
        let (phase, wait) = match end {
            Ended::Refused { said } => {
                (Some(AttachCardPhase::Refused { said: said.clone() }), false)
            }
            Ended::Cut => (Some(AttachCardPhase::Queued), true),
            Ended::Failed { said } => (Some(AttachCardPhase::Failed { said: said.clone() }), false),
            Ended::Unreadable => (Some(AttachCardPhase::Lost), false),
            Ended::Stopped => (None, false),
        };
        Settled { phase, wait }
    }

    /// 一条会话那一排：卡（按先后）· 断着等连上没有 · 在不在跑 · 本机这边的错。
    #[derive(Debug, Default)]
    pub struct Tray {
        cards: Vec<Card>,
        cut: bool,
        running: bool,
        local: Option<AttachLocal>,
    }

    impl Tray {
        pub fn cards(&self) -> &[Card] {
            &self.cards
        }

        /// 断着、等连上。
        pub fn cut(&self) -> bool {
            self.cut
        }

        /// 加一份（排队）；本机这边的错收掉。
        pub fn add(&mut self, key: String, name: String, bytes: u64, dir: bool) {
            self.local = None;
            self.cards.push(Card {
                key,
                name,
                bytes,
                dir,
                phase: AttachCardPhase::Queued,
                id: None,
                done: 0,
            });
        }

        /// 照本机记着的摆回一份（进程没了再开）：带编号 ⇒ 已落好、不重传。
        pub fn restore(&mut self, key: String, name: String, bytes: u64, id: Option<String>) {
            let phase = if id.is_some() {
                AttachCardPhase::Landed
            } else {
                AttachCardPhase::Queued
            };
            self.cards.push(Card {
                key,
                name,
                bytes,
                dir: false,
                phase,
                id,
                done: 0,
            });
        }

        /// ✕ 一份：交回撤掉的那张（在传 ⇒ 宿主撤那一趟；落好了 ⇒ 宿主让那台删掉那个编号）。
        pub fn remove(&mut self, key: &str) -> Option<Card> {
            self.local = None;
            let i = self.cards.iter().position(|c| c.key == key)?;
            Some(self.cards.remove(i))
        }

        /// ［重试］：失败 · 本机副本开不了的回排队；断着的也再试一次。
        pub fn retry(&mut self) {
            self.cut = false;
            for c in &mut self.cards {
                if matches!(
                    c.phase,
                    AttachCardPhase::Failed { .. } | AttachCardPhase::Lost
                ) {
                    c.phase = AttachCardPhase::Queued;
                }
            }
        }

        /// 连上了（流通了 · 换了一条流）：排队的接着传。
        pub fn up(&mut self) {
            self.cut = false;
        }

        /// 本机这边的错（存不下 · 没有相机）：下一次加 · 撤就收掉。
        pub fn local(&mut self, why: Option<AttachLocal>) {
            self.local = why;
        }

        /// 全落好了（没有卡也算）。
        pub fn ready(&self) -> bool {
            self.cards
                .iter()
                .all(|c| c.phase == AttachCardPhase::Landed)
        }

        /// 不看这一排了（屏关了）：卡全撤掉。在跑的那一圈下一步就收场，在传的那一份落好了照「卡已撤」让那台删掉。
        pub fn close(&mut self) {
            self.cards.clear();
        }

        /// 发出去那一下：全落好了 ⇒ 编号（按先后）、那一排清空；还有没落好的 ⇒ `None`。
        pub fn take(&mut self) -> Option<Vec<String>> {
            if !self.ready() {
                return None;
            }
            let ids = self.cards.drain(..).filter_map(|c| c.id).collect();
            Some(ids)
        }

        /// 交 face-core 写字的那一份；`offline` 是那台离线那一句，只在有排队的、又断着时带上。
        pub fn want(&self, offline: Option<&str>) -> AttachWant {
            let queued = self
                .cards
                .iter()
                .any(|c| c.phase == AttachCardPhase::Queued);
            AttachWant {
                cards: self
                    .cards
                    .iter()
                    .map(|c| AttachWantCard {
                        name: c.name.clone(),
                        phase: c.phase.clone(),
                    })
                    .collect(),
                offline: offline.filter(|_| queued && self.cut).map(str::to_string),
                local: self.local,
            }
        }

        fn card(&mut self, key: &str) -> Option<&mut Card> {
            self.cards.iter_mut().find(|c| c.key == key)
        }

        /// 照结局表落一张卡；结局要等连上 ⇒ 记断着。
        fn settle(&mut self, key: &str, end: &Ended) {
            let s = settle(end);
            if s.wait {
                self.cut = true;
            }
            if let (Some(phase), Some(c)) = (s.phase, self.card(key)) {
                c.phase = phase;
                c.done = 0;
            }
        }
    }

    /// 问那台的几口（宿主接：桌面经那台常驻流、手机经这台常驻流）＋ 那一排变了交给谁。
    pub trait Upload {
        /// 问收不收：答与 `items` 同序同数（对不上由宿主当那台的错交 [`Ended::Failed`]）。
        fn check(
            &self,
            items: &[AttachItem],
        ) -> impl Future<Output = Result<Vec<AttachVerdict>, Ended>> + Send;
        /// 那一份逐块送进那台暂存区；`moved` 逐块报已送字节。撤了（宿主那一份撤的旗）⇒ [`Ended::Stopped`]。
        fn stage(
            &self,
            card: &Card,
            moved: &mut (dyn FnMut(u64) + Send),
        ) -> impl Future<Output = Result<Staged, Ended>> + Send;
        /// 提交 ⇒ 那台回的编号。
        fn commit(
            &self,
            card: &Card,
            staged: &Staged,
        ) -> impl Future<Output = Result<String, Ended>> + Send;
        /// 落好了才发现卡已撤：让那台删掉那个编号（删不掉交那台的孤儿扫）。
        fn drop_landed(&self, id: &str) -> impl Future<Output = ()> + Send;
        /// 那一排变了。
        fn changed(&self, tray: &Tray);
    }

    fn lock(t: &Mutex<Tray>) -> MutexGuard<'_, Tray> {
        t.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 这一圈：每传一份前整排（不收的除外）问一次收不收 → 排在最前的那一份送块 → 提交；一份一份来，断了停下等 [`Tray::up`]。
    /// 已经有一圈在跑 ⇒ 当场回（那一圈每一步都重读名单，新加的自然排上）。宿主在加 · 重试 · 连上之后各叫一次。
    pub async fn pump<U: Upload + Sync>(tray: &Mutex<Tray>, up: &U) {
        {
            let mut t = lock(tray);
            if t.running {
                return;
            }
            t.running = true;
        }
        loop {
            // 整排问一次（没有排队的 · 断着 ⇒ 这一圈收场；收场与「还有没有活」同一把锁里判，不漏后来加的）。
            let asked: Vec<Card> = {
                let mut t = lock(tray);
                let asked: Vec<Card> = t
                    .cards
                    .iter()
                    .filter(|c| !matches!(c.phase, AttachCardPhase::Refused { .. }))
                    .cloned()
                    .collect();
                if t.cut || !asked.iter().any(|c| c.phase == AttachCardPhase::Queued) {
                    t.running = false;
                    return;
                }
                asked
            };
            let items: Vec<AttachItem> = asked
                .iter()
                .map(|c| AttachItem {
                    name: c.name.clone(),
                    bytes: c.bytes,
                    dir: c.dir,
                })
                .collect();
            let got = up.check(&items).await;
            {
                let mut t = lock(tray);
                match got {
                    Ok(verdicts) if verdicts.len() == asked.len() => {
                        // 落好了的只为让那台照总量算，答落不到它身上（编号已在那台）。
                        for (c, v) in asked.iter().zip(verdicts) {
                            if !v.ok && c.phase != AttachCardPhase::Landed {
                                let said = v.said.unwrap_or_default();
                                t.settle(&c.key, &Ended::Refused { said });
                            }
                        }
                    }
                    other => {
                        let end = other.err().unwrap_or(Ended::Failed {
                            said: String::new(),
                        });
                        for c in asked.iter().filter(|c| c.phase == AttachCardPhase::Queued) {
                            t.settle(&c.key, &end);
                        }
                    }
                }
                up.changed(&t);
            }
            // 排在最前的那一份开传。
            let card = {
                let mut t = lock(tray);
                if t.cut {
                    continue;
                }
                let Some(c) = t
                    .cards
                    .iter_mut()
                    .find(|c| c.phase == AttachCardPhase::Queued)
                else {
                    continue;
                };
                c.phase = AttachCardPhase::Moving;
                c.done = 0;
                let card = c.clone();
                up.changed(&t);
                card
            };
            let mut moved = |done: u64| {
                let mut t = lock(tray);
                if let Some(c) = t.card(&card.key) {
                    c.done = done;
                    up.changed(&t);
                }
            };
            let got = match up.stage(&card, &mut moved).await {
                Ok(staged) => up.commit(&card, &staged).await,
                Err(e) => Err(e),
            };
            let orphan = {
                let mut t = lock(tray);
                let orphan = match (&got, t.card(&card.key)) {
                    (Ok(id), None) => Some(id.clone()),
                    (_, None) => None,
                    (Ok(id), Some(c)) => {
                        c.phase = AttachCardPhase::Landed;
                        c.id = Some(id.clone());
                        c.done = 0;
                        None
                    }
                    (Err(e), Some(_)) => {
                        t.settle(&card.key, e);
                        None
                    }
                };
                up.changed(&t);
                orphan
            };
            if let Some(id) = orphan {
                up.drop_landed(&id).await;
            }
        }
    }
}

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

#[cfg(test)]
#[path = "../../../../tests/common/chunk-core/chunk_core_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/common/chunk-core/attach_tests.rs"]
mod attach_tests;
