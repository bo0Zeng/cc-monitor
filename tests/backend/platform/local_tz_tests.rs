use super::*;

/// ★ 这台的时区（IANA 名）：`TZ` 是 IANA 名（可带开头的 `:`）⇒ 它；不是（POSIX 串 · 空）⇒ 看 `/etc/localtime` 指向 zoneinfo 里的哪一份；
/// 都说不出 ⇒ `None`（调用方说「时区不明」，不猜成 UTC）。名字都过一遍时区库：库里没有的不算。
#[test]
fn the_local_zone_name_comes_from_tz_or_the_localtime_link() {
    use std::path::Path;
    assert_eq!(
        iana_of(Some("America/Los_Angeles"), None, None).as_deref(),
        Some("America/Los_Angeles")
    );
    assert_eq!(
        iana_of(Some(":Asia/Shanghai"), None, None).as_deref(),
        Some("Asia/Shanghai")
    );
    assert_eq!(
        iana_of(
            Some("EST5EDT,M3.2.0,M11.1.0"),
            Some(Path::new("/usr/share/zoneinfo/Europe/Berlin")),
            None
        )
        .as_deref(),
        Some("Europe/Berlin"),
        "TZ 不是 IANA 名 ⇒ 看 localtime 那条链"
    );
    assert_eq!(
        iana_of(
            None,
            Some(Path::new("../usr/share/zoneinfo/America/New_York")),
            None
        )
        .as_deref(),
        Some("America/New_York")
    );
    assert_eq!(iana_of(Some(""), None, None), None);
    assert_eq!(
        iana_of(Some("Mars/Olympus"), None, None),
        None,
        "库里没有的名字不算"
    );
    assert_eq!(iana_of(None, Some(Path::new("/etc/some-file")), None), None);
    assert_eq!(iana_of(None, None, None), None);
}

/// Windows 那一形：没有 `TZ`、没有 `/etc/localtime` 那条链，系统答得出它的时区（Windows 时区名映射成的 IANA 名）⇒ 那个名，
/// 不落成「说不出」（那时调用方按 `UTC` 排，PST 的人看到的文件时间差 8 小时）。前两样说得出时它们在前；系统答的名也过时区库。
#[test]
fn the_system_answer_names_the_zone_when_tz_and_the_link_say_nothing() {
    use std::path::Path;
    assert_eq!(
        iana_of(None, None, Some("America/Los_Angeles")).as_deref(),
        Some("America/Los_Angeles")
    );
    assert_eq!(
        iana_of(Some("Asia/Tokyo"), None, Some("America/Los_Angeles")).as_deref(),
        Some("Asia/Tokyo"),
        "TZ 说得出 ⇒ 它在前"
    );
    assert_eq!(
        iana_of(
            None,
            Some(Path::new("/usr/share/zoneinfo/Europe/Berlin")),
            Some("America/Los_Angeles")
        )
        .as_deref(),
        Some("Europe/Berlin"),
        "那条链说得出 ⇒ 它在前"
    );
    assert_eq!(
        iana_of(None, None, Some("Pacific Standard Time")),
        None,
        "没映射成 IANA 名的 Windows 时区名不算"
    );
}
