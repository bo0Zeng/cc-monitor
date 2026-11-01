//! 常驻流 deflate 的读半边：一帧的压缩字节整段到了、之后那台不再来字节 ⇒ 这一帧当场整行读得出，不等下一批。
use super::*;
use std::time::Duration;

/// 只给这几段字节、之后永远 `Pending`（不唤醒）的读者：那台静下来时的 socket。
struct ThenSilent {
    chunks: std::collections::VecDeque<Vec<u8>>,
}

impl AsyncRead for ThenSilent {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let Some(c) = self.chunks.front_mut() else {
            return Poll::Pending;
        };
        let n = c.len().min(out.remaining());
        out.put_slice(&c[..n]);
        c.drain(..n);
        if c.is_empty() {
            self.chunks.pop_front();
        }
        Poll::Ready(Ok(()))
    }
}

/// 同那台写半边：raw deflate 级别 6，一帧一次 sync flush。
fn deflate_frames(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut z = flate2::Compress::new(flate2::Compression::new(6), false);
    let mut out = Vec::new();
    for f in frames {
        let mut at = 0;
        loop {
            out.reserve(f.len() + 1024);
            let was = z.total_in();
            z.compress_vec(&f[at..], &mut out, flate2::FlushCompress::Sync)
                .unwrap();
            at += (z.total_in() - was) as usize;
            if at == f.len() && out.capacity() > out.len() {
                break;
            }
        }
    }
    out
}

/// 一行 JSON：字符串里是 `body`，末尾一个 `\n`。
fn frame(body: &[u8]) -> Vec<u8> {
    let mut f = br#"{"kind":"line","text":""#.to_vec();
    f.extend_from_slice(body);
    f.extend_from_slice(b"\"}\n");
    f
}

/// 伪随机的小写字母（不依赖随机库，夹具每次一样）。
fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            b'a' + ((s >> 33) % 26) as u8
        })
        .collect()
}

async fn every_frame_comes_out_before_the_silence(frames: Vec<Vec<u8>>, piece: usize) {
    let bytes = deflate_frames(&frames);
    let chunks = bytes.chunks(piece).map(|c| c.to_vec()).collect();
    let r = BufReader::new(ThenSilent { chunks });
    let mut rd = Downstream::new(r, true);
    let mut buf = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        let got = tokio::time::timeout(
            Duration::from_secs(1),
            read_capped_line(&mut rd, &mut buf, BACKEND_FRAME_LINE_CAP),
        )
        .await
        .unwrap_or_else(|_| {
            panic!(
                "第 {i} 帧（{} 字节）的压缩字节全到了却没交出来（每段 {piece} 字节）",
                f.len()
            )
        });
        assert!(matches!(got.unwrap(), CappedLine::Line));
        assert_eq!(buf, f[..f.len() - 1], "第 {i} 帧内容");
    }
}

#[tokio::test]
async fn the_last_frame_comes_out_once_its_bytes_are_in_even_if_the_wire_goes_quiet() {
    // 重复多的一行：几字节的回指拷贝就解出一大段 ⇒ 解压器把输入吃干净、输出还压在手里。
    let repetitive = frame(&b"ab".repeat(100 * 1024));
    // 一段随机、后面整段重复它：长回指跨过外层 8 KB 的读缓冲。
    let mut echo = noise(16 * 1024, 7);
    echo.extend_from_within(..);
    echo.extend_from_within(..);
    let echoed = frame(&echo);
    let random = frame(&noise(200 * 1024, 11));
    for piece in [usize::MAX, 4096, 1] {
        every_frame_comes_out_before_the_silence(vec![repetitive.clone()], piece).await;
        every_frame_comes_out_before_the_silence(vec![echoed.clone()], piece).await;
        every_frame_comes_out_before_the_silence(vec![random.clone()], piece).await;
        every_frame_comes_out_before_the_silence(
            vec![
                frame(b"short"),
                repetitive.clone(),
                echoed.clone(),
                frame(b"tail"),
            ],
            piece,
        )
        .await;
    }
}
