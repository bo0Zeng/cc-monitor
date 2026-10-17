//! **分块传**在手机这一侧的宿主：循环是 `chunk_core` 那一份（桌面文件窗口同一份），这里只接三根线 ——
//! 问那台（常驻流那一条，每块一个期限）· 本机那一份（Kotlin 交来的文件描述符，这里用完就关）·
//! 往 Kotlin 交的事：进度 `moved {call, done, total}`，结局走一问一答那一种 `reply`（同 [`resident::reply_json`] 那一行，编号同一族）。
//!
//! 上传这里只把块送进那台的暂存区、交回 `{key, chunks, bytes, sha256}`；提交（同名照 `on_exists`）由 Kotlin 一问（同桌面）。

use crate::resident::{self, OnEvent};
use chunk_core::{Ask, Sink, Watch};
use cmd_wire::files::{Call, WirePath};
use resident_client::client::CallError;
use std::collections::HashMap;
use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::Notify;

/// 经这台的常驻流问那台：每一问一个期限；没成 ⇒ 交 Kotlin 的那一行（同一问一答的结局）。
struct Over {
    h: i64,
    id: i64,
    within: Duration,
}

impl Ask for Over {
    type Fail = String;
    async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, String> {
        let Some(f) = resident::feed(self.h) else {
            return Err(cancelled(self.id));
        };
        let deadline = tokio::time::Instant::now() + self.within;
        let args = serde_json::to_value(req).unwrap_or_default();
        let got = match f.client_by(deadline).await {
            Some(c) => c.call_until(C::NAME, args, deadline).await,
            None => Err(CallError::Disconnected),
        };
        typed::<C>(self.id, f.machine(), got)
    }
}

/// 一问的结局 ⇒ 那条命令的回包；没成 · 形状对不上 ⇒ 交 Kotlin 的那一行。
pub fn typed<C: Call>(
    id: i64,
    machine: &str,
    got: Result<Option<serde_json::Value>, CallError>,
) -> Result<C::Reply, String> {
    match got {
        Ok(v) => serde_json::from_value(v.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            serde_json::json!({ "call": id, "unreadable": true, "detail": format!("{}: {e}", C::NAME) })
                .to_string()
        }),
        Err(e) => Err(resident::reply_json(id, machine, C::NAME, Err(e))),
    }
}

/// 撤一趟（［停］：Kotlin 那一侧不等了）：在问的那一块当场丢下，之后本机那一份一个字节都不再写。
#[derive(Default)]
pub struct Halt {
    hit: AtomicBool,
    now: Notify,
}

impl Halt {
    pub fn set(&self) {
        self.hit.store(true, Ordering::SeqCst);
        self.now.notify_waiters();
    }

    pub fn is(&self) -> bool {
        self.hit.load(Ordering::SeqCst)
    }
}

/// 照撤的一问：撤了 ⇒ 不问、在问的不等、回话到了也不交（那一块不写）。
pub struct Halting<A> {
    pub ask: A,
    pub halt: Arc<Halt>,
    pub id: i64,
}

impl<A: Ask<Fail = String>> Ask for Halting<A> {
    type Fail = String;
    async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, String> {
        let now = self.halt.now.notified();
        tokio::pin!(now);
        now.as_mut().enable();
        if self.halt.is() {
            return Err(cancelled(self.id));
        }
        let got = tokio::select! {
            biased;
            got = self.ask.ask(req) => got,
            () = now => return Err(cancelled(self.id)),
        };
        if self.halt.is() {
            return Err(cancelled(self.id));
        }
        got
    }
}

/// 在走的那几趟，按问答编号（［停］照它撤）。
fn halts() -> &'static Mutex<HashMap<i64, Arc<Halt>>> {
    static H: OnceLock<Mutex<HashMap<i64, Arc<Halt>>>> = OnceLock::new();
    H.get_or_init(Default::default)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 撤问答编号 `id` 那一趟；不在走 ⇒ `false`。
