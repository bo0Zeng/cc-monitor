//! 带上限的异步读一行 · 读整份：超了回超限、不截断；超限之后内存不涨；超长那一行之后的流照读。
use super::*;
use cap_core::{Cap, Over};
use tokio::io::BufReader;

fn cap(bytes: u64) -> Cap {
    Cap {
        bytes,
        over: Over::Reject,
    }
}

/// 正常一行：内容不含行尾 `\n`。
#[tokio::test]
async fn a_normal_line_comes_back_without_its_newline() {
    let mut rd = BufReader::new(&b"hello\n"[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(64)).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf, b"hello");
}

/// 恰好等于上限的一行照收：边界差一格就是合法数据被丢。
#[tokio::test]
async fn a_line_exactly_at_the_cap_is_still_accepted() {
    let mut data = vec![b'x'; 16];
    data.push(b'\n');
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf.len(), 16);
}

/// `cap.bytes + 1` 字节的一行 ⇒ 超限，报出真实字节数，`buf` 里不留半行。
#[tokio::test]
async fn one_byte_over_the_cap_is_over_not_cut() {
    let c = cap(16);
    let mut data = vec![b'x'; (c.bytes + 1) as usize];
    data.push(b'\n');
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    match read_capped_line(&mut rd, &mut buf, c).await.unwrap() {
        CappedLine::TooLong(n) => assert_eq!(n, c.bytes + 1, "报出的字节数是真实长度"),
        other @ (CappedLine::Line | CappedLine::Eof) => panic!("该判超限，实得 {other:?}"),
    }
    assert!(buf.is_empty(), "超限的行不许截成前 16 字节留在 buf 里");
}

/// 超限之后不再往 `buf` 里塞字节：读完再判的话，一条几百 MiB 无换行的流内存就跟着涨。
#[tokio::test]
async fn over_limit_stops_growing_the_buffer() {
    let mut data = vec![b'x'; 100_000];
    data.push(b'\n');
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    match read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap() {
        CappedLine::TooLong(n) => assert_eq!(n, 100_000),
        other @ (CappedLine::Line | CappedLine::Eof) => panic!("该判超限，实得 {other:?}"),
    }
    assert!(
        buf.capacity() < 1024,
        "超限之后 buf 容量涨到了 {}：字节还在往里塞",
        buf.capacity()
    );
}

/// 超长那一行之后的下一行照读：丢一行不连带丢整条流。
/// 超长那一行跨几个 `fill_buf` 块（20_000 > `BufReader` 默认 8 KiB），超限之后续找换行那条路才走得到。
#[tokio::test]
async fn the_line_after_an_over_long_one_is_still_delivered() {
    let mut data = vec![b'x'; 20_000];
    data.push(b'\n');
    data.extend_from_slice(b"after\n");
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::TooLong(20_000)
    ));
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf, b"after");
}

/// 流尽而无残行 ⇒ `Eof`；末尾没 `\n` 的残行当一行交出。
#[tokio::test]
async fn eof_and_a_trailing_partial_line() {
    let mut rd = BufReader::new(&b""[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::Eof
    ));

    let mut rd = BufReader::new(&b"tail"[..]);
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf, b"tail");
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::Eof
    ));
}

/// 超长的残行（没 `\n` 就到头）也是超限，不当一行交出。
#[tokio::test]
async fn an_over_long_trailing_partial_is_over() {
    let data = vec![b'x'; 40];
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::TooLong(40)
    ));
}

/// 空行（连着两个 `\n`）是一行，不是 EOF。
#[tokio::test]
async fn an_empty_line_is_a_line_not_an_eof() {
    let mut rd = BufReader::new(&b"\nx\n"[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, cap(16)).await.unwrap(),
        CappedLine::Line
    ));
    assert!(buf.is_empty());
}

/// 整读：恰好到上限照收；空流是零字节的一份。
#[tokio::test]
async fn read_whole_up_to_the_cap() {
    let data = vec![b'y'; 32];
    match read_capped_all(&data[..], cap(32)).await.unwrap() {
        CappedAll::All(got) => assert_eq!(got, data),
        CappedAll::OverCap => panic!("恰好到上限该收"),
    }
    match read_capped_all(&b""[..], cap(32)).await.unwrap() {
        CappedAll::All(got) => assert!(got.is_empty()),
        CappedAll::OverCap => panic!("空流不超限"),
    }
}

/// 整读：`cap.bytes + 1` 字节 ⇒ 超限，不交出前 `cap.bytes` 字节当整份。
#[tokio::test]
async fn read_whole_one_byte_over_is_over_not_cut() {
    let c = cap(32);
    let data = vec![b'y'; (c.bytes + 1) as usize];
    assert!(matches!(
        read_capped_all(&data[..], c).await.unwrap(),
        CappedAll::OverCap
    ));
}

/// 整读：超限那一刻就停，不把后面的字节读进来（对端可以一直发）。
#[tokio::test]
async fn read_whole_stops_reading_past_the_cap() {
    let data = vec![b'y'; 1_000_000];
    let mut rd = &data[..];
    assert!(matches!(
        read_capped_all(&mut rd, cap(16)).await.unwrap(),
        CappedAll::OverCap
    ));
    assert!(
        rd.len() >= 1_000_000 - 64 * 1024,
        "超限之后还在读：只剩 {} 字节没读",
        rd.len()
    );
}
