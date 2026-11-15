//! FRAMES.md 由类型生成、跟提交的那份一致；指纹随帧形变。

use devsched_wire::shape::{fingerprint_of, frames_md, shape_of};
use devsched_wire::{proto, samples};

#[test]
fn committed_frames_md_matches_types() {
    let want = frames_md();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/FRAMES.md");
    if std::env::var("DEVSCHED_WIRE_BLESS").as_deref() == Ok("1") {
        std::fs::write(path, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        have == want,
        "FRAMES.md 跟帧的类型对不上；重写：DEVSCHED_WIRE_BLESS=1 cargo test -p devsched-wire --test frames_md\n--- 生成的 ---\n{want}"
    );
    assert!(have.contains(proto()), "FRAMES.md 里没有指纹");
}

#[test]
fn fingerprint_changes_with_shape() {
    let base = samples();
    let fp = fingerprint_of(&shape_of(&base));
    assert_eq!(fp, proto());
    // 帧里多一个键 ⇒ 帧形变 ⇒ 指纹变
    let mut values: Vec<serde_json::Value> = base
        .iter()
        .map(|f| serde_json::to_value(f).unwrap())
        .collect();
    values[0]
        .as_object_mut()
        .unwrap()
        .insert("extra".into(), serde_json::json!(1));
    let fp2 = fingerprint_of(&devsched_wire::shape::shape_of_values(&values));
    assert_ne!(fp, fp2);
    // 一个键换了类型也变
    let mut values: Vec<serde_json::Value> = base
        .iter()
        .map(|f| serde_json::to_value(f).unwrap())
        .collect();
    let o = values[0].as_object_mut().unwrap();
    let k = o.keys().find(|k| *k != "t").unwrap().clone();
    o.insert(k, serde_json::json!(true));
    assert_ne!(
        fp,
        fingerprint_of(&devsched_wire::shape::shape_of_values(&values))
    );
}

#[test]
fn proto_looks_like_hex() {
    let p = proto();
    assert_eq!(p.len(), 16);
    assert!(p.chars().all(|c| c.is_ascii_hexdigit()));
}
