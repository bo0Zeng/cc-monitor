//! 〔步 24f〕`files/raw.rs` 的判据 —— **钉住「路径一路走字节」这件事**。
//!
//! # 它买到的
//!
//! - 往返**恒等**：有效 UTF-8 与**非** UTF-8 两条路各一趟，回来的字节与去的时候**逐字节相等**。
//! - 两条路**真的分岔**（一条给字符串、一条给十六进制）—— 不然「无损」这件事就只剩一条路在证。
//! - 子串判词的正反两侧都有输入（ASCII 大小写那一档也有反例）。
//! - 「直接子项」那条判词的边界：祖孙不算、目录自己不算、两种分隔符都认。
//!
//! # 它**买不到**的
//!
//! - **跨平台字节对等**判不了：同一个文件名在 Linux 与 Windows 上
//!   `as_encoded_bytes` 给的不是同一串（后者是 WTF-8）。本族只承诺同机往返。
//! - `to_path_buf` 那条 `unsafe` 的前提（字节来自同一平台的 `as_encoded_bytes`）
//!   **机器钉不住** —— 它是协议纪律，模块头注里如实登记过。

use super::*;

/// 一串**不是有效 UTF-8** 的字节。
///
/// `0xff 0xfe` 在 UTF-8 里不可能出现（两个都是非法首字节）⇒ 这一串必走十六进制那条路。
/// ⚠ 用函数现拼而不是写成字面量：`&[u8]` 字面量在源码里读起来像一串数字，
/// 而「它为什么不是有效 UTF-8」是这份夹具的**全部承重理由**，要写在能读到的地方。
fn not_utf8() -> Vec<u8> {
    let mut v = b"leaf-".to_vec();
    v.extend_from_slice(&[0xff, 0xfe]);
    v.extend_from_slice(b"-tail");
    v
}

#[test]
fn a_valid_utf8_path_goes_down_the_string_branch_and_round_trips_byte_for_byte() {
    let bytes = b"/home/somebody/\xe4\xb8\xad\xe6\x96\x87/a.rs".to_vec();
    assert!(
        std::str::from_utf8(&bytes).is_ok(),
        "这份夹具本该是有效 UTF-8 —— 夹具坏了，下面两条断言在测别的东西"
    );
    let v = to_json(&bytes);
    assert!(
        v.is_string(),
        "有效 UTF-8 没走字符串那条路：{v:?} —— 那会让常见情形白花一倍体积"
    );
    assert_eq!(
        from_json(&v),
        Some(bytes),
        "往返之后字节变了 —— 「一路走字节」这句话当场不成立"
    );
}

#[test]
fn a_non_utf8_path_goes_down_the_hex_branch_and_round_trips_byte_for_byte() {
    let bytes = not_utf8();
    assert!(
        std::str::from_utf8(&bytes).is_err(),
        "这份夹具本该**不是**有效 UTF-8 —— 夹具坏了，本条会去测字符串那条路"
    );
    let v = to_json(&bytes);
    assert!(
        v.get(HEX_KEY).is_some(),
        "非 UTF-8 没走十六进制那条路：{v:?}\n\
         🔴 这正是 `设计/60 §2 档②` 那条「库层有损解码，**寻址不到**」——\n\
         一旦在这里解成替换字符，回程拿着那串去找的就是一个不存在的名字。"
    );
    assert_eq!(
        from_json(&v),
        Some(bytes),
        "往返之后字节变了 —— 本族存在的理由之一当场不成立"
    );
}

/// ★★ **反空真**：两条路必须真的分岔。
///
/// 不钉这一格的话，把 [`to_json`] 改成「一律给字符串」之后
/// 上面那条非 UTF-8 的往返会红，但**「两条路」这个结构**本身没有任何东西在证 ——
/// 而本仓反复治的正是「判据看起来在守，其实守的是另一件事」。
#[test]
fn the_two_encodings_are_really_two_different_shapes() {
    let a = to_json(b"plain");
    let b = to_json(&not_utf8());
    assert_ne!(
        std::mem::discriminant(&a),
        std::mem::discriminant(&b),
        "两条路给出了同一种 JSON 形状 —— 那就只剩一条路了"
    );
}

#[test]
fn a_malformed_path_value_is_refused_instead_of_guessed() {
    // 🔴 「尽力而为」在这里是缺陷不是优点：猜错一个字节就是去看另一个文件。
    for bad in [
        serde_json::json!(7),
        serde_json::json!(null),
        serde_json::json!([1, 2, 3]),
        serde_json::json!({"b16": "zz"}),
        serde_json::json!({"b16": "abc"}),
        serde_json::json!({"other": "00"}),
    ] {
        assert_eq!(
            from_json(&bad),
            None,
            "这个形状被接受了，而它不是一个路径：{bad:?}"
        );
    }
}

#[test]
fn the_substring_predicate_has_both_sides() {
    assert!(contains(b"/a/origin.rs", b"origin", false));
    assert!(!contains(b"/a/origin.rs", b"Origin", false));
    assert!(contains(b"/a/origin.rs", b"Origin", true));
    assert!(!contains(b"/a/origin.rs", b"vitest", true));
    // 空针匹配一切（用来数总条目），针比草垛长一律不中。
    assert!(contains(b"abc", b"", false));
    assert!(!contains(b"ab", b"abc", false));
    // 非 ASCII **一律按字节比** —— 这不是「大小写不敏感的搜索」。
    let cjk = "中文".as_bytes();
    assert!(contains(cjk, cjk, true));
}

#[test]
fn direct_child_means_direct_child_and_nothing_else() {
    assert!(is_direct_child(b"/a/b/c", b"/a/b"));
    assert!(is_direct_child(b"C:\\a\\b\\c", b"C:\\a\\b"));
    // 祖孙不算 —— 算进去的话 overlay 会把整棵子树都盖掉，而它只重列了一层。
    assert!(!is_direct_child(b"/a/b/c/d", b"/a/b"));
    // 目录自己不算（它是它父目录的子项）。
    assert!(!is_direct_child(b"/a/b", b"/a/b"));
    // 前缀相同但不是同一个目录。
    assert!(!is_direct_child(b"/a/bb/c", b"/a/b"));
    // 空目录名一律不算 —— 否则它会盖住一切。
    assert!(!is_direct_child(b"/a", b""));
}

#[test]
fn path_bytes_and_to_path_buf_are_two_sides_of_one_coin() {
    let p = std::env::temp_dir().join("ccm-24f-roundtrip");
    let bytes = path_bytes(&p).to_vec();
    assert_eq!(
        to_path_buf(&bytes),
        p,
        "取字节再还原之后不是同一个路径 —— 那条 `unsafe` 的前提就不成立了"
    );
}
