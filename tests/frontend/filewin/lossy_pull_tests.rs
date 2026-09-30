//! 〔FILES2 · 第四波 · 2026-09-27〕`filewin/lossy_pull.rs` 的判据 —— **有损名（非 UTF-8）的下载**。
//!
//! 要求住址：`设计/60 §7` 第 9 条 Q4；主会话 09-27 按通行做法裁：「落到 Linux 本机 ⇒ 字节原样当文件名；落到 Windows 本机 ⇒
//! 名字按有损形替换并在结局里说一句『名字里有认不出的字节，已改成 X』」；用户 09-27 V152「经那台后端链路按字节寻址分块读回」（下载对远端只读，`设计/60 §4.4`）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`a_kept_lossy_name_lands_as_its_raw_bytes_on_unix_and_a_renamed_one_as_typed`] | 尾段没改 ⇒ 父目录 ＋ 原始字节（b16）；改了 ⇒ 原样串 | 期望手写 |
//! | [`a_lossy_pull_reads_the_remote_by_bytes_and_lands_it_through_the_local_backend`] | 远端**只读**（线上只有 `files-read-chunk`，按字节寻址）；块逐个进本机暂存、本机提交落点按字节、摘要 == 本机现算 | 实得是合成对端按 `origin` 记下的；摘要 `sha2` 现算 |
//! | [`the_chunk_is_the_backend_cap`] | 窗口一块读多少 == 后端 `files::READ_CHUNK_MAX_BYTES` | 读两侧源码 |

use super::*;

#[test]
#[cfg(unix)]
fn a_kept_lossy_name_lands_as_its_raw_bytes_on_unix_and_a_renamed_one_as_typed() {
    let (v, note) = local_dest("/home/u/dl/f\u{FFFD}.bin", "f\u{FFFD}.bin", b"f\xfe.bin");
    assert_eq!(
        v,
        crate::source::wire_bytes(b"/home/u/dl/f\xfe.bin"),
        "尾段没改却没换成原始字节"
    );
    assert_eq!(note, None, "Linux 上字节原样，不该出声");
    let (v, note) = local_dest("/home/u/dl/renamed.bin", "f\u{FFFD}.bin", b"f\xfe.bin");
    assert_eq!(v, serde_json::json!("/home/u/dl/renamed.bin"));
    assert_eq!(note, None);
    assert_eq!(
        local_path_of(&crate::source::wire_bytes(b"/a/f\xfe")),
        Some(std::path::PathBuf::from(
            <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"/a/f\xfe")
        ))
    );
}

/// 合成两台（远端 `R` · 本机 `<local>`）：远端只答 `files-read-chunk`（按 offset 切一份定长正文），本机只答写那几条；全部按 origin 记。
struct Two {
    log: std::sync::Arc<std::sync::Mutex<Vec<(String, String, serde_json::Value)>>>,
    body: Vec<u8>,
}

