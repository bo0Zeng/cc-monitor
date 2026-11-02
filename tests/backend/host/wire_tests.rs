//! 托管口的帧：编解码往返 · 认不出的动词不断线 · 坏帧断线。

use super::*;

fn round(f: Frame) {
    let bytes = encode(&f);
    let (back, used) = decode(&bytes).expect("解得开").expect("够一帧");
    assert_eq!(used, bytes.len());
    assert_eq!(back, f);
}

#[test]
fn every_kind_of_frame_survives_a_round_trip() {
    round(Frame::Out(b"\x1b[31mred\x00\xff".to_vec()));
    round(Frame::In(vec![0x02]));
    round(Frame::Ctl(Ctl::Hello {
        proto: PROTO,
        build: "b".into(),
        want: Want::Input,
        size: Some(Size {
            rows: 40,
            cols: 120,
        }),
        who: Who {
            kind: WhoKind::Terminal,
            client: None,
        },
    }));
    round(Frame::Ctl(Ctl::Welcome {
        proto: PROTO,
        id: "h1".into(),
        size: Size { rows: 24, cols: 80 },
        you: 3,
        holder: Some(Holder {
            client: 3,
            kind: WhoKind::Terminal,
            front: None,
            pid: Some(9),
            mine: true,
        }),
        screen: "\x1b[Hhi".into(),
    }));
    round(Frame::Ctl(Ctl::Ended { code: Some(7) }));
    round(Frame::Ctl(Ctl::Capture {
        back: 2000,
        color: true,
    }));
    round(Frame::Ctl(Ctl::Bye));
}

#[test]
fn the_control_verbs_are_kebab_case_on_the_wire() {
    let b = encode(&Frame::Ctl(Ctl::HeldElsewhere { holder: None }));
    let text = String::from_utf8_lossy(&b[5..]).into_owned();
    assert_eq!(text, r#"{"t":"held-elsewhere"}"#);
}

#[test]
fn an_unknown_verb_from_a_newer_peer_reads_as_unknown() {
    let body = br#"{"t":"some-future-verb","x":1}"#;
    let mut bytes = ((body.len() + 1) as u32).to_be_bytes().to_vec();
    bytes.push(KIND_CTL);
    bytes.extend_from_slice(body);
    let (f, _) = decode(&bytes).expect("不断线").expect("一帧");
    assert_eq!(f, Frame::Ctl(Ctl::Unknown));
}

#[test]
fn a_frame_longer_than_the_limit_is_refused() {
    let bytes = (MAX_FRAME.bytes as u32 + 1).to_be_bytes();
    assert!(matches!(decode(&bytes), Err(Bad::TooLong(_))));
}

#[test]
fn an_unknown_kind_is_refused() {
    let bytes = [0, 0, 0, 2, 9, 0];
    assert!(matches!(decode(&bytes), Err(Bad::Kind(9))));
}

#[test]
fn half_a_frame_is_not_yet_a_frame() {
    let bytes = encode(&Frame::Out(b"abcdef".to_vec()));
    assert!(decode(&bytes[..bytes.len() - 1]).expect("不坏").is_none());
}

#[tokio::test]
async fn async_read_and_write_agree_with_encode() {
    let (mut a, mut b) = tokio::io::duplex(1024);
    write_frame(&mut a, &Frame::In(b"x".to_vec()))
        .await
        .expect("写");
    write_frame(&mut a, &Frame::Ctl(Ctl::Take))
        .await
        .expect("写");
    drop(a);
    assert_eq!(
        read_frame(&mut b).await.expect("读"),
        Some(Frame::In(b"x".to_vec()))
    );
    assert_eq!(
        read_frame(&mut b).await.expect("读"),
        Some(Frame::Ctl(Ctl::Take))
    );
    assert_eq!(read_frame(&mut b).await.expect("读"), None);
}
