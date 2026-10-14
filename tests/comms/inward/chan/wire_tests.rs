//! 通道的线上词汇：错误 · 流格两套「内 ⇄ 线上」换法逐变体往返、认不出的跳号标签当协议坏了，
//! 拆帧 / 封帧的几种结局，能力事实（`Offer`）那几问。夹具全是合成的。
use super::*;
use std::time::Duration;

fn hop(idx: u8, tag: &'static str, reach: Reach, why: HopFault) -> CallError {
    CallError::Hop {
        at: HopId { idx, tag },
        reach,
        why,
    }
}

/// 每一种 `CallError` 换成线上形状再换回来，原样；`Refused` 那份体走帧体、不进头。
#[test]
fn every_call_error_survives_the_wire() {
    let all = [
        hop(0, "open", Reach::NotSent, HopFault::Unreachable),
        hop(1, "wait", Reach::Unknown, HopFault::Overrun),
        hop(0, "read", Reach::Sent, HopFault::Dropped),
        CallError::Peer {
            why: PeerFault::Unsupported,
        },
        CallError::Peer {
            why: PeerFault::Refused {
                body: Body(b"\x00\xffopaque".to_vec()),
            },
        },
        OursFault::Cancelled.into(),
        OursFault::Misuse.into(),
        OursFault::Broken.into(),
        CallError::Ours {
            why: OursFault::Cancelled,
            runs_on: true,
        },
    ];
    for e in all {
        let (w, body) = err_to_wire(e.clone());
        let head = serde_json::to_string(&w).unwrap();
        assert!(!head.contains("opaque"), "体进了头：{head}");
        let w: WireErr = serde_json::from_str(&head).unwrap();
        assert_eq!(err_from_wire(w, body), e);
    }
}

/// 跳号标签只认那五个：认不出 ⇒ 协议坏了（`Ours{Broken}`），不当新标签收下。
#[test]
fn an_unknown_hop_tag_is_a_broken_protocol_not_a_new_tag() {
    for tag in HOP_TAGS {
        let w = WireErr::Hop {
            idx: 0,
            tag: tag.to_string(),
            reach: Reach::Sent,
            why: HopFault::Dropped,
        };
        assert!(
            matches!(err_from_wire(w, Vec::new()), CallError::Hop { .. }),
            "{tag}"
        );
    }
    let w = WireErr::Hop {
        idx: 0,
        tag: "connect".into(),
        reach: Reach::Sent,
        why: HopFault::Dropped,
    };
    assert_eq!(err_from_wire(w, Vec::new()), OursFault::Broken.into());
    let item = WireItem::Unseen {
        idx: 1,
        tag: "connect".into(),
        why: HopFault::Unreachable,
    };
    assert_eq!(item_from_wire(item, Vec::new()), None);
}

/// 流里每一种格往返原样；`Frame` 与 `Closed{Peer}` 的体走帧体。
#[test]
fn every_stream_item_survives_the_wire() {
    let all = [
        Item::Frame {
            seq: 7,
            body: Body(b"payload".to_vec()),
        },
        Item::Gap {
            from_seq: 3,
            to_seq: Some(9),
        },
        Item::Gap {
            from_seq: 3,
            to_seq: None,
        },
        Item::Unseen {
            at: HopId {
                idx: 1,
                tag: "auth",
            },
            why: HopFault::Unreachable,
        },
        Item::Seen {
            from: Some(Cursor(b"c1".to_vec())),
        },
        Item::Seen { from: None },
        Item::Closed {
            by: By::Peer(Body(b"bye".to_vec())),
        },
        Item::Closed {
            by: By::Ours(OursFault::Misuse),
        },
    ];
    for i in all {
        let (w, body) = item_to_wire(i.clone());
        let w: WireItem = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(item_from_wire(w, body), Some(i));
    }
}

