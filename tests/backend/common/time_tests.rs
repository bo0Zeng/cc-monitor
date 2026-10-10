//! **公历换算与 ISO8601 时刻** —— 判据。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | T1 | 定点：1970-01-01 = 0 · 1969-12-31 = −1 · 2000-02-29 · 0000-03-01 · −0001-12-31 | 真值 |
//! | T2 | 两个方向互逆（−800 年到 +800 年逐天） | 全量 |
//! | T3 | ISO8601：秒 · 毫秒 · 1/2 位小数补零 · 太短 / 不是数字 ⇒ `None` | 真值 |
//! | T5 | 给人看的时刻：当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年 · 时区偏移跨日 | 真值 |
//! | T6 | 回包出口那一遍：认得的时刻格（闭集）各添一格 `<键>Text`，按层级走到底；别的数 · 已有的字不动 | 真值 |
//! | T7 | 记录 · 轮次 · 子运行那几处的钟面：本地钟秒数 ⇒ `HH:MM`；ISO / 毫秒按那一刻的偏移排；解不出 ⇒ 缺 | 真值 |
//! | T8 | 历史页那几格（本地钟秒数）：分段（今天 · 昨天 · 本周周一起 · 按月 / 往年带年）· 行尾 · 内容头时间段 · 会话内查找的时刻 | 真值 |
//! | T9 | 历史页回包出口那一遍：行添 `atText` · `sectionText` · `spanText`，按各自那一刻的偏移排；没有时刻的格不添 | 真值 |
//! | T10 | 文件的修改时间（本地钟秒数）：列里那一格（今天 `HH:MM` · 今年 `MM-DD` · 往年带年）· 完整那一格 `YYYY-MM-DD HH:MM:SS` | 真值 |
//! | T4 | 换算常量 `719_468` 在后端生产段只住本模块 | 文本，零命中 ＋ 正控 |

use super::*;

#[test]
fn t1_fixed_points() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
    assert_eq!(civil_from_days(20_361), (2025, 9, 30));
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    assert_eq!(civil_from_days(days_from_civil(2000, 2, 29)), (2000, 2, 29));
    assert_eq!(
        days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 28),
        2
    );
    assert_eq!(
        days_from_civil(0, 3, 1) - days_from_civil(0, 2, 28),
        2,
        "0 年是闰年"
    );
    assert_eq!(days_from_civil(0, 1, 1) - days_from_civil(-1, 12, 31), 1);
    assert_eq!(civil_from_days(days_from_civil(-1, 12, 31)), (-1, 12, 31));
}

#[test]
fn t2_round_trip() {
    let lo = days_from_civil(-800, 1, 1);
    let hi = days_from_civil(800, 1, 1);
    for z in lo..hi {
        let (y, m, d) = civil_from_days(z);
        assert_eq!(days_from_civil(y, m, d), z, "{y}-{m}-{d}");
    }
}

#[test]
fn t3_iso8601() {
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.250Z"), Some(1_250));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.2Z"), Some(1_200));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.25Z"), Some(1_250));
    assert_eq!(
        parse_iso8601_ms("2026-10-07T12:34:56.789Z"),
        Some((days_from_civil(2026, 10, 7) * 86_400 + 12 * 3600 + 34 * 60 + 56) * 1000 + 789)
    );
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.123456Z"), Some(123));
    assert_eq!(
        parse_iso8601_ms("2021-01-01T00:00:00Z"),
        Some(1_609_459_200_000)
    );
    assert_eq!(parse_iso8601_ms("garbage"), None);
    assert_eq!(parse_iso8601_ms("2026-10-07T12:34"), None);
    assert_eq!(parse_iso8601_ms("2026-1x-07T12:34:56Z"), None);
}

