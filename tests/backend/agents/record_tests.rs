//! 通用记录的线上形状由 `claudecode/record_of_tests.rs` 的金样钉（各类全格 ＋ 最少格）；这里只钉公共格的写法。
use super::*;

#[test]
fn the_common_fields_are_camel_case_and_the_kind_is_t() {
    let r = Record {
        agent: "a".into(),
        id: "i".into(),
        at: None,
        time_text: Some("09:30".into()),
        body: Body::Title {
            text: "x".into(),
            by: TitleBy::User,
        },
    };
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"agent":"a","id":"i","timeText":"09:30","t":"title","text":"x","by":"user"}"#
    );
}
