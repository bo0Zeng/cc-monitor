//! 这台机器的本地时区：那一刻与 UTC 的差（[`offset_secs`]）· 时区叫什么（[`iana_name`]）。后端 · 壳 · 文件窗口都只经这里问。
//!
//! 读法：偏移 POSIX 走 `localtime_r` 的 `tm_gmtoff`（读 `TZ` 与 `/etc/localtime`，同 `date`），Windows 把那一刻交给
//! `SystemTimeToTzSpecificLocalTime`（那一刻是标准时还是夏令时由它按系统时区规则答）；名字 `TZ` → `/etc/localtime` 那条链 →
//! 系统答的名（unix 读 `/etc/timezone`、没有这份就说不出，Windows 经 `iana-time-zone` 把系统时区名映射成 IANA 名）。
//! 读不出 ⇒ `None`，调用方按 UTC 排、不猜。哪个名字算数由调用方给（两个前端看长相 [`looks_iana`]，后端过它的时区库）：本包只管去哪儿找。

/// 时区名最长几个字节（IANA 名今天最长三十出头）：[`iana_name`] 长过它不交；请求信封 `tz` 那一格按它留位子。
pub const TZ_ROOM: usize = 64;

/// 时刻 `t`（unix 秒）那一刻比 UTC 快几秒（东正；夏令时按那一刻算）。
#[cfg(unix)]
#[cfg_attr(
    target_pointer_width = "64",
    expect(
        clippy::useless_conversion,
        reason = "`tm_gmtoff` 是 `c_long`：64 位上就是 `i64`，32 位上不是；转换照写、两边都对"
    )
)]
pub fn offset_secs(t: i64) -> Option<i64> {
    #[cfg(target_pointer_width = "64")]
    let tt: libc::time_t = t;
    #[cfg(not(target_pointer_width = "64"))]
    let tt = libc::time_t::try_from(t).ok()?;
    // SAFETY: `tm` 是纯数据（全零是合法值）；`localtime_r` 只写进我们给的那一份、不留指针。
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let got = unsafe { libc::localtime_r(&tt, &mut tm) };
    (!got.is_null()).then_some(i64::from(tm.tm_gmtoff))
}

/// 时刻 `t`（unix 秒）那一刻比 UTC 快几秒：那一刻的 UTC 钟面交给系统换成本地钟面，两个钟面之差就是偏移。
#[cfg(windows)]
pub fn offset_secs(t: i64) -> Option<i64> {
    /// 1601-01-01（FILETIME 的零点）到 1970-01-01 的秒数。
    const FROM_1601: i64 = 11_644_473_600;
    /// FILETIME 一秒几刻（100 纳秒一刻）。
    const TICKS: i64 = 10_000_000;
    let utc_ticks = t.checked_add(FROM_1601)?.checked_mul(TICKS)?;
    let utc = win::FileTime::of(u64::try_from(utc_ticks).ok()?);
    // SAFETY: 三个结构体逐格对着 Win32 `FILETIME` · `SYSTEMTIME` 写，全零是合法初值；三个函数只读入参、只写进我们给的那一份。
    let mut utc_face: win::SystemTime = unsafe { std::mem::zeroed() };
    let mut local_face: win::SystemTime = unsafe { std::mem::zeroed() };
    let mut local: win::FileTime = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        win::FileTimeToSystemTime(&utc, &mut utc_face) != 0
            && win::SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc_face, &mut local_face)
                != 0
            && win::SystemTimeToFileTime(&local_face, &mut local) != 0
    };
    if !ok {
        return None;
    }
    let local_ticks = i64::try_from(local.ticks()).ok()?;
    Some((local_ticks - utc_ticks) / TICKS)
}

/// 本包不引 `windows-sys`，照 `proc.rs` 手写 `extern "system"`；结构体逐格对着 Win32 `FILETIME` · `SYSTEMTIME`。
#[cfg(windows)]
mod win {
    #[repr(C)]
    pub(super) struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        pub(super) fn of(ticks: u64) -> Self {
            Self {
                low: (ticks & 0xFFFF_FFFF) as u32,
                high: (ticks >> 32) as u32,
            }
        }

