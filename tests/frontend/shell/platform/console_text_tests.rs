//! 住址：主会话 09-29 裁（`第四波记录/P2.md §6` 第 3 条）「加 / 撤两条失败时 stderr 的解码 —— Windows 臂按那台控制台的 OEM 代码页解 …；非 Windows 照旧 UTF-8」。
//! 替身字节逐字节手写（「张三：拒绝访问。」的 936 编码），不由被测代码算；它就是真 PowerShell 在 936 控制台下写的那一段，由末尾那条读数核。

use super::*;

const SAMPLE_TEXT: &str = "张三：拒绝访问。";
/// [`SAMPLE_TEXT`] 在代码页 936 下的字节（手写）。
const SAMPLE_936: &[u8] = &[
    0xD5, 0xC5, 0xC8, 0xFD, 0xA3, 0xBA, 0xBE, 0xDC, 0xBE, 0xF8, 0xB7, 0xC3, 0xCE, 0xCA, 0xA1, 0xA3,
];

/// Windows：936 的替身字节按 936 解回原文。⚠ 本机只交叉编译；这一条在 Windows 上跑（CI 的 windows runner · 真机）。
#[cfg(windows)]
#[test]
fn a_936_console_line_reads_back_as_its_text() {
    assert_eq!(
        from_code_page(SAMPLE_936, 936).as_deref(),
        Some(SAMPLE_TEXT)
    );
    assert_eq!(from_code_page(&[], 936).as_deref(), Some(""));
}

/// 非 Windows：照旧按 UTF-8 解。
#[cfg(not(windows))]
#[test]
fn elsewhere_console_bytes_read_as_utf8() {
    assert_eq!(console_text(SAMPLE_TEXT.as_bytes()), SAMPLE_TEXT);
}

/// 〔P2〕读数，**不在门禁**（要一个 PowerShell；`CCM_PWSH=<程序> cargo test -- --ignored p2_`，那个程序收一个 `.ps1` 路径去跑）。
/// 核替身字节不是编的：PowerShell 把控制台编码设成 936 之后抛一句 [`SAMPLE_TEXT`]，stderr 里恰好有 [`SAMPLE_936`] 那一段、没有它的 UTF-8。
/// 买不到：Windows PowerShell 5.1 真控制台（这里是 PowerShell 7 容器，936 是替身）。
#[test]
#[ignore = "要一个 PowerShell：CCM_PWSH=<收 .ps1 路径的程序> cargo test -- --ignored"]
fn p2_powershell_under_a_936_console_writes_the_sample_bytes_to_stderr() {
    let Ok(pwsh) = std::env::var("CCM_PWSH") else {
        panic!("没给 CCM_PWSH");
    };
    let dir = std::env::temp_dir().join(format!("ccm-p2-oem-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建 tempdir");
    let file = dir.join("err.ps1");
    std::fs::write(
        &file,
        format!("[Console]::OutputEncoding = [Text.Encoding]::GetEncoding(936)\nthrow '{SAMPLE_TEXT}'\n"),
    )
    .unwrap();
    let out = std::process::Command::new(&pwsh).arg(&file).output();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out.expect("起 CCM_PWSH");
    let has = |needle: &[u8]| out.stderr.windows(needle.len()).any(|w| w == needle);
    assert!(!out.status.success(), "那一句 throw 没让它失败");
    assert!(
        has(SAMPLE_936),
        "stderr 里没有 936 那一段：{:?}",
        out.stderr
    );
    assert!(
        !has(SAMPLE_TEXT.as_bytes()),
        "stderr 是 UTF-8 写的 —— 替身不成立"
    );
}