#[test]
fn t4_the_constant_lives_only_here() {
    let root = crate::guard_support::src_root();
    let needle = "719_468";
    let files = guard_core::scan_tree!(&root, &["rs"]);
    let scanned = files.len();
    let hits: Vec<String> = files
        .iter()
        .filter(|(_, src)| crate::guard_support::production_code(src).contains(needle))
        .map(|(p, _)| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert!(scanned >= 60, "只扫到 {scanned} 份 —— 遍历坏了");
    assert_eq!(
        hits,
        vec!["common/time.rs".to_string()],
        "公历换算又长出了一份"
    );
    let sample = "fn civil(z: i64) { let z = z + 719_468; }";
    assert!(
        crate::guard_support::production_code(sample).contains(needle),
        "针认不出样本"
    );
}

#[test]
fn t5_display_forms_follow_the_day_and_the_year() {
    let now = days_from_civil(2026, 10, 7) * 86_400 + 12 * 3_600;
    assert_eq!(fmt_at(now + 2 * 3_600 + 5 * 60, now, 0), "14:05");
    assert_eq!(fmt_at(now + 86_400, now, 0), "10-08 12:00");
    assert_eq!(fmt_at(now - 300 * 86_400, now, 0), "2025-12-11 12:00");
    // 东八区：UTC 20:00 是本地次日 04:00。
    assert_eq!(fmt_at(now + 8 * 3_600, now, 480), "10-08 04:00");
}

#[test]
fn t6_reply_gets_a_text_next_to_every_time_cell() {
    let now = days_from_civil(2026, 10, 7) * 86_400 + 12 * 3_600;
    let mut v = serde_json::json!({
        "now": now,
        "earliestReturn": {"account": "b", "at": now + 600},
        "accounts": [{
            "seenAt": now - 60,
            "reading": {"resetsAt": now + 86_400, "used": 0.4},
            "slots": [{"slot": "5h", "pct": 40, "resetsAt": now + 3_600}, {"slot": "7d", "pct": 3}],
        }],
        "sessions": {"s": {"account": {"since": now - 120, "history": [{"at": now - 60, "fromResetsAt": now + 60}]}}},
        "count": 3,
    });
    with_texts(&mut v, now, 0);
    assert_eq!(v["earliestReturn"]["atText"], "12:10");
    let a = &v["accounts"][0];
    assert_eq!(a["seenAtText"], "11:59");
    assert_eq!(a["reading"]["resetsAtText"], "10-08 12:00");
    assert_eq!(a["slots"][0]["resetsAtText"], "13:00");
    assert!(
        a["slots"][1].get("resetsAtText").is_none(),
        "没有时刻 ⇒ 不添"
    );
    let acc = &v["sessions"]["s"]["account"];
    assert_eq!(acc["sinceText"], "11:58");
    assert_eq!(acc["history"][0]["atText"], "11:59");
    assert_eq!(acc["history"][0]["fromResetsAtText"], "12:01");
    assert!(
        v.get("nowText").is_none() && v.get("countText").is_none(),
        "闭集之外的数不动"
    );
    assert!(a["reading"].get("usedText").is_none());
}

#[test]
fn t7_clock_face_of_a_record_time() {
    let base = days_from_civil(2026, 10, 7) * 86_400;
    assert_eq!(hm(base + 9 * 3_600 + 5 * 60 + 59), "09:05");
    assert_eq!(hm(base - 60), "23:59", "前一天的最后一分钟");
    // 偏移按那一刻算：东八区 UTC 20:30 ⇒ 次日 04:30（只写钟面，不写日子）。
    assert_eq!(hm(base + 20 * 3_600 + 30 * 60 + 8 * 3_600), "04:30");
    let iso = "2026-10-07T20:30:15.123Z";
    let t = base + 20 * 3_600 + 30 * 60 + 15;
    assert_eq!(
        iso_hm_here(iso).as_deref(),
        Some(hm(local_secs(t)).as_str())
    );
    assert_eq!(ms_hm_here(t * 1_000 + 123), hm(local_secs(t)));
    assert_eq!(hms(base + 7 * 3_600 + 13 * 60 + 20), "07:13:20");
    assert_eq!(secs_hms_here(t), hms(local_secs(t)));
    assert_eq!(iso_hm_here(""), None);
    assert_eq!(iso_hm_here("not a time at all"), None);
}

/// 本地钟上的 年-月-日 时:分 ⇒ 本地钟秒数（判据里当作「已经按偏移排过」的那一格）。
fn lt(y: i64, m: u32, d: u32, h: i64, mi: i64) -> i64 {
    days_from_civil(y, m, d) * 86_400 + h * 3_600 + mi * 60
}

#[test]
fn t8_history_sections_row_times_spans_and_hits() {
    use copy_core::copy_text;
    // 2026-10-07 是周三。
    let now = lt(2026, 10, 7, 15, 30);
    let sec = |t| section_text(t, now);
    assert_eq!(
        sec(lt(2026, 10, 7, 0, 1)),
        copy_text("history.section.today", &[])
    );
    assert_eq!(
        sec(lt(2026, 10, 6, 23, 59)),
        copy_text("history.section.yesterday", &[])
    );
    assert_eq!(
        sec(lt(2026, 10, 5, 8, 0)),
        copy_text("history.section.week", &[])
    );
    assert_eq!(
        sec(lt(2026, 10, 4, 22, 0)),
        copy_text("history.section.month", &[("month", "10")])
    );
    assert_eq!(
        sec(lt(2026, 9, 30, 12, 0)),
        copy_text("history.section.month", &[("month", "9")])
    );
    assert_eq!(
        sec(lt(2025, 12, 31, 12, 0)),
        copy_text(
            "history.section.yearMonth",
            &[("year", "2025"), ("month", "12")]
        )
    );
    // 周一那天：昨天是周日 ⇒ 本周那一段是空的（昨天之前直接按月）。
    let mon = lt(2026, 10, 5, 9, 0);
    assert_eq!(
        section_text(lt(2026, 10, 4, 9, 0), mon),
        copy_text("history.section.yesterday", &[])
    );
    assert_eq!(
        section_text(lt(2026, 10, 3, 9, 0), mon),
        copy_text("history.section.month", &[("month", "10")])
    );

    assert_eq!(row_time(lt(2026, 10, 7, 14, 2), now), "14:02");
    assert_eq!(row_time(lt(2026, 10, 6, 22, 10), now), "10-06 22:10");
    assert_eq!(row_time(lt(2026, 9, 30, 3, 0), now), "09-30");
    assert_eq!(row_time(lt(2025, 9, 30, 3, 0), now), "2025-09-30");

    assert_eq!(
        span_text(lt(2026, 10, 7, 2, 1), lt(2026, 10, 7, 14, 2), now),
        "02:01–14:02"
    );
    assert_eq!(
        span_text(lt(2026, 9, 30, 2, 1), lt(2026, 9, 30, 14, 2), now),
        "09-30 02:01–14:02"
    );
    assert_eq!(
        span_text(lt(2026, 9, 30, 3, 0), lt(2026, 10, 1, 4, 57), now),
        "09-30 03:00 – 10-01 04:57"
    );

    let y = copy_text("history.section.yesterday", &[]);
    assert_eq!(hit_time(lt(2026, 10, 7, 1, 52), now), "01:52");
    assert_eq!(hit_time(lt(2026, 10, 6, 18, 20), now), format!("{y} 18:20"));
    assert_eq!(hit_time(lt(2026, 9, 30, 18, 20), now), "09-30 18:20");
    assert_eq!(hit_time(lt(2025, 9, 30, 18, 20), now), "2025-09-30 18:20");
}

#[test]
fn t9_history_row_gets_its_three_texts() {
    // 东八区（固定偏移）：UTC 2026-10-07 05:00 ⇒ 本地 13:00 · 此刻 UTC 07:30 ⇒ 本地 15:30。
    let east = |t: i64| t + 8 * 3_600;
    let utc = |y, m, d, h, mi| lt(y, m, d, h, mi) * 1_000;
    let (at, section, span) = history_times(
        utc(2026, 10, 7, 5, 0),
        utc(2026, 10, 6, 18, 1),
        utc(2026, 10, 7, 5, 0),
        utc(2026, 10, 7, 7, 30),
        &east,
    );
    assert_eq!(at.0, "13:00");
    assert_eq!(
        section.0,
        copy_core::copy_text("history.section.today", &[])
    );
    assert_eq!(
        span.0, "02:01–13:00",
        "两头各按自己那一刻排：UTC 前一天 18:01 落在本地今天"
    );
}

#[test]
fn t10_file_mtime_short_and_full() {
    // 2023-11-14T22:13:20Z ＝ 1_700_000_000（`date -u -d @1700000000` 现打）。
    let t = 1_700_000_000_i64;
    let today = |y, m, d| days_from_civil(y, m, d);
    assert_eq!(
        mtime_texts(t, today(2026, 10, 5)),
        ("2023-11-14".to_string(), "2023-11-14 22:13:20".to_string())
    );
    // 东八区：跨过午夜进了第二天；同一年 ⇒ 月日；就是今天 ⇒ 时分。
    let east = t + 8 * 3_600;
    assert_eq!(
        mtime_texts(east, today(2023, 1, 1)),
        ("11-15".to_string(), "2023-11-15 06:13:20".to_string())
    );
    assert_eq!(mtime_texts(east, today(2023, 11, 15)).0, "06:13");
    // 西五区：纪元零点往回退进 1969 年。
    assert_eq!(mtime_texts(-5 * 3_600, today(2026, 1, 1)).0, "1969-12-31");
    // 闰日。
    assert_eq!(mtime_texts(951_782_400, today(2000, 3, 1)).0, "02-29");
}
