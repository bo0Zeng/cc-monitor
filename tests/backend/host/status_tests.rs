//! 状态行的模板与字段表：解析 · 写成那一行 · 截 · 宽字 · 路径。

use super::*;

fn vals(kv: &[(&str, &str)]) -> Values {
    kv.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_bar_splits_the_template_into_a_left_and_a_right_half() {
    let t = Template::parse("{name} · {cwd}|{time}");
    let v = vals(&[("name", "w1"), ("cwd", "~/a"), ("time", "09:05")]);
    assert_eq!(t.halves(&v), ("w1 · ~/a".to_string(), "09:05".to_string()));
    assert_eq!(t.render(&v, 20), "w1 · ~/a       09:05");
    assert_eq!(width(&t.render(&v, 20)), 20);
}

#[test]
fn without_a_bar_everything_is_the_left_half() {
    let t = Template::parse("[{name}]");
    assert_eq!(t.render(&vals(&[("name", "w1")]), 10), "[w1]");
}

#[test]
fn unknown_keys_stay_as_written_and_are_listed_once() {
    let t = Template::parse("{nmae} {name} {nmae} {x}");
    assert_eq!(t.unknown(), &["nmae".to_string(), "x".to_string()]);
    assert_eq!(t.halves(&vals(&[("name", "w1")])).0, "{nmae} w1 {nmae} {x}");
}

#[test]
fn an_empty_template_leaves_no_line() {
    assert!(!Template::parse("").shows());
    assert!(!Template::parse("   ").shows());
    assert!(Template::parse("{name}").shows());
    assert!(Template::parse(DEFAULT_TEMPLATE).shows());
}

#[test]
fn every_key_in_the_default_template_is_a_field() {
    assert!(Template::parse(DEFAULT_TEMPLATE).unknown().is_empty());
}

#[test]
fn an_empty_value_drops_the_text_after_it_and_the_last_one_the_text_before() {
    let t = Template::parse("{name} · {branch} · {cwd}|{queue} · {time}");
    let v = vals(&[("name", "w1"), ("cwd", "~/a"), ("time", "09:05")]);
    assert_eq!(t.halves(&v), ("w1 · ~/a".to_string(), "09:05".to_string()));
    let queue = copy_text("beQueue.said.waiting", &[("n", "2")]);
    let v = vals(&[("name", "w1"), ("branch", "main"), ("queue", &queue)]);
    assert_eq!(t.halves(&v), ("w1 · main".to_string(), queue.clone()));
}

#[test]
fn the_left_half_is_cut_first_then_the_right() {
    let t = Template::parse("{name}|{time}");
    let v = vals(&[("name", "abcdefghij"), ("time", "09:05")]);
    assert_eq!(t.render(&v, 12), "abcde… 09:05");
    assert_eq!(t.render(&v, 5), "09:05");
    assert_eq!(t.render(&v, 4), "09:…");
}

#[test]
fn wide_characters_take_two_columns() {
    assert_eq!(width("宽字五个格"), 10);
    let t = Template::parse("{holder}|{time}");
    let v = vals(&[("holder", "宽字五个格"), ("time", "09:05")]);
    let line = t.render(&v, 12);
    assert_eq!(width(&line), 12);
    assert_eq!(line, "宽字…  09:05");
}

#[test]
fn the_path_is_written_under_home_and_cut_from_the_left() {
    assert_eq!(path_shown("/home/u", Some("/home/u")), "~");
    assert_eq!(path_shown("/home/u/x/y", Some("/home/u/")), "~/x/y");
    assert_eq!(path_shown("/home/uu/x", Some("/home/u")), "/home/uu/x");
    let long = format!("/home/u/{}", "d".repeat(60));
    let shown = path_shown(&long, Some("/home/u"));
    assert_eq!(width(&shown), PATH_COLS);
    assert!(shown.starts_with('…') && shown.ends_with("ddd"), "{shown}");
}

#[test]
fn every_field_has_its_words_in_the_copy_table_and_keys_do_not_repeat() {
    let mut seen = std::collections::BTreeSet::new();
    for f in FIELDS {
        assert!(seen.insert(f.key), "键重了：{}", f.key);
        let says = (f.says)();
        let sample = (f.sample)();
        assert!(
            !says.starts_with('〔') && !says.is_empty(),
            "说明不在文案表：{}",
            f.key
        );
        assert!(
            !sample.starts_with('〔') && !sample.is_empty(),
            "示例不在文案表：{}",
            f.key
        );
    }
    assert!(field("nope").is_none());
    assert!(Template::parse("{time}").ticks());
    assert!(Template::parse("{name}|{time}").ticks());
    assert!(!Template::parse("{name}").ticks());
    assert!(!Template::parse("{tiem}").ticks());
}

/// 段尾连着几格空 ⇒ 不留悬空的分隔；段头连着几格空同样。
#[test]
fn several_empty_values_in_a_row_leave_no_dangling_separator() {
    let t = Template::parse("{name} · {model} · {ctx}");
    assert_eq!(t.halves(&vals(&[("name", "w1")])).0, "w1");
    assert_eq!(t.halves(&vals(&[("ctx", "35%")])).0, "35%");
    assert_eq!(t.halves(&vals(&[])).0, "");
    let t = Template::parse("[{name} · {model}]");
    assert_eq!(t.halves(&vals(&[("model", "opus")])).0, "[opus]");
}
