//! 文件管理线上类型：路径字节两形往返无损、认不得的形状拒。

use super::*;

fn round(b: &[u8]) -> (String, WirePath) {
    let s = serde_json::to_string(&WirePath(b.to_vec())).unwrap();
    let back: WirePath = serde_json::from_str(&s).unwrap();
    (s, back)
}

#[test]
fn utf8_goes_out_as_a_string_and_other_bytes_as_b16_and_both_come_back_unchanged() {
    let (s, back) = round("目录/a.txt".as_bytes());
    assert_eq!(s, "\"目录/a.txt\"");
    assert_eq!(back.0, "目录/a.txt".as_bytes());
    let (s, back) = round(b"odd-\xff.bin");
    assert_eq!(s, r#"{"b16":"6f64642dff2e62696e"}"#);
    assert_eq!(back.0, b"odd-\xff.bin");
    // 是 UTF-8 也可以用 b16 那一形送进来。
    let p: WirePath = serde_json::from_str(r#"{"b16":"6162"}"#).unwrap();
    assert_eq!(p.0, b"ab");
}

#[test]
fn shapes_that_are_not_a_path_are_refused() {
    for bad in [
        "1",
        "null",
        "[]",
        r#"{"b16":"zz"}"#,
        r#"{"b16":"abc"}"#,
        r#"{"b16":"AB"}"#,
        r#"{"hex":"ab"}"#,
        r#"{"b16":"ab","x":1}"#,
        "{}",
    ] {
        assert!(
            serde_json::from_str::<WirePath>(bad).is_err(),
            "`{bad}` 不是一个路径，却解出来了"
        );
    }
}

#[test]
fn a_chunk_always_goes_out_as_b16_even_when_it_is_utf8() {
    let s = serde_json::to_string(&HexBytes(b"hi".to_vec())).unwrap();
    assert_eq!(s, r#"{"b16":"6869"}"#);
    let back: HexBytes = serde_json::from_str(&s).unwrap();
    assert_eq!(back.0, b"hi");
    assert!(serde_json::from_str::<HexBytes>("\"hi\"").is_err());
}

// ── 写的那几条 ─────────────────────────────────────

#[test]
fn a_delete_expect_is_an_empty_dir_mark_or_bytes_and_nothing_else() {
    let e: DeleteExpect = serde_json::from_str(r#"{"empty_dir":true}"#).unwrap();
    assert_eq!(e, DeleteExpect::EmptyDir);
    assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"empty_dir":true}"#);
    let b: DeleteExpect = serde_json::from_str(r#"{"b16":"ff00"}"#).unwrap();
    assert_eq!(b, DeleteExpect::Bytes(WirePath(vec![0xff, 0])));
    assert_eq!(serde_json::to_string(&b).unwrap(), r#"{"b16":"ff00"}"#);
    let s: DeleteExpect = serde_json::from_str(r#""abc""#).unwrap();
    assert_eq!(s, DeleteExpect::Bytes(WirePath::from("abc")));
    for bad in [
        "null",
        "1",
        r#"{"empty_dir":false}"#,
        r#"{"empty_dir":true,"b16":"00"}"#,
        r#"{"b16":"00","x":1}"#,
        "{}",
    ] {
        assert!(
            serde_json::from_str::<DeleteExpect>(bad).is_err(),
            "`{bad}` 不该当成删的 `expect`"
        );
    }
}

#[test]
fn optional_write_flags_refuse_null_but_may_be_left_out() {
    let base = r#""root":"/r","rel":"a""#;
    let ok: DeleteItem = serde_json::from_str(r#"{"rel":"a"}"#).unwrap();
    assert_eq!((ok.recursive, ok.expect), (None, None));
    for k in ["recursive", "expect"] {
        let s = format!("{{\"rel\":\"a\",\"{k}\":null}}");
        assert!(
            serde_json::from_str::<DeleteItem>(&s).is_err(),
            "`{k}: null` 该拒（今天就拒）"
        );
    }
    for k in ["ticket", "on_exists", "decide"] {
        let s = format!("{{\"root\":\"/r\",\"items\":[],\"{k}\":null}}");
        assert!(
            serde_json::from_str::<FilesCopyReq>(&s).is_err(),
            "`{k}: null` 该拒"
        );
    }
    // `single` 历来收 `null`（当没给）。
    let m: FilesMkdirReq = serde_json::from_str(&format!("{{{base},\"single\":null}}")).unwrap();
    assert_eq!(m.single, None);
}

#[test]
fn a_put_must_say_what_it_read_and_null_means_absent() {
    let base = r#""root":"/r","rel":"a","content":"x""#;
    assert!(serde_json::from_str::<FilesPutReq>(&format!("{{{base}}}")).is_err());
    let p: FilesPutReq = serde_json::from_str(&format!("{{{base},\"expect\":null}}")).unwrap();
    assert_eq!(p.expect, None);
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"root":"/r","rel":"a","content":"x","expect":null}"#
    );
}

#[test]
fn a_sha256_expect_is_exactly_one_lowercase_digest() {
    let d = "a".repeat(SHA256_HEX_LEN);
    let ok: Sha256Expect = serde_json::from_str(&format!(r#"{{"sha256":"{d}"}}"#)).unwrap();
    assert_eq!(ok.sha256.0, d);
    for bad in [
        format!(r#"{{"sha256":"{}"}}"#, "A".repeat(SHA256_HEX_LEN)),
        format!(r#"{{"sha256":"{}"}}"#, "a".repeat(SHA256_HEX_LEN - 1)),
        format!(r#"{{"sha256":"{d}","x":1}}"#),
        "{}".to_string(),
        format!(r#""{d}""#),
    ] {
        assert!(
            serde_json::from_str::<Sha256Expect>(&bad).is_err(),
            "`{bad}` 不是摘要形"
        );
    }
}

#[test]
fn a_staged_chunk_always_goes_out_as_b16_and_comes_in_either_shape() {
    // 是 UTF-8 也发 b16（一个控制字符转义成六个，字符串形一块多长就算不准了）。
    let chunk = ChunkBytes(b"a\0".to_vec());
    assert_eq!(serde_json::to_string(&chunk).unwrap(), r#"{"b16":"6100"}"#);
    for s in [r#"{"b16":"6100"}"#, r#""a\u0000""#] {
        let back: ChunkBytes = serde_json::from_str(s).unwrap();
        assert_eq!(back.bytes(), b"a\0", "{s}");
    }
}

/// 四个带大小的回包各带写好的那一格 `size_text`（同 `LsEntry.size_text`），往返不变；命中缺 `size` 就缺 `size_text`。
#[test]
fn every_reply_with_a_size_carries_its_written_size() {
    fn back<T: serde::de::DeserializeOwned + Serialize>(
        json: serde_json::Value,
    ) -> serde_json::Value {
        let v: T =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{json} 解不开：{e}"));
        serde_json::to_value(&v).unwrap()
    }
    let stat = back::<StatReply>(serde_json::json!({
        "path": "/a", "up": "/", "kind": "file", "size": 2048, "size_text": "2.0 KB",
        "readonly": false, "owner": null, "link_target": null,
    }));
    assert_eq!(stat["size_text"], "2.0 KB");
    let size = back::<SizeReply>(serde_json::json!({
        "path": "/a", "bytes": 2048, "size_text": "2.0 KB", "files": 1, "dirs": 0, "links": 0,
        "other": 0, "skipped_mounts": 0, "unreadable_dirs": 0,
    }));
    assert_eq!(size["size_text"], "2.0 KB");
    let tree = back::<FilesTreeReply>(serde_json::json!({
        "path": "/a", "bytes": 2048, "size_text": "2.0 KB", "files": 1, "dirs": 0, "links": 0,
    }));
    assert_eq!(tree["size_text"], "2.0 KB");
    let hit = |size: serde_json::Value, text: Option<&str>| {
        let mut j = serde_json::json!({
            "path": "/a", "kind": "file", "class": "text", "location": "", "size": size,
            "mtime_secs": null, "mtime_text": null, "marks": [],
        });
        if let Some(t) = text {
            j["size_text"] = t.into();
        }
        back::<FilesFindHit>(j)
    };
    assert_eq!(hit(2048.into(), Some("2.0 KB"))["size_text"], "2.0 KB");
    assert!(hit(serde_json::Value::Null, None)
        .get("size_text")
        .is_none());
}

/// 远端路径按字节切只这一份：只认 `/`，尾巴上的 `/` 全去掉，根与空的上一级是根、尾段为空。
#[test]
fn a_remote_path_is_cut_by_bytes_with_every_trailing_slash_dropped() {
    let up = |p: &str| WirePath::parent(p.as_bytes()).0;
    let tail = |p: &str| WirePath::tail(p.as_bytes()).to_vec();
    assert_eq!(up("a//"), b"/");
    assert_eq!(tail("a//"), b"a");
    assert_eq!(up("/"), b"/");
    assert_eq!(tail("/"), b"");
    assert_eq!(up(""), b"/");
    assert_eq!(tail(""), b"");
    assert_eq!(up("a/b//"), b"a");
    assert_eq!(tail("a/b//"), b"b");
    assert_eq!(up("/x/y//"), b"/x");
    assert_eq!(up("/x"), b"/");
    assert_eq!(tail("/srv/a\\b.txt"), b"a\\b.txt", "`\\` 不是分隔符");
    assert_eq!(WirePath::tail(b"/d/odd-\xff"), b"odd-\xff");
    assert_eq!(WirePath::child(b"/a//", b"n").0, b"/a/n");
    assert_eq!(WirePath::child(b"/", b"n").0, b"/n");
    assert_eq!(WirePath::child(b"", b"n").0, b"/n");
    assert_eq!(WirePath::common_dir(b"/x/y/z", b"/x/y//w").0, b"/x/y");
    assert_eq!(WirePath::common_dir(b"/a", b"/b").0, b"/");
}
