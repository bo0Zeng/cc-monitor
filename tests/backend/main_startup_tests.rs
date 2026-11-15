//! 进程起来那一下（`stream/entry.rs::run`）：拨号层自己读这一端的机器表（`dial::machines_now`，起来时不用谁先装一份读法）；
//! 常驻起来那一段也钉在这里。

const ENTRY: &str = include_str!("../../src/backend/stream/entry.rs");

/// 家里放一份机器表、什么都不装 ⇒ 拨号层照它读到那一台（各台的常驻流 · 问那台 · 传输 · 部署都按名字查它）。
#[test]
fn the_machine_table_is_read_without_anything_installed() {
    let home = crate::platform::paths::home_dir().expect("测试档的家（沙箱）");
    let file = home.join(relay_route_core::MACHINES_REL);
    std::fs::create_dir_all(file.parent().expect("机器表在一个目录里")).unwrap();
    std::fs::write(
        &file,
        r#"{"version": 1, "hosts": [{"label": "startup-probe", "host": "probe.lan", "user": "pi"}]}"#,
    )
    .unwrap();
    let names: Vec<String> = crate::dial::machines_now()
        .iter()
        .map(|h| h.name().to_string())
        .collect();
    let _ = std::fs::remove_file(&file);
    assert_eq!(names, vec!["startup-probe".to_string()]);
}

/// 常驻那一位起来那一段（各台常驻流起来之前那几步）：不调换形（落盘文件的旧版经声明上挂的升级步读，`common/own_state.rs`）。
#[test]
fn the_resident_startup_runs_no_reshape() {
    let feeds = ENTRY
        .find("control::feeds::start(")
        .expect("找不到各台常驻流起来那一步 —— 本条要跟着改");
    let at = ENTRY[..feeds]
        .rfind("if resident {")
        .expect("找不到常驻那一段 —— 本条要跟着改");
    let calls: Vec<&str> = ENTRY[at..feeds]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    assert!(calls.is_empty(), "常驻起来那一段多调了别的：{calls:?}");
}

/// ★ 一条流写不出去了（对端写完一行就关的那一种：答题 hook 只交不等）⇒ 读循环照样把已收的那一行读完、派出去，再收场；
/// 不在写方向先结束那一下把读循环掐掉。
#[tokio::test]
async fn a_stream_whose_writer_ends_first_still_reads_what_came_in() {
    let (gate_tx, gate_rx) = tokio::sync::oneshot::channel::<()>();
    let (read_tx, read_rx) = tokio::sync::oneshot::channel::<()>();
    // 读循环：等写方向结束那一下之后才轮到它读那一行（同一条线程上，写方向那一头先完）。
    let inbound = tokio::spawn(async move {
        gate_rx.await.ok();
        read_tx.send(()).ok();
    });
    let writer = async move {
        gate_tx.send(()).ok();
        crate::stream::writer::WriterEnd::PeerGone
    };
    super::stream_ends(writer, inbound).await;
    assert!(read_rx.await.is_ok(), "那一行读完、派出去了，没被掐掉");
}

/// ★ 写者不是因为对端走了才停（出方向关了 · 一帧写不出）⇒ 当场收场、掐掉读循环：对端还连着时读循环等不到 EOF，
/// 不能让这条流一直不收场。
#[tokio::test]
async fn a_stream_whose_writer_stops_with_the_peer_still_there_ends_at_once() {
    let inbound = tokio::spawn(std::future::pending::<()>());
    tokio::time::timeout(
        guard_core::wall::CEILING,
        super::stream_ends(async { crate::stream::writer::WriterEnd::Closed }, inbound),
    )
    .await
    .expect("当场收场");
}

/// 判据那一侧的 tap 来源：一直没有东西（这一条只看应答那一路）。
struct QuietTap;

impl crate::stream::tap::TapSource for QuietTap {
    async fn next(&mut self) -> Option<crate::wire::Frame> {
        std::future::pending().await
    }
}

/// ★ stdio 那条收到停机信号时有一条命令在飞 ⇒ 排空期间它做完交的最终应答照样写到对端。
/// 替身：出方向通道的发送端握在 `release` 里（生产是这条流在 watcher 上的那一份：放了 ⇒ watcher 摘掉这条连接、出方向随即关）；
/// 对端是一根内存管道。信号先到 ⇒ 写者照跑一拍（出方向这时若已关，它就照「出方向关了」收尾、关上应答收件口），
/// 在飞那一条随后交它的最终应答 ⇒ 对端要读得到。
#[tokio::test]
async fn a_command_in_flight_at_the_stop_signal_still_gets_its_final_reply() {
    use tokio::io::AsyncBufReadExt as _;
    let (out_tx, out_rx) = crate::observe::watcher::frame_channel(1 << 20);
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<crate::wire::Frame>(8);
    let (peer, ours) = tokio::io::duplex(1 << 16);
    let writer = async move {
        crate::stream::writer::writer_task(
            ours,
            out_rx,
            reply_rx,
            QuietTap,
            Default::default(),
            None,
        )
        .await;
    };
    let stop = std::future::ready(());
    tokio::pin!(writer, stop);
    let mut inbound = tokio::spawn(std::future::pending::<()>());
    let end = super::stdio_ends(writer.as_mut(), stop, &mut inbound, move || drop(out_tx)).await;
    assert!(
        matches!(end, super::StdioEnd::Signalled(_)),
        "信号先到，应回收到停机信号"
    );
    // 排空开始：写者照跑一拍。
    let mut writer_done = false;
    tokio::select! {
        biased;
        () = &mut writer => writer_done = true,
        () = std::future::ready(()) => {}
    }
    // 在飞那一条此刻做完，交最终应答（收件口已关 ⇒ 送不进去，这里不管它，只看对端到没到）。
    let final_reply = <crate::wire::Frame as crate::wire::FrameMake>::ok("in-flight-1");
    let _ = reply_tx.send(final_reply).await;
    let mut peer = tokio::io::BufReader::new(peer);
    let mut got = String::new();
    if !writer_done {
        tokio::time::timeout(guard_core::wall::CEILING, async {
            tokio::select! {
                () = &mut writer => {}
                r = peer.read_line(&mut got) => { r.expect("读对端"); }
            }
        })
        .await
        .expect("应答在期限内写到");
    }
    drop(end);
    assert!(
        got.contains("in-flight-1"),
        "在飞命令的最终应答没写到对端（写者{}）：{got:?}",
        if writer_done {
            "排空一开始就停了"
        } else {
            "还在跑"
        }
    );
}
