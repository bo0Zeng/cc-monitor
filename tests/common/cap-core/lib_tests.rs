//! 有上限读（`cap-core`）的判据：正好到上限照交 · 超一个字节就是超、不截 · 超了就停 · 窗口读 · 读本机文件的两道。

use super::*;

/// 同步整读的判据用的一道读上限（超了怎么办这几条不看，只看字节数）。
fn reject(bytes: u64) -> Cap {
    Cap {
        bytes,
        over: Over::Reject,
    }
}

/// 同步整读：正好到上限照交整份，空流交空。
#[test]
fn sync_read_whole_up_to_the_cap() {
    let data = vec![b'x'; 32];
    assert_eq!(
        read_capped_from(&data[..], reject(32)).unwrap(),
        CappedAll::All(data)
    );
    assert_eq!(
        read_capped_from(&b""[..], reject(32)).unwrap(),
        CappedAll::All(Vec::new())
    );
}

/// 同步整读：`cap.bytes + 1` 字节 ⇒ 超限，不交出前 `cap.bytes` 字节当整份。
#[test]
fn sync_read_whole_one_byte_over_is_over_not_cut() {
    let c = reject(32);
    let data = vec![b'y'; (c.bytes + 1) as usize];
    assert_eq!(read_capped_from(&data[..], c).unwrap(), CappedAll::OverCap);
}

/// 同步整读：超限那一刻就停，不把后面的字节读进来（文件读的途中还会长 · 管子另一头可以一直写）。
#[test]
fn sync_read_whole_stops_reading_past_the_cap() {
    let data = vec![b'y'; 1_000_000];
    let mut rd = &data[..];
    assert_eq!(
        read_capped_from(&mut rd, reject(16)).unwrap(),
        CappedAll::OverCap
    );
    assert!(
        rd.len() >= 1_000_000 - 64 * 1024,
        "超限之后还在读：只剩 {} 字节没读",
        rd.len()
    );
}

/// 读窗口：至多 `n` 字节，读到 `n` 就停，后面的留在流里；流比窗口短 ⇒ 交到流尽为止。
#[test]
fn a_window_read_stops_at_its_width_and_leaves_the_rest() {
    let data: Vec<u8> = (0..100u8).collect();
    let mut rd = &data[..];
    assert_eq!(read_window(&mut rd, 10).unwrap(), data[..10].to_vec());
    assert_eq!(rd.len(), 90, "窗口读多拿了字节");
    assert_eq!(read_window(&mut rd, 1000).unwrap(), data[10..].to_vec());
    assert_eq!(read_window(&mut rd, 0).unwrap(), Vec::<u8>::new());
}

fn regular_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-regular-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 读本机文件：普通文件到上限照交；长度已过上限 ⇒ 不打开就回超限、带上实际大小；读的途中长过上限 ⇒ 超限、不带大小。
#[test]
fn a_regular_file_is_read_up_to_the_cap_and_refused_past_it() {
    let d = regular_dir("cap");
    let f = d.join("b.bin");
    std::fs::write(&f, b"123456789").unwrap();
    let meta = std::fs::metadata(&f).unwrap();
    let open = || std::fs::File::open(&f);
    assert_eq!(
        read_capped_regular(&meta, open, reject(9)).unwrap(),
        Regular::Whole(b"123456789".to_vec())
    );
    let never = || -> std::io::Result<std::fs::File> { panic!("长度已过上限还打开了") };
    assert_eq!(
        read_capped_regular(&meta, never, reject(8)).unwrap(),
        Regular::OverCap(Some(9))
    );
    // 看长度那一下还是 9 字节，打开时已经长了：第二道照样拦住。
    std::fs::write(&f, b"1234567890123").unwrap();
    assert_eq!(
        read_capped_regular(&meta, open, reject(9)).unwrap(),
        Regular::OverCap(None)
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 读本机文件：不是普通文件（目录 · FIFO · 设备）⇒ 不打开（打开一个 FIFO 会卡住等写端）。
#[test]
fn a_non_regular_file_is_not_opened() {
    let d = regular_dir("dir");
    let meta = std::fs::metadata(&d).unwrap();
    let never = || -> std::io::Result<std::fs::File> { panic!("不是普通文件还打开了") };
    assert_eq!(
        read_capped_regular(&meta, never, reject(1 << 20)).unwrap(),
        Regular::NotRegular
    );
    let _ = std::fs::remove_dir_all(&d);
}
