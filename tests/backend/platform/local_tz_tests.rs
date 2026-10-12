use super::*;

/// ★ 这台的时区（IANA 名）：`TZ` 是 IANA 名（可带开头的 `:`）⇒ 它；不是（POSIX 串 · 空）⇒ 看 `/etc/localtime` 指向 zoneinfo 里的哪一份；
/// 都说不出 ⇒ `None`（调用方说「时区不明」，不猜成 UTC）。名字都过一遍时区库：库里没有的不算。
#[test]
fn the_local_zone_name_comes_from_tz_or_the_localtime_link() {
    use std::path::Path;
    assert_eq!(
        iana_of(Some("America/Los_Angeles"), None).as_deref(),
        Some("America/Los_Angeles")
    );
    assert_eq!(
        iana_of(Some(":Asia/Shanghai"), None).as_deref(),
        Some("Asia/Shanghai")
    );
    assert_eq!(
        iana_of(
            Some("EST5EDT,M3.2.0,M11.1.0"),
            Some(Path::new("/usr/share/zoneinfo/Europe/Berlin"))
        )
        .as_deref(),
        Some("Europe/Berlin"),
        "TZ 不是 IANA 名 ⇒ 看 localtime 那条链"
    );
    assert_eq!(
        iana_of(
            None,
            Some(Path::new("../usr/share/zoneinfo/America/New_York"))
        )
        .as_deref(),
        Some("America/New_York")
    );
    assert_eq!(iana_of(Some(""), None), None);
    assert_eq!(
        iana_of(Some("Mars/Olympus"), None),
        None,
        "库里没有的名字不算"
    );
    assert_eq!(iana_of(None, Some(Path::new("/etc/some-file"))), None);
    assert_eq!(iana_of(None, None), None);
}