/// 复制详情的通道那几项：传输错带跳号与发没发出，对端拒只给码，本侧撤单而那台撤不动后接 ` runs_on`。
#[test]
fn hop_facts_name_the_hop_only_for_transport_errors() {
    let f = hop(0, "write", Reach::NotSent, HopFault::Overrun).hop_facts();
    assert!(f.not_sent);
    assert_eq!(f.hop.as_deref(), Some("0:write NotSent"));
    assert_eq!(f.code, "Overrun");
    let f = CallError::Peer {
        why: PeerFault::Refused {
            body: Body::default(),
        },
    }
    .hop_facts();
    assert_eq!(
        (f.not_sent, f.hop, f.code.as_str()),
        (false, None, "refused")
    );
    let f = CallError::Peer {
        why: PeerFault::Unsupported,
    }
    .hop_facts();
    assert_eq!(f.code, "unsupported");
    let f = CallError::Ours {
        why: OursFault::Cancelled,
        runs_on: true,
    }
    .hop_facts();
    assert_eq!(f.code, "Cancelled runs_on");
    assert_eq!(
        CallError::from(OursFault::Cancelled).hop_facts().code,
        "Cancelled"
    );
}

fn call_head() -> Head {
    Head::Call {
        id: 5,
        origin: Origin::local(),
        op: "ping".into(),
        left: Duration::from_millis(1500),
        view: None,
    }
}

/// 封一帧再拆回来：头与体原样，体不进 JSON。
#[tokio::test]
async fn a_frame_round_trips_head_and_opaque_body() {
    let mut buf = Vec::new();
    write_frame(&mut buf, &call_head(), b"\x00\x01raw")
        .await
        .unwrap();
    let head_len = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
    assert!(!String::from_utf8_lossy(&buf[4..4 + head_len]).contains("raw"));
    let (head, body) = read_frame(&mut buf.as_slice(), 1024).await.unwrap();
    assert_eq!(head, call_head());
    assert_eq!(body, b"\x00\x01raw");
}

/// 拆帧的几种失败：帧边界上没字节了 ⇒ `Eof`；读到一半断 ⇒ `Io`；超过宿主给的上限 · 头解不出 ⇒ `Bad`。
#[tokio::test]
async fn reading_a_frame_tells_eof_from_truncation_from_garbage() {
    let empty: &[u8] = &[];
    assert!(matches!(
        read_frame(&mut &empty[..], 64).await,
        Err(ReadFault::Eof)
    ));

    let mut buf = Vec::new();
    write_frame(&mut buf, &call_head(), b"body").await.unwrap();
    let cut = &buf[..buf.len() - 2];
    assert!(matches!(
        read_frame(&mut &cut[..], 1024).await,
        Err(ReadFault::Io(_))
    ));

    // 头比上限长：不读那么多字节、当场拒。
    assert!(matches!(
        read_frame(&mut buf.as_slice(), 8).await,
        Err(ReadFault::Bad(_))
    ));

    let mut junk = Vec::new();
    junk.extend_from_slice(&3u32.to_be_bytes());
    junk.extend_from_slice(b"{x}");
    junk.extend_from_slice(&0u32.to_be_bytes());
    assert!(matches!(
        read_frame(&mut junk.as_slice(), 64).await,
        Err(ReadFault::Bad(_))
    ));
}

/// 撤单那一问：对端交了撤单 op 且这一条不在撤不动那一栏 ⇒ 补发；否则一帧都不补。
#[test]
fn withdraw_is_asked_only_when_the_peer_offers_cancel_for_that_op() {
    let ops = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let o = Offer::new(ops(&["ping", WITHDRAW_OP]), Vec::new(), ops(&["files-put"]));
    assert_eq!(o.withdraw("ping"), Withdraw::Asked);
    assert_eq!(o.withdraw("files-put"), Withdraw::NotOffered);
    let old = Offer::new(ops(&["ping"]), Vec::new(), Vec::new());
    assert_eq!(old.withdraw("ping"), Withdraw::NotOffered);
    assert!(o.admits("ping") && !o.admits("pong"));
}