pub fn halt(id: i64) -> bool {
    lock(halts()).get(&id).map(|h| h.set()).is_some()
}

/// 不看这台了（流收了）⇒ 下一块不送。
fn cancelled(id: i64) -> String {
    serde_json::json!({ "call": id, "cancelled": true }).to_string()
}

/// 本机那一份读 / 写不动：那一句 ＋ 系统原话进详情。
fn local(id: i64, said: String, e: &std::io::Error) -> String {
    serde_json::json!({ "call": id, "ok": false, "code": "io_failed", "message": said, "detail": e.to_string() })
        .to_string()
}

/// 撤与进度：流收了 ⇒ 撤；每一块报一次 `moved`。
pub struct Moves {
    pub h: i64,
    pub id: i64,
    pub on_event: OnEvent,
}

impl Watch for Moves {
    fn stopped(&self) -> bool {
        resident::feed(self.h).is_none()
    }
    fn moved(&mut self, done: u64, total: Option<u64>) {
        (self.on_event)(
            self.h,
            "moved",
            serde_json::json!({ "call": self.id, "done": done, "total": total }).to_string(),
        );
    }
}

/// 普通文件 ⇒ 多大；管道之类 ⇒ 不知道。
fn size_of(file: &File) -> Option<u64> {
    file.metadata()
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

/// 上传那一半：`src`（总数不知道 ⇒ `None`）逐块送进那台的暂存区（新键）⇒ `{call, ok, data: {key, chunks, bytes, sha256}}` 或没成那一行。
pub async fn stage_with<A: Ask<Fail = String>>(
    ask: &A,
    id: i64,
    key: String,
    (src, total): (impl std::io::Read, Option<u64>),
    name: &str,
    watch: &mut impl Watch,
) -> String {
    let said = || copy_core::copy_text("mobFiles.local.readFailed", &[("name", name)]);
    match chunk_core::stage(
        ask,
        key,
        src,
        total,
        watch,
        || cancelled(id),
        |e| local(id, said(), &e),
    )
    .await
    {
        Ok(s) => serde_json::json!({
            "call": id, "ok": true,
            "data": { "key": s.key, "chunks": s.chunks, "bytes": s.bytes, "sha256": s.sha256 },
        })
        .to_string(),
        Err(line) => line,
    }
}

/// 读回来的一块写进本机那一份。
struct ToFile<'a, W> {
    file: W,
    id: i64,
    shown: &'a str,
}

impl<W: std::io::Write> Sink for ToFile<'_, W> {
    type Fail = String;
    async fn put(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        self.file.write_all(&bytes).map_err(|e| {
            local(
                self.id,
                copy_core::copy_text("mobFiles.local.writeFailed", &[("path", self.shown)]),
                &e,
            )
        })
    }
}

/// 下载那一半：`path` 逐块读回写进 `file` ⇒ `{call, ok, data: {bytes}}` 或没成那一行（`shown` 是本机落点，句子里用）。
pub async fn pull_with<A: Ask<Fail = String>>(
    ask: &A,
    id: i64,
    path: WirePath,
    file: impl std::io::Write,
    shown: &str,
    watch: &mut impl Watch,
) -> String {
    let mut sink = ToFile { file, id, shown };
    match chunk_core::pull(ask, path, &mut sink, watch, || cancelled(id)).await {
        Ok(n) => serde_json::json!({ "call": id, "ok": true, "data": { "bytes": n } }).to_string(),
        Err(line) => line,
    }
}

/// 起一趟：编号与撤的那一口登记上，`run` 在后台跑；跑完 ⇒ 摘掉，结局那一行没撤才交（撤了 Kotlin 那一侧不在等）。
fn launch<F: std::future::Future<Output = String> + Send + 'static>(
    h: i64,
    within_ms: u64,
    on_event: OnEvent,
    run: impl FnOnce(Halting<Over>, Moves) -> F,
) -> i64 {
    let id = resident::next_call();
    let halt = Arc::new(Halt::default());
    lock(halts()).insert(id, halt.clone());
    let within = Duration::from_millis(within_ms.max(1));
    let ask = Halting {
        ask: Over { h, id, within },
        halt: halt.clone(),
        id,
    };
    let job = run(
        ask,
        Moves {
            h,
            id,
            on_event: on_event.clone(),
        },
    );
    tokio::spawn(async move {
        let line = job.await;
        lock(halts()).remove(&id);
        if !halt.is() {
            on_event(h, "reply", line);
        }
    });
    id
}

