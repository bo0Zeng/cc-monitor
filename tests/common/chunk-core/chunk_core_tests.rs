//! 分块传那两半：上传逐块进暂存区（块号连着、整份摘要对、本机读不动 / 撤了各照调用方那一形）；下载逐块读回、读到头为止。
//! 替身那台在内存里（不碰盘、不起后端）。

use super::*;
use cmd_wire::files::{FilesStageChunkReply, HexBytes, ReadChunkReply};
use std::cell::RefCell;

/// 替身答的都是当场就有的结果 ⇒ 不要运行时，空唤醒器推一下就完。
fn run<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => panic!("替身不该挂起"),
    }
}

/// 那台：收下送来的块（块号 · 字节）；读回时从 `file` 里切。`fail_at` ⇒ 第几问回失败。
#[derive(Default)]
struct Box {
    staged: RefCell<Vec<(u64, Vec<u8>)>>,
    file: Vec<u8>,
    asks: RefCell<usize>,
    fail_at: Option<usize>,
}

impl Ask for Box {
    type Fail = String;
    async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, String> {
        let n = {
            let mut a = self.asks.borrow_mut();
            *a += 1;
            *a
        };
        if self.fail_at == Some(n) {
            return Err(format!("第 {n} 问那台拒了"));
        }
        let v = serde_json::to_value(req).unwrap();
        let reply = match C::NAME {
            "files-stage-chunk" => {
                let r: FilesStageChunkReq = serde_json::from_value(v).unwrap();
                self.staged
                    .borrow_mut()
                    .push((r.seq, r.content.bytes().to_vec()));
                serde_json::to_value(FilesStageChunkReply {
                    bytes: r.content.bytes().len() as u64,
                })
                .unwrap()
            }
            "files-read-chunk" => {
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
            }
            other => panic!("替身不认 {other}"),
        };
        Ok(serde_json::from_value(reply).unwrap())
    }
}

#[derive(Default)]
struct Seen {
    moved: Vec<(u64, Option<u64>)>,
    stop_after: Option<usize>,
}

impl Watch for Seen {
    fn stopped(&self) -> bool {
        self.stop_after.is_some_and(|n| self.moved.len() > n)
    }
    fn moved(&mut self, done: u64, total: Option<u64>) {
        self.moved.push((done, total));
    }
}

fn body(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 7 % 251) as u8).collect()
}

fn sha(b: &[u8]) -> String {
    hex(&sha2::Sha256::digest(b))
}

#[test]
fn an_upload_goes_block_by_block_with_consecutive_numbers_and_the_whole_digest() {
    let data = body(CHUNK * 2 + 123);
    let b = Box::default();
    let mut w = Seen::default();
    let got = run(stage(
        &b,
        "k".repeat(32),
        &data[..],
        Some(data.len() as u64),
        &mut w,
        || "撤了".to_string(),
        |e| e.to_string(),
    ))
    .unwrap();
    let staged = b.staged.borrow();
    assert_eq!(staged.iter().map(|s| s.0).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(
        staged.iter().map(|s| s.1.len()).collect::<Vec<_>>(),
        [CHUNK, CHUNK, 123]
    );
    assert_eq!(
        staged.iter().flat_map(|s| s.1.clone()).collect::<Vec<_>>(),
        data
    );
    assert_eq!(
        got,
        Staged {
            key: "k".repeat(32),
            chunks: 3,
            bytes: data.len() as u64,
            sha256: sha(&data)
        }
    );
    assert_eq!(
        w.moved.last(),
        Some(&(data.len() as u64, Some(data.len() as u64)))
    );
    // 正好整块：不多送一块空的。
    let even = body(CHUNK);
    let b2 = Box::default();
    let got = run(stage(
        &b2,
        "e".repeat(32),
        &even[..],
        None,
        &mut Seen::default(),
        String::new,
        |e| e.to_string(),
    ))
    .unwrap();
    assert_eq!((got.chunks, b2.staged.borrow().len()), (1, 1));
}

#[test]
fn a_failed_block_a_stop_and_a_local_read_error_each_end_it_their_own_way() {
    let data = body(CHUNK * 3);
    let refused = Box {
        fail_at: Some(2),
        ..Default::default()
    };
    let e = run(stage(
        &refused,
        "k".repeat(32),
        &data[..],
        None,
        &mut Seen::default(),
        String::new,
        |e| e.to_string(),
    ))
    .unwrap_err();
    assert_eq!(e, "第 2 问那台拒了");
    assert_eq!(refused.staged.borrow().len(), 1, "那一块没成之后不再送");
    let b = Box::default();
    let mut w = Seen {
        stop_after: Some(1),
        ..Default::default()
    };
    let e = run(stage(
        &b,
        "k".repeat(32),
        &data[..],
        None,
        &mut w,
        || "撤了".to_string(),
        |e| e.to_string(),
    ))
    .unwrap_err();
    assert_eq!((e.as_str(), b.staged.borrow().len()), ("撤了", 1));
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("盘坏了"))
        }
    }
    let e = run(stage(
        &Box::default(),
        "k".repeat(32),
        Broken,
        None,
        &mut Seen::default(),
        String::new,
        |e| format!("本机：{e}"),
    ))
    .unwrap_err();
    assert_eq!(e, "本机：盘坏了");
}

#[test]
fn a_download_reads_block_by_block_until_the_end_and_hands_each_block_over() {
    struct Keep(Vec<u8>);
    impl Sink for Keep {
        type Fail = String;
        async fn put(&mut self, bytes: Vec<u8>) -> Result<(), String> {
            self.0.extend(bytes);
            Ok(())
        }
    }
    let data = body(CHUNK + 9);
    let b = Box {
        file: data.clone(),
        ..Default::default()
    };
    let mut keep = Keep(Vec::new());
    let mut w = Seen::default();
    let n = run(pull(
        &b,
        WirePath::from("/f"),
        &mut keep,
        &mut w,
        String::new,
    ))
    .unwrap();
    assert_eq!((n, keep.0), (data.len() as u64, data.clone()));
    assert_eq!(*b.asks.borrow(), 2, "读到 eof 就停");
    assert_eq!(
        w.moved.last(),
        Some(&(data.len() as u64, Some(data.len() as u64)))
    );
    let empty = Box::default();
    let mut keep = Keep(Vec::new());
    assert_eq!(
        run(pull(
            &empty,
            WirePath::from("/f"),
            &mut keep,
            &mut Seen::default(),
            String::new
        ))
        .unwrap(),
        0
    );
}

#[test]
fn hex_is_lowercase_and_byte_exact() {
    let b: Vec<u8> = (0u8..=255).collect();
    let want: String = b.iter().map(|x| format!("{x:02x}")).collect();
    assert_eq!(hex(&b), want);
    assert_eq!(
        key_of([0xab; 16]).len(),
        32,
        "暂存件的键是 32 位小写十六进制"
    );
}
