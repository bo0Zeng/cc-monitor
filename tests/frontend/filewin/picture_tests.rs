//! [`super`] 的判据 —— **一张图读一次、解一次**（预览 · 「看一眼」· 查看页共用那一份）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`small_images_stay_as_they_are_and_big_ones_shrink_to_the_side`] | 小图原样；最长边过了 `side` ⇒ 等比缩到那么大，原图尺寸照记；不是图 ⇒ 那一句 | 期望手写 |
//! | [`too_many_pixels_says_so_instead_of_undecodable`] | 头上写着 30000 × 30000 的 png ⇒ 「像素过多」那一句，不是「无法解码」 | 文件头手拼（CRC 现算） |
//! | [`one_picture_is_read_once_for_every_place_that_shows_it`] | 两处要同一张 ⇒ 线上恰一趟从 0 起读的 `files-read-chunk`、两处拿到同一张贴图 | 实得是合成后端那本账；字节是盘上真文件 |
//! | [`what_nobody_shows_any_more_is_dropped`] | `keep_only` 不留它 ⇒ 再看就没了；再要 ⇒ 再读一趟 | 读数是合成后端那本账 |
//! | [`a_picture_that_grows_past_the_cap_while_read_stops_and_says_so`] | 盘上那份比上限大（列表那时还小）⇒ 读到上限就停、那一句，不解码 | 线上块数读的是那本账 |

use super::*;
use crate::find::testing::{wire_up, Declared, FakeBackend, Wired};

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([
            (x * 7 % 251) as u8,
            (y * 13 % 241) as u8,
            ((x ^ y) % 239) as u8,
            255,
        ])
    })
    .write_to(&mut buf, image::ImageFormat::Png)
    .unwrap();
    buf.into_inner()
}

#[test]
fn small_images_stay_as_they_are_and_big_ones_shrink_to_the_side() {
    let (img, native) = decode(&png(3, 2), 8192).unwrap();
    assert_eq!((img.size, native), ([3, 2], [3, 2]));
    let (img, native) = decode(&png(100, 10), 40).unwrap();
    assert_eq!(img.size, [40, 4], "最长边没缩到 side");
    assert_eq!(native, [100, 10], "原图尺寸没照记");
    assert_eq!(
        decode(b"not an image", 8192).err().as_deref(),
        Some(copy_text("rsFilewinPicture.decode.undecodable", &[]).as_str())
    );
}

/// 只有签名 ＋ IHDR 的 png（宽高照给，CRC 现算）：解码器在分配之前就按内存那道闸判。
fn png_header(w: u32, h: u32) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in bytes {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = b"IHDR".to_vec();
    chunk.extend_from_slice(&w.to_be_bytes());
    chunk.extend_from_slice(&h.to_be_bytes());
    chunk.extend_from_slice(&[8, 6, 0, 0, 0]);
    out.extend_from_slice(&13u32.to_be_bytes());
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&crc(&chunk).to_be_bytes());
    // 一个空的 IDAT：解码器读到它才算读完了文件头。
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(b"IDAT");
    out.extend_from_slice(&crc(b"IDAT").to_be_bytes());
    out
}

#[test]
fn too_many_pixels_says_so_instead_of_undecodable() {
    let said = decode(&png_header(30000, 30000), 8192).err().unwrap();
    assert_eq!(
        said,
        copy_text("rsFilewinPicture.decode.tooManyPixels", &[]),
        "过闸的大图说成了别的"
    );
}

