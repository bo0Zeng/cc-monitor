use super::split_stream_flags;

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// 流模式起参只剩「我是流模式」那个词 ＋ `--tz` · `--view`：`--stream` 剥干净、什么都不置；
/// 删掉的旧旗标不认（不剥 ⇒ 落到「未知 flag 照常进流」那条通用路，`wants` 里没有它们的位）。
#[test]
fn only_the_stream_word_is_a_stream_flag() {
    let (rest, got) = split_stream_flags(v(&["--stream"])).unwrap();
    assert!(rest.is_empty(), "--stream 没被剥干净 → §26 死循环");
    assert_eq!(got, super::StreamWants::default());
    for old in ["--with-bg", "--tail-only", "--with-pid", "--with-raw"] {
        let (rest, _) = split_stream_flags(v(&["--stream", old])).unwrap();
        assert_eq!(rest, v(&[old]), "{old} 还被当成流旗标剥");
    }
}

/// 流的声明 `--view <声明>`（JSON 原样或 base64）：流模式那一形里连值一起剥、解成这条流的声明；
/// 认不出 ⇒ `Err`（不起流）；打头是子命令时不碰（那是子命令自己的 `--view`，归 CLI 臂解）。
#[test]
fn the_stream_view_flag_is_parsed_in_stream_mode_only() {
    let decl = r#"{"omit":{"session_added":["pid"]}}"#;
    let (rest, got) = split_stream_flags(v(&["--stream", "--view", decl])).unwrap();
    assert!(rest.is_empty(), "{rest:?}");
    assert_eq!(
        got.view,
        super::StreamView::parse(&serde_json::from_str(decl).unwrap()).unwrap()
    );
    assert!(got.view.is_some());
    let b64 = crate::stream::wire::b64_encode(decl.as_bytes());
    let (_, by_b64) = split_stream_flags(v(&["--stream", "--view", &b64])).unwrap();
    assert_eq!(by_b64.view, got.view, "base64 那一形解出来不一样");
    assert!(
        split_stream_flags(v(&[
            "--stream",
            "--view",
            r#"{"omit":{"facts":["usage"]}}"#
        ]))
        .is_err(),
        "点了流上不出的成品却起了流"
    );
    assert!(
        split_stream_flags(v(&["--stream", "--view"])).is_err(),
        "缺值"
    );
    let cli = v(&["--history-read", "--view", decl]);
    let (rest, got) = split_stream_flags(cli.clone()).unwrap();
    assert_eq!(rest, cli, "子命令的 --view 被流模式剥走了");
    assert!(got.view.is_none());
}

/// 看的那一台的时区 `--tz <IANA 名>`：任意位置、连值一起剥掉（不剥 ⇒ 当查询参数 / 当位置参数）；认得的名 ⇒ 那个时区，
/// 缺值 · 认不得 ⇒ UTC（不拒）。一次性 CLI 面与流模式同一个旗标。
#[test]
fn the_viewer_time_zone_flag_is_stripped_with_its_value() {
    use super::{StreamWants, Tz};
    let sh = Tz::named("Asia/Shanghai").expect("时区库里有上海");
    let (rest, got) = split_stream_flags(v(&["--stream", "--tz", "Asia/Shanghai"])).unwrap();
    assert!(rest.is_empty(), "{rest:?}");
    assert_eq!(
        got,
        StreamWants {
            tz: sh.clone(),
            ..Default::default()
        }
    );
    let (rest, got) =
        split_stream_flags(v(&["--quota-read", "--tz", "Asia/Shanghai", "--text"])).unwrap();
    assert_eq!(
        rest,
        v(&["--quota-read", "--text"]),
        "值不许留下来当位置参数"
    );
    assert_eq!(got.tz, sh);
    let (rest, got) = split_stream_flags(v(&["--quota-read", "--tz", "Nowhere/Atlantis"])).unwrap();
    assert_eq!(rest, v(&["--quota-read"]));
    assert_eq!(got.tz, Tz::default(), "认不得 ⇒ UTC");
    let (rest, got) = split_stream_flags(v(&["--quota-read", "--tz", "--text"])).unwrap();
    assert_eq!(
        rest,
        v(&["--quota-read", "--text"]),
        "缺值：下一个旗标不是它的值"
    );
    assert_eq!(got.tz, Tz::default());
    assert_ne!(sh, Tz::default(), "反空真：上海不等于 UTC");
}

/// 查询参数原样保留。
#[test]
fn query_args_pass_through() {
    let (rest, wants) = split_stream_flags(v(&["--read-session", "/p/s.jsonl"])).unwrap();
    assert_eq!(rest, v(&["--read-session", "/p/s.jsonl"]));
    assert_eq!(wants, super::StreamWants::default());
}