/// 起一趟上传（暂存那一半）：回问答编号，进度与结局经 `on_event` 交；这台不在 ⇒ 0（`file` 当场关）。
pub fn stage(h: i64, file: File, name: String, within_ms: u64, on_event: OnEvent) -> i64 {
    if resident::feed(h).is_none() {
        return 0;
    }
    let mut random = [0u8; 16];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random);
    launch(h, within_ms, on_event, move |ask, mut moves| async move {
        let total = size_of(&file);
        stage_with(
            &ask,
            ask.id,
            chunk_core::key_of(random),
            (file, total),
            &name,
            &mut moves,
        )
        .await
    })
}

/// 起一趟下载：`args` 是 `{path}`（那台上的路径，线上那一形）。回问答编号；这台不在 · 入参读不动 ⇒ 0。
pub fn pull(
    h: i64,
    file: File,
    args: &str,
    shown: String,
    within_ms: u64,
    on_event: OnEvent,
) -> i64 {
    let path = serde_json::from_str::<serde_json::Value>(args)
        .ok()
        .and_then(|v| serde_json::from_value::<WirePath>(v.get("path")?.clone()).ok());
    let (Some(path), Some(_)) = (path, resident::feed(h)) else {
        return 0;
    };
    launch(h, within_ms, on_event, move |ask, mut moves| async move {
        pull_with(&ask, ask.id, path, file, &shown, &mut moves).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmd_wire::files::{
        FilesStageChunkReply, FilesStageChunkReq, HexBytes, ReadChunkReply, ReadChunkReq,
    };
    use std::sync::Mutex;

    /// 替身那台：收下送来的块；读回从 `file` 里切；`refuse` ⇒ 每一问都回那一句失败（码 `refused`）。
    #[derive(Default)]
    struct Box {
        staged: Mutex<Vec<(u64, Vec<u8>)>>,
        file: Vec<u8>,
        refuse: bool,
    }

    impl Ask for Box {
        type Fail = String;
        async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, String> {
            if self.refuse {
                return typed::<C>(
                    7,
                    "box",
                    Err(CallError::Remote {
                        code: "refused".into(),
                        message: "那台拒了".into(),
                        detail: "d".into(),
                        data: None,
                    }),
                );
            }
            let v = serde_json::to_value(req).unwrap();
            let reply = if C::NAME == "files-stage-chunk" {
                let r: FilesStageChunkReq = serde_json::from_value(v).unwrap();
                let n = r.content.bytes().len() as u64;
                self.staged
                    .lock()
                    .unwrap()
                    .push((r.seq, r.content.bytes().to_vec()));
                serde_json::to_value(FilesStageChunkReply { bytes: n }).unwrap()
            } else {
                let r: ReadChunkReq = serde_json::from_value(v).unwrap();
                let a = (r.offset as usize).min(self.file.len());
                let b = (a + r.len as usize).min(self.file.len());
                serde_json::to_value(ReadChunkReply {
                    path: r.path,
                    offset: r.offset,
                    size: self.file.len() as u64,
                    eof: b == self.file.len(),
                    content: HexBytes(self.file[a..b].to_vec()),
                })
                .unwrap()
            };
            typed::<C>(7, "box", Ok(Some(reply)))
        }
    }

    #[derive(Default)]
    struct Seen(Vec<(u64, Option<u64>)>);

    impl Watch for Seen {
        fn stopped(&self) -> bool {
            false
        }
        fn moved(&mut self, done: u64, total: Option<u64>) {
            self.0.push((done, total));
        }
    }

    /// 读到一半就坏的本机那一份。
    struct Broken;

    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }
    }

    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::StorageFull))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    fn json(line: &str) -> serde_json::Value {
        serde_json::from_str(line).expect("JSON")
    }

    /// 上传那一半：块号连着进同一个键、每块报一次走到哪，结局是 Kotlin 提交要交的那四样（摘要同 chunk-core 那一份算法）。
    #[test]
    fn a_staged_upload_reports_every_chunk_and_hands_back_what_the_commit_needs() {
        let body: Vec<u8> = (0..chunk_core::CHUNK * 2 + 5)
            .map(|i| (i % 251) as u8)
            .collect();
        let there = Box::default();
        let mut seen = Seen::default();
        let key = "a".repeat(32);
        let total = Some(body.len() as u64);
        let line = run(stage_with(
            &there,
            3,
            key.clone(),
            (&body[..], total),
            "up.bin",
            &mut seen,
        ));
        let v = json(&line);
        assert_eq!(
            (v["call"].as_i64(), v["ok"].as_bool()),
            (Some(3), Some(true)),
            "{v}"
        );
        assert_eq!(v["data"]["key"], key.as_str());
        assert_eq!(v["data"]["chunks"], 3);
        assert_eq!(v["data"]["bytes"], body.len());
        let mut sha = chunk_core::Stager::new(String::new());
        assert_eq!(v["data"]["sha256"], {
            run(sha.put(&Box::default(), body.clone())).unwrap();
            sha.finish().sha256
        });
        let staged = there.staged.lock().unwrap();
        assert_eq!(
            staged.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            staged
                .iter()
                .flat_map(|(_, b)| b.clone())
                .collect::<Vec<_>>(),
            body
        );
        assert_eq!(seen.0.last(), Some(&(body.len() as u64, total)));
        assert_eq!(seen.0.len(), 4, "起点一次 ＋ 每块一次");
    }

    /// 没成的三种：那台拒了 ⇒ 与一问一答同一行（码 · 那台那一句）；本机读 / 写不动 ⇒ 手机那一句、系统原话进详情。
    #[test]
    fn failures_land_as_the_reply_line_kotlin_already_reads() {
        let no = Box {
            refuse: true,
            ..Box::default()
        };
        let v = json(&run(stage_with(
            &no,
            4,
            "k".into(),
            (&[1u8][..], Some(1)),
            "up.bin",
            &mut Seen::default(),
        )));
        assert_eq!(
            (v["ok"].as_bool(), v["code"].as_str(), v["message"].as_str()),
            (Some(false), Some("refused"), Some("那台拒了"))
        );

        let v = json(&run(stage_with(
            &Box::default(),
            5,
            "k".into(),
            (Broken, None),
            "up.bin",
            &mut Seen::default(),
        )));
        assert_eq!(v["call"], 5);
        assert_eq!(
            v["message"],
            copy_core::copy_text("mobFiles.local.readFailed", &[("name", "up.bin")])
        );
        assert!(!v["detail"].as_str().unwrap().is_empty());

        let there = Box {
            file: vec![1, 2, 3],
            ..Box::default()
        };
        let v = json(&run(pull_with(
            &there,
            6,
            WirePath(b"/w/f".to_vec()),
            Broken,
            "/sdcard/f",
            &mut Seen::default(),
        )));
        assert_eq!(
            v["message"],
            copy_core::copy_text("mobFiles.local.writeFailed", &[("path", "/sdcard/f")])
        );
    }

    /// 下载那一半：逐块读回写进本机那一份，结局带总字节；走到哪按那台给的总数报。
    #[test]
    fn a_pull_writes_every_chunk_and_says_how_many_bytes() {
        let file: Vec<u8> = (0..chunk_core::CHUNK + 9).map(|i| (i % 13) as u8).collect();
        let there = Box {
            file: file.clone(),
            ..Box::default()
        };
        let mut out = Vec::new();
        let mut seen = Seen::default();
        let v = json(&run(pull_with(
            &there,
            8,
            WirePath(b"/w/f".to_vec()),
            &mut out,
            "/sdcard/f",
            &mut seen,
        )));
        assert_eq!(
            (
                v["call"].as_i64(),
                v["ok"].as_bool(),
                v["data"]["bytes"].as_u64()
            ),
            (Some(8), Some(true), Some(file.len() as u64))
        );
        assert_eq!(out, file);
        assert_eq!(
            seen.0.last(),
            Some(&(file.len() as u64, Some(file.len() as u64)))
        );
    }

    /// 那台读回第二块时：`Hang` ⇒ 一直不回 · `Late` ⇒ 回话前一刻撤了（撤在回话路上）。
    enum Second {
        Hang,
        Late,
    }

    struct Slow {
        there: Box,
        second: Second,
        halt: Arc<Halt>,
    }

    impl Ask for Slow {
        type Fail = String;
        async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, String> {
            let off = serde_json::to_value(req).unwrap()["offset"]
                .as_u64()
                .unwrap();
            if off > 0 {
                match self.second {
                    Second::Hang => std::future::pending::<()>().await,
                    Second::Late => self.halt.set(),
                }
            }
            self.there.ask(req).await
        }
    }

    /// 🔴 ［停］之后本机那一份一个字节都不再写：在问的那一块当场丢下（那台不回也不等）· 回话撤在路上那一块也不写；结局是撤了那一行。
    #[test]
    fn after_a_stop_no_byte_lands_in_the_local_file() {
        for second in [Second::Hang, Second::Late] {
            let file: Vec<u8> = (0..chunk_core::CHUNK * 3).map(|i| (i % 7) as u8).collect();
            let halt = Arc::new(Halt::default());
            let slow = Slow {
                there: Box {
                    file: file.clone(),
                    ..Box::default()
                },
                second,
                halt: halt.clone(),
            };
            let ask = Halting {
                ask: slow,
                halt: halt.clone(),
                id: 11,
            };
            let mut out = Vec::new();
            let line = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(async {
                    let mut seen = Seen::default();
                    let pull = pull_with(
                        &ask,
                        11,
                        WirePath(b"/w/f".to_vec()),
                        &mut out,
                        "/sdcard/f",
                        &mut seen,
                    );
                    // 撤了之后再让几圈：拉的那一趟该在这几圈里收手（计圈数，不按墙钟）。
                    let stop = async {
                        tokio::task::yield_now().await;
                        halt.set();
                        for _ in 0..64 {
                            tokio::task::yield_now().await;
                        }
                    };
                    tokio::select! {
                        biased;
                        line = pull => line,
                        () = stop => panic!("撤了还在等那台"),
                    }
                });
            assert_eq!(
                json(&line),
                serde_json::json!({"call": 11, "cancelled": true})
            );
            assert_eq!(out.len(), chunk_core::CHUNK, "撤之前写下的只有第一块");
        }
    }

    /// 🔴 撤一趟按问答编号认：在走的那一趟 ⇒ 撤了；不认得的编号 ⇒ `false`。
    #[test]
    fn a_stop_finds_its_transfer_by_call_id() {
        let h = Arc::new(Halt::default());
        lock(halts()).insert(4242, h.clone());
        assert!(halt(4242));
        assert!(h.is());
        lock(halts()).remove(&4242);
        assert!(!halt(4243));
    }

    /// 形状对不上的回包 ⇒ `unreadable`（不当成功用），详情点名那条命令。
    #[test]
    fn a_reply_of_the_wrong_shape_is_unreadable_not_ok() {
        let e = typed::<FilesStageChunkReq>(9, "box", Ok(Some(serde_json::json!({"nope": 1}))))
            .unwrap_err();
        let v = json(&e);
        assert_eq!(
            (v["call"].as_i64(), v["unreadable"].as_bool()),
            (Some(9), Some(true))
        );
        assert!(v["detail"]
            .as_str()
            .unwrap()
            .starts_with("files-stage-chunk: "));
    }
}