impl chan_core::chan::router::Backends for Two {
    fn call(
        &self,
        origin: chan_core::chan::wire::Origin,
        op: chan_core::chan::wire::Op,
        payload: chan_core::chan::wire::Body,
        _left: std::time::Duration,
        _cancel: chan_core::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<chan_core::chan::wire::Body, chan_core::chan::wire::CallError>,
    > {
        let a: serde_json::Value = serde_json::from_slice(&payload.0).unwrap_or_default();
        self.log
            .lock()
            .unwrap()
            .push((origin.0.clone(), op.0.clone(), a.clone()));
        let v = if op.0 == "files-read-chunk" {
            let off = a["offset"].as_u64().unwrap() as usize;
            let len = a["len"].as_u64().unwrap() as usize;
            let end = (off + len).min(self.body.len());
            let part = &self.body[off.min(end)..end];
            serde_json::json!({ "path": a["path"], "offset": off, "size": self.body.len(),
                "eof": end >= self.body.len(), "content": { "b16": crate::chunk_upload::hex(part) } })
        } else {
            serde_json::json!({ "path": "/x", "bytes": 0 })
        };
        Box::pin(async move {
            Ok(chan_core::chan::wire::Body(
                serde_json::to_vec(&v).unwrap_or_default(),
            ))
        })
    }

    fn subscribe(
        &self,
        _o: chan_core::chan::wire::Origin,
        _k: chan_core::chan::wire::Kind,
        _f: Option<chan_core::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, chan_core::chan::wire::Item> {
        use futures::StreamExt as _;
        futures::stream::empty().boxed()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lossy_pull_reads_the_remote_by_bytes_and_lands_it_through_the_local_backend() {
    use sha2::Digest as _;
    let body: Vec<u8> = (0..(PULL_CHUNK as usize + 1000))
        .map(|i| (i % 249) as u8)
        .collect();
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let h = chan_core::chan::handoff::start_with(
        std::sync::Arc::new(Two {
            log: log.clone(),
            body: body.clone(),
        }),
        chan_core::chan::handoff::mint_key(),
        4 << 20,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("回环口绑得上");
    let line = chan_core::chan::dial::dial(
        &h,
        chan_core::chan::wire::Budget {
            until: std::time::Instant::now() + std::time::Duration::from_secs(5),
            cancel: chan_core::chan::wire::CancelToken::new(),
        },
    )
    .await
    .expect("拨得通");
    let board = crate::download::DownloadBoard::default();
    let local = crate::source::wire_bytes(b"/tmp/dl/f\xfe");
    pull_by_bytes(
        &line,
        &chan_core::chan::wire::Origin("R".into()),
        b"/srv/d\xff/f\xfe",
        local,
        true,
        &board,
    )
    .await
    .expect("该走通");
    let got = log.lock().unwrap().clone();
    // 远端只读：那台机器上只收到读块那一条，别的一条都没有。
    let remote: Vec<&str> = got
        .iter()
        .filter(|(o, ..)| o == "R")
        .map(|(_, c, _)| c.as_str())
        .collect();
    assert_eq!(
        remote,
        ["files-read-chunk", "files-read-chunk"],
        "远端收到了读以外的命令"
    );
    assert!(
        got.iter()
            .filter(|(o, ..)| o == "R")
            .all(|(_, _, a)| a["path"] == crate::source::wire_bytes(b"/srv/d\xff/f\xfe")),
        "远端没按字节寻址"
    );
    let mine: Vec<&(String, String, serde_json::Value)> =
        got.iter().filter(|(o, ..)| o == "<local>").collect();
    let stages: Vec<&serde_json::Value> = mine
        .iter()
        .filter(|(_, c, _)| c == "files-stage-chunk")
        .map(|(_, _, a)| a)
        .collect();
    let mut rebuilt = Vec::new();
    for (i, a) in stages.iter().enumerate() {
        assert_eq!(a["seq"], i);
        rebuilt.extend(unhex(a["content"]["b16"].as_str().unwrap()).unwrap());
    }
    assert_eq!(rebuilt, body, "进本机暂存的块拼回来不是远端那份");
    let commit = &mine.last().expect("本机没收到提交").2;
    let want_sha: String = sha2::Sha256::digest(&body)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    assert_eq!(
        commit,
        &serde_json::json!({
            "key": stages[0]["key"], "root": "/tmp/dl", "rel": crate::source::wire_bytes(b"f\xfe"),
            "overwrite": true, "expect": { "sha256": want_sha }, "chunks": 2, "bytes": body.len(),
        })
    );
    assert_eq!(
        board.seen(),
        (body.len() as u64, body.len() as u64),
        "进度没报满"
    );
}

#[test]
fn the_chunk_is_the_backend_cap() {
    let backend = guard_core::production_code(include_str!("../../../src/backend/files/mod.rs"));
    let needle = format!(
        "pub const READ_CHUNK_MAX_BYTES: u64 = {} * 1024;",
        PULL_CHUNK / 1024
    );
    assert_eq!(
        backend.matches(needle.as_str()).count(),
        1,
        "窗口一块读多少与后端的上限不是同一个数"
    );
}
