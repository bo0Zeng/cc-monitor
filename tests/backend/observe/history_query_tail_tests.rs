use super::split_tail;

fn meta_of(m: &str) -> (u64, u64) {
    let v: serde_json::Value = serde_json::from_str(m.trim()).unwrap();
    (
        v["total"].as_u64().unwrap(),
        v["tail_from"].as_u64().unwrap(),
    )
}

#[test]
fn tail_splits_and_numbers() {
    let data = b"{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"a\":4}\n";
    let (meta, tail, head) = split_tail(data, 2);
    assert_eq!(meta_of(&meta), (5, 3));
    assert_eq!(tail, b"{\"a\":3}\n{\"a\":4}\n");
    assert_eq!(head, b"{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n");
}

#[test]
fn n_bigger_than_total_is_all_tail() {
    let data = b"{\"a\":0}\n{\"a\":1}\n";
    let (meta, tail, head) = split_tail(data, 500);
    assert_eq!(meta_of(&meta), (2, 0));
    assert_eq!(tail, data.as_slice());
    assert!(head.is_empty());
}

#[test]
fn empty_and_torn_only() {
    let (meta, tail, head) = split_tail(b"", 500);
    assert_eq!(meta_of(&meta), (0, 0));
    assert!(tail.is_empty() && head.is_empty());
    let (meta, tail, head) = split_tail(b"{\"torn", 500);
    assert_eq!(meta_of(&meta), (0, 0));
    assert!(tail.is_empty() && head.is_empty());
}

#[test]
fn blank_lines_not_counted_but_bytes_preserved() {
    let data = b"{\"a\":0}\n\n{\"a\":1}\n{\"a\":2}\n";
    let (meta, tail, head) = split_tail(data, 1);
    assert_eq!(meta_of(&meta), (3, 2));
    assert_eq!(tail, b"{\"a\":2}\n");
    assert_eq!(head, b"{\"a\":0}\n\n{\"a\":1}\n");
}