async fn rig(tag: &str, bytes: &[u8]) -> (Wired, std::path::PathBuf, PicKey) {
    let dir = std::env::temp_dir().join(format!("ccm-fw-pic-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.png");
    std::fs::write(&file, bytes).unwrap();
    let wired = wire_up(
        tag,
        FakeBackend::new(&["files-read-chunk"], Declared::default()),
    )
    .await;
    let key = PicKey {
        path: file.to_string_lossy().to_string(),
        size: bytes.len() as u64,
        mtime: Some(1),
    };
    (wired, dir, key)
}

fn load_of(wired: &Wired, key: &PicKey, cap: u64) -> Load {
    Load {
        rt: tokio::runtime::Handle::current(),
        line: wired.line.clone(),
        origin: crate::source::Origin(wired.origin.clone()),
        wire: serde_json::Value::String(key.path.clone()),
        cap,
        side: 8192,
        ctx: None,
    }
}

fn first_reads(wired: &Wired) -> usize {
    wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-read-chunk" && r["args"]["offset"] == 0)
        .count()
}

fn chunk_reads(wired: &Wired) -> usize {
    wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-read-chunk")
        .count()
}

async fn settle(p: &Pictures, key: &PicKey, ctx: &egui::Context) -> Look {
    for _ in 0..1000 {
        match p.look(key, Some(ctx)) {
            Some(Look::Reading { .. }) | None => {}
            Some(l) => return l,
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("等了 5 秒图还没读完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_picture_is_read_once_for_every_place_that_shows_it() {
    // 600 × 400 的噪点 png：好几块（一块 256 KB）。
    let bytes = png(600, 400);
    assert!(
        bytes.len() as u64 > crate::lossy_pull::PULL_CHUNK,
        "量具：图太小，只有一块"
    );
    let (wired, dir, key) = rig("fw-pic-once", &bytes).await;
    let ctx = egui::Context::default();
    let pics = Pictures::default();
    // 预览要它、查看页也要它（同一把钥匙）。
    assert!(
        pics.load(&key, load_of(&wired, &key, 1 << 30)),
        "第一处要却没读"
    );
    assert!(
        !pics.load(&key, load_of(&wired, &key, 1 << 30)),
        "第二处要又起了一趟"
    );
    let a = match settle(&pics, &key, &ctx).await {
        Look::Ready(p) => p,
        other => panic!("没读成：{other:?}"),
    };
    let b = match pics.look(&key, Some(&ctx)) {
        Some(Look::Ready(p)) => p,
        other => panic!("第二处没拿到：{other:?}"),
    };
    assert_eq!(a.tex.id(), b.tex.id(), "两处拿到的不是同一张贴图");
    assert_eq!(a.native, [600, 400]);
    assert_eq!(first_reads(&wired), 1, "同一张图从头读了不止一趟");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_nobody_shows_any_more_is_dropped() {
    let (wired, dir, key) = rig("fw-pic-drop", &png(8, 8)).await;
    let ctx = egui::Context::default();
    let pics = Pictures::default();
    pics.load(&key, load_of(&wired, &key, 1 << 30));
    assert!(matches!(settle(&pics, &key, &ctx).await, Look::Ready(_)));
    pics.keep_only(&[key.clone()].into_iter().collect());
    assert!(pics.look(&key, None).is_some(), "还在摆着的被扔了");
    pics.keep_only(&Default::default());
    assert!(pics.look(&key, None).is_none(), "没人摆的还留着");
    assert!(
        pics.load(&key, load_of(&wired, &key, 1 << 30)),
        "扔了之后再要没读"
    );
    assert!(matches!(settle(&pics, &key, &ctx).await, Look::Ready(_)));
    assert_eq!(first_reads(&wired), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picture_that_grows_past_the_cap_while_read_stops_and_says_so() {
    let bytes = png(600, 400);
    let (wired, dir, mut key) = rig("fw-pic-grew", &bytes).await;
    key.size = 10; // 列表那时还小。
    let cap = crate::lossy_pull::PULL_CHUNK + 1;
    let ctx = egui::Context::default();
    let pics = Pictures::default();
    pics.load(&key, load_of(&wired, &key, cap));
    let said = match settle(&pics, &key, &ctx).await {
        Look::Failed(s) => s,
        other => panic!("超了上限还在显示：{other:?}"),
    };
    assert_eq!(
        said,
        copy_text(
            "rsFilewinPicture.read.grew",
            &[("limit", &copy_core::size_text(cap))]
        )
    );
    assert_eq!(chunk_reads(&wired), 2, "到了上限没停");
    let _ = std::fs::remove_dir_all(dir);
}