/// typed 命令撤不撤得动看本侧编进来的指纹表（`cmd_wire::shapes::UNCANCELLABLE`；撤不撤得动算进了指纹，
/// 指纹对得上才会发，对上了两边必然一致）；hello 那一栏只说不是 typed 的那几条，它列了 typed 命令也不认。
#[test]
fn typed_commands_take_cancellability_from_the_shapes_table() {
    use cmd_wire::shapes::{SHAPES, UNCANCELLABLE};
    let ops = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let stuck = UNCANCELLABLE
        .first()
        .copied()
        .expect("至少一条 typed 阻塞档");
    let free = SHAPES
        .iter()
        .map(|(c, _)| *c)
        .find(|c| !UNCANCELLABLE.contains(c))
        .expect("至少一条 typed 可撤的");
    let o = Offer::new(
        ops(&[stuck, free, "ping", WITHDRAW_OP]),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(o.withdraw(stuck), Withdraw::NotOffered, "{stuck}");
    assert_eq!(o.withdraw(free), Withdraw::Asked, "{free}");
    let o = Offer::new(
        ops(&[free, "ping", WITHDRAW_OP]),
        Vec::new(),
        ops(&[free, "ping"]),
    );
    assert_eq!(o.withdraw(free), Withdraw::Asked, "typed 命令以指纹表为准");
    assert_eq!(
        o.withdraw("ping"),
        Withdraw::NotOffered,
        "不是 typed 的照 hello 那一栏"
    );
}

/// hello / resync 里那两格：坏项逐项丢、不丢整份；缺格 ⇒ 空。`shapes` 里值不是串的那一项丢掉。
#[test]
fn offer_facts_drop_bad_items_one_by_one() {
    let v = serde_json::json!({
        "unavailable": [
            {"command": "tmux-ls", "code": "no_tmux"},
            {"command": "x"},
            7,
            {"command": "kill", "code": "unsupported"}
        ],
        "uncancellable": ["files-put", 3, "files-delete"],
        "shapes": {"a": "h1", "b": 2}
    });
    let obj = v.as_object().unwrap();
    let (unavailable, uncancellable) = Offer::facts_of(obj);
    assert_eq!(
        unavailable,
        vec![
            ("tmux-ls".to_string(), "no_tmux".to_string()),
            ("kill".to_string(), "unsupported".to_string())
        ]
    );
    assert_eq!(uncancellable, ["files-put", "files-delete"]);
    assert_eq!(
        Offer::shapes_of(obj),
        vec![("a".to_string(), "h1".to_string())]
    );
    let none = serde_json::Map::new();
    assert_eq!(Offer::facts_of(&none), (Vec::new(), Vec::new()));
    assert!(Offer::shapes_of(&none).is_empty());

    let mut o = Offer::new(vec!["tmux-ls".into()], unavailable, Vec::new());
    assert_eq!(o.unavailable("tmux-ls"), Some("no_tmux"));
    o.refresh_facts(Vec::new(), Vec::new());
    assert_eq!(o.unavailable("tmux-ls"), None);
    assert_eq!(o.ops(), ["tmux-ls"]);
}

/// 期限：剩余只减不增、过了就是零；换钟面不改时刻。撤单手柄拨一下，等它的都醒。
#[tokio::test]
async fn budget_and_cancel_token_basics() {
    let b = Budget {
        until: std::time::Instant::now() - Duration::from_millis(1),
        cancel: CancelToken::new(),
    };
    assert_eq!(b.remaining(), Duration::ZERO);
    assert_eq!(b.deadline().into_std(), b.until);
    let t = b.cancel.clone();
    let waiter = tokio::spawn(async move { t.cancelled().await });
    assert!(!b.cancel.is_cancelled());
    b.cancel.cancel();
    tokio::time::timeout(guard_core::wall::CEILING, waiter)
        .await
        .unwrap()
        .unwrap();
    assert!(b.cancel.is_cancelled());
}
