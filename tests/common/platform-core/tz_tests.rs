use crate::tz::{iana_of, looks_iana, TZ_ROOM};
use std::path::Path;

/// ★ 这台的时区（IANA 名）：`TZ` 算数（可带 POSIX 开头的 `:`）⇒ 它（同 C 库与浏览器：`TZ` 盖过系统那一份，偏移也是这么认的）；
/// 不算数（POSIX 偏移串 · 文件路径 · 过长 · 空）⇒ 看 `/etc/localtime` 指向 zoneinfo 里的哪一份；都说不出 ⇒ `None`（调用方按 UTC，不猜）。
/// 「算不算数」由调用方给：两个前端只看长相（[`looks_iana`]），后端过它的时区库。
#[test]
fn the_local_zone_name_comes_from_tz_or_the_localtime_link() {
    assert_eq!(
        iana_of(Some("America/Los_Angeles"), None, None, looks_iana).as_deref(),
        Some("America/Los_Angeles")
    );
    assert_eq!(
        iana_of(Some(":Asia/Shanghai"), None, None, looks_iana).as_deref(),
        Some("Asia/Shanghai")
    );
    assert_eq!(
        iana_of(Some("UTC"), None, None, looks_iana).as_deref(),
        Some("UTC")
    );
    let berlin = Path::new("/usr/share/zoneinfo/Europe/Berlin");
    for odd in [
        "",
        ":",
        "CST-8",
        "EST5EDT,M3.2.0,M11.1.0",
        "/etc/localtime",
        "a b/c",
        &"X/".repeat(40),
    ] {
        assert_eq!(
            iana_of(Some(odd), Some(berlin), None, looks_iana).as_deref(),
            Some("Europe/Berlin"),
            "TZ 不像 IANA 名 ⇒ 看 localtime 那条链：{odd:?}"
        );
    }
    assert_eq!(
        iana_of(
            None,
            Some(Path::new("../usr/share/zoneinfo/America/New_York")),
            None,
            looks_iana
        )
        .as_deref(),
        Some("America/New_York")
    );
    assert_eq!(iana_of(Some(""), None, None, looks_iana), None);
    assert_eq!(
        iana_of(None, Some(Path::new("/etc/some-file")), None, looks_iana),
        None
    );
    assert_eq!(iana_of(None, None, None, looks_iana), None);
}

/// 「算不算数」每一步都问调用方：`TZ` 给的名它不认 ⇒ 接着看那条链，不就此说不出；
/// 它认得的不带斜杠的老名字（`GMT`）照收 —— 长相那一道只是前端的缺省。不长过 [`TZ_ROOM`] 两种都管。
#[test]
fn every_source_is_checked_by_the_callers_own_test() {
    let berlin = Path::new("/usr/share/zoneinfo/Europe/Berlin");
    let library = |n: &str| n != "Mars/Olympus";
    assert_eq!(
        iana_of(Some("Mars/Olympus"), Some(berlin), None, library).as_deref(),
        Some("Europe/Berlin"),
        "库里没有的名字不算，往下一样回落"
    );
    assert_eq!(iana_of(Some("Mars/Olympus"), None, None, library), None);
    assert_eq!(
        iana_of(Some("GMT"), None, None, |_: &str| true).as_deref(),
        Some("GMT")
    );
    assert_eq!(iana_of(Some("GMT"), None, None, looks_iana), None);
    assert_eq!(
        iana_of(Some(&"A".repeat(TZ_ROOM + 1)), None, None, |_: &str| true),
        None,
        "长过 TZ_ROOM 不交"
    );
}

/// 没有 `TZ`、没有 `/etc/localtime` 那条链，系统答得出它的时区（Windows：系统时区名映射成的 IANA 名；Linux：`/etc/timezone` 那一行）⇒ 那个名，
/// 不落成「说不出」（那时调用方按 `UTC` 排，PST 的人看到的文件时间差 8 小时）。前两样说得出时它们在前；系统答的名也一样要算数。
#[test]
fn the_system_answer_names_the_zone_when_tz_and_the_link_say_nothing() {
    assert_eq!(
        iana_of(None, None, Some("America/Los_Angeles"), looks_iana).as_deref(),
        Some("America/Los_Angeles")
    );
    assert_eq!(
        iana_of(None, None, Some("Europe/Paris\n"), looks_iana).as_deref(),
        Some("Europe/Paris"),
        "`/etc/timezone` 那一行带换行"
    );
    assert_eq!(
        iana_of(
            Some("Asia/Tokyo"),
            None,
            Some("America/Los_Angeles"),
            looks_iana
        )
        .as_deref(),
        Some("Asia/Tokyo"),
        "TZ 说得出 ⇒ 它在前"
    );
    assert_eq!(
        iana_of(
            None,
            Some(Path::new("/usr/share/zoneinfo/Europe/Berlin")),
            Some("America/Los_Angeles"),
            looks_iana
        )
        .as_deref(),
        Some("Europe/Berlin"),
        "那条链说得出 ⇒ 它在前"
    );
    assert_eq!(
        iana_of(None, None, Some("Pacific Standard Time"), looks_iana),
        None,
        "没映射成 IANA 名的 Windows 时区名不算"
    );
}

/// 偏移：问得到就是一个整分钟数、落在 ±14 小时里（测试机 `TZ=UTC` 时就是 0）。按那一刻算冬夏这件事这里判不了（改进程的 `TZ` 会串到别的判据）。
#[test]
fn the_offset_is_whole_minutes_within_a_day() {
    let off = crate::tz::offset_secs(1_700_000_000).expect("本机偏移问不到");
    assert_eq!(off % 60, 0);
    assert!(off.abs() <= 14 * 3600, "{off}");
}