        pub(super) fn ticks(&self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }

    /// `SYSTEMTIME`：年 · 月 · 星期 · 日 · 时 · 分 · 秒 · 毫秒，各一个 `WORD`。本包只转交、不读。
    #[repr(C)]
    pub(super) struct SystemTime {
        parts: [u16; 8],
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub(super) fn FileTimeToSystemTime(ft: *const FileTime, st: *mut SystemTime) -> i32;
        /// 第一个参数给空 ⇒ 按系统此刻设的那个时区（连它的夏令时规则）换。
        pub(super) fn SystemTimeToTzSpecificLocalTime(
            zone: *const std::ffi::c_void,
            utc: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
        pub(super) fn SystemTimeToFileTime(st: *const SystemTime, ft: *mut FileTime) -> i32;
    }
}

/// 这台机器的时区叫什么（IANA 名，例：`America/Los_Angeles`）：`TZ` · `/etc/localtime` 指向 zoneinfo 里的哪一份 · 系统答的名，
/// 依次取第一个 `known` 认的（Windows 名 → IANA 名只走这里的系统那一臂）。都说不出 ⇒ `None`：调用方按 UTC 写、说「时区不明」，不猜。
pub fn iana_name(known: impl Fn(&str) -> bool) -> Option<String> {
    let tz = std::env::var("TZ").ok();
    #[cfg(unix)]
    let (link, system) = (std::fs::read_link("/etc/localtime").ok(), etc_timezone());
    #[cfg(not(unix))]
    let (link, system) = (
        None::<std::path::PathBuf>,
        iana_time_zone::get_timezone().ok(),
    );
    iana_of(tz.as_deref(), link.as_deref(), system.as_deref(), known)
}

/// Debian 系的 `/etc/timezone`（一行 IANA 名）：`/etc/localtime` 是拷贝、不是链时靠它。读不出 ⇒ `None`；只读 [`TZ_ROOM`] 多一字节。
#[cfg(unix)]
fn etc_timezone() -> Option<String> {
    use std::io::Read as _;
    let mut line = String::new();
    std::fs::File::open("/etc/timezone")
        .and_then(|f| f.take(TZ_ROOM as u64 + 1).read_to_string(&mut line))
        .ok()?;
    Some(line)
}

/// 像不像 IANA 名（`区/地` 或 `UTC`，只含字母数字与 `_+-/`）：两个前端不带时区库，[`iana_name`] 就交它；
/// 偏移串（`CST-8`）· 文件路径 · 空 · Windows 时区名都不像。
pub fn looks_iana(n: &str) -> bool {
    !n.is_empty()
        && (n == "UTC" || (n.contains('/') && !n.starts_with('/') && !n.ends_with('/')))
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_+-/".contains(&b))
}

/// [`iana_name`] 的判法（量具直接喂）：`TZ`（POSIX 的 `:` 前缀照去）· 那条链 zoneinfo 之后那一段 · 系统答的名（去首尾空白）依次试；
/// 每一样都要 `known` 认、且不长过 [`TZ_ROOM`] 才算，不算就试下一样。
pub(crate) fn iana_of(
    tz: Option<&str>,
    localtime_link: Option<&std::path::Path>,
    system: Option<&str>,
    known: impl Fn(&str) -> bool,
) -> Option<String> {
    let take = |n: &str| (!n.is_empty() && n.len() <= TZ_ROOM && known(n)).then(|| n.to_string());
    if let Some(n) = tz.map(|t| t.strip_prefix(':').unwrap_or(t)).and_then(take) {
        return Some(n);
    }
    let from_link = localtime_link.and_then(|p| p.to_str()).and_then(|link| {
        let at = link.rfind("zoneinfo/")? + "zoneinfo/".len();
        take(&link[at..])
    });
    from_link.or_else(|| system.map(str::trim).and_then(take))
}
