use super::*;

#[test]
fn epoch_is_zero() {
    // 1970-01-01 = 0 days since itself
    assert_eq!(days_from_civil(1970, 1, 1), 0);
}

#[test]
fn monotonic_across_month() {
    let jan_31 = days_from_civil(2026, 1, 31);
    let feb_1 = days_from_civil(2026, 2, 1);
    assert_eq!(feb_1 - jan_31, 1);
}

#[test]
fn monotonic_across_year() {
    let dec_31 = days_from_civil(2025, 12, 31);
    let jan_1 = days_from_civil(2026, 1, 1);
    assert_eq!(jan_1 - dec_31, 1);
}

#[test]
fn leap_year_feb_29() {
    // 2024 是闰年，Feb 有 29 天
    let feb_28 = days_from_civil(2024, 2, 28);
    let feb_29 = days_from_civil(2024, 2, 29);
    let mar_1 = days_from_civil(2024, 3, 1);
    assert_eq!(feb_29 - feb_28, 1);
    assert_eq!(mar_1 - feb_29, 1);
}

#[test]
fn atomic_write_json_first_write_creates_file() {
    let tmp = std::env::temp_dir().join(format!(
        "ccm-utils-test-first-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&tmp);
    let v = serde_json::json!({ "a": 1, "b": "hi" });
    atomic_write_json(&tmp, &v).unwrap();
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&tmp).unwrap()).unwrap();
    assert_eq!(on_disk["a"], 1);
    assert_eq!(on_disk["b"], "hi");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn atomic_write_json_replace_keeps_content() {
    let tmp = std::env::temp_dir().join(format!(
        "ccm-utils-test-replace-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    atomic_write_json(&tmp, &serde_json::json!({ "v": 1 })).unwrap();
    atomic_write_json(&tmp, &serde_json::json!({ "v": 2, "extra": "y" })).unwrap();
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&tmp).unwrap()).unwrap();
    assert_eq!(on_disk["v"], 2);
    assert_eq!(on_disk["extra"], "y");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn iso_parse_simple() {
    let ms = parse_iso8601_ms("2026-05-20T15:11:42.345Z").unwrap();
    assert!(ms > 1_700_000_000_000);
    assert!(ms < 2_000_000_000_000);
}

#[test]
fn iso_parse_no_fraction() {
    let ms = parse_iso8601_ms("2026-05-20T15:11:42Z").unwrap();
    assert!(ms > 1_700_000_000_000);
}

#[test]
fn iso_parse_bad_returns_none() {
    assert!(parse_iso8601_ms("not-a-date").is_none());
}

#[test]
fn iso_parse_short_frac_normalizes() {
    // 1 位 frac → 乘 100 to ms
    let a = parse_iso8601_ms("2026-05-20T00:00:00.5Z").unwrap();
    let b = parse_iso8601_ms("2026-05-20T00:00:00.500Z").unwrap();
    assert_eq!(a, b);
}

#[test]
fn iso_parse_long_frac_truncates() {
    // 6 位 frac → 除 1000 to ms（剥 us）
    let a = parse_iso8601_ms("2026-05-20T00:00:00.123456Z").unwrap();
    let b = parse_iso8601_ms("2026-05-20T00:00:00.123Z").unwrap();
    assert_eq!(a, b);
}

#[test]
fn now_ms_increases_monotonically() {
    let a = now_ms();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = now_ms();
    assert!(b >= a);
}

#[test]
fn scan_dir_jsons_empty_when_missing() {
    let dir = std::env::temp_dir().join(format!("ccm-scan-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let m: std::collections::HashMap<String, serde_json::Value> =
        scan_dir_jsons(&dir, |v: &serde_json::Value| {
            v.get("k")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string()
        });
    assert!(m.is_empty());
}

#[test]
fn scan_dir_jsons_parses_jsons_only() {
    let dir = std::env::temp_dir().join(format!(
        "ccm-scan-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.json"), r#"{"k":"one"}"#).unwrap();
    std::fs::write(dir.join("b.json"), r#"{"k":"two"}"#).unwrap();
    std::fs::write(dir.join("ignored.txt"), "not json").unwrap();
    std::fs::write(dir.join("broken.json"), "not valid json").unwrap();
    let m: std::collections::HashMap<String, serde_json::Value> =
        scan_dir_jsons(&dir, |v: &serde_json::Value| {
            v.get("k")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string()
        });
    assert_eq!(m.len(), 2);
    assert!(m.contains_key("one"));
    assert!(m.contains_key("two"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn atomic_write_json_no_stray_tmp() {
    // 写完 dst 父目录里不应该有任何 ccm-tmp-* 残留
    let dir = std::env::temp_dir().join(format!("ccm-utils-test-tmpcheck-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("a.json");
    let _ = std::fs::remove_file(&dst);
    atomic_write_json(&dst, &serde_json::json!({ "k": 1 })).unwrap();
    let stray = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains(".ccm-tmp-"));
    assert!(!stray, "ccm-tmp- 残留未清理");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn base64_rfc4648_vectors() {
    // RFC 4648 §10 标准测试向量 —— 守护 base64 算法 + 三种填充情形
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    // 经典 base64 教科书向量
    assert_eq!(base64_encode(b"Man"), "TWFu");
}

#[test]
fn powershell_encoded_command_matches_known() {
    // PowerShell: [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes("echo hi"))
    // "echo hi" → UTF-16LE → base64。守护"UTF-16LE 而非 UTF-8"这个关键约束。
    assert_eq!(
        powershell_encoded_command("echo hi"),
        "ZQBjAGgAbwAgAGgAaQA="
    );
}

#[test]
fn powershell_encoded_command_is_shell_safe() {
    // 含空格 / 括号 / `;` / `&` / `$` / 引号的命令编码后必须只剩 base64 字符，
    // 这是它能安全穿过 wt.exe / cmd 多层 shell 的前提。
    let nasty = r#"if (Get-Command cc) { cc --resume a; & claude "$x" } else { claude }"#;
    let enc = powershell_encoded_command(nasty);
    assert!(
        enc.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='),
        "encoded command 含非 base64 字符: {enc}"
    );
    assert!(!enc.contains(' ') && !enc.contains(';') && !enc.contains('"'));
}

/// ★★ 〔`K-H2b` `D8 阻-2`，08-29〕**带中转前缀的那一串，编码之后前缀还在。**
///
/// # 它为什么活到第九轮才有人量 —— **一次平台归属的连坐误分类**
///
/// `powershell_encoded_command` **没有 `cfg`，在 Linux 上是真编译的**
///（本文件没有任何 `#[cfg(windows)]`，`cargo test -p monitor --lib` 里它被跑到）。
/// 但它**长在 Windows 那条腿上**（`launch::launch_powershell_window` 是它唯一的生产调用方），
/// 于是被连坐地当成了「Windows ⇒ 判不了」那一族。
/// `D8` 的刀 `D8P36`（体首行把 `$env:ANTHROPIC_BASE_URL=…; ` 那一段剥掉再编码）实测：
/// **全量门禁四个数一格不动（`1328 / 一致 / 493 / 1512`）· `GATE: OK`。**
/// 成因：上面那两条判据喂的是 `"echo hi"` 与那条 `nasty` —— **两条都不带中转前缀**，
/// 一把只剥「以某个字面段开头」的刀在它们身上是恒等变换。
///
/// 🔴 **PM 08-29 由此立的那条落法**（本条是它的第一个兑现）：
/// **凡是标「平台判不了」的，要给出它的 `cfg`；给不出 `cfg` = 它在本平台编译 = 不是判不了。**
///
/// # 量法：一个已知的 base64（**不是**拿本函数自己算一遍去比自己）
///
/// 期望值由**另一套实现**独立算出（`python3 -c
/// "base64.b64encode(s.encode('utf-16-le'))"`，08-29 现打）⇒ 本条不是恒真。
/// 第二格是**非空对照**：把前缀那一段拿掉，同一把编码器必须给出**另一个**值 ——
/// 没有这一格，一条「剥了前缀照样等于期望值」的读数分不出「买到了」还是「x == x」。
///
/// ⚠ **它买不到什么**：Windows 上 `powershell.exe -EncodedCommand` 真的照这个串起了进程
/// —— 那要真机，本件登记在「判不了」里（而那一格**给得出 `cfg`**：调用方
/// `launch::launch_powershell_window` 带 `#[cfg(windows)]`，门禁跑在 Linux ⇒ 不进编译单元）。
#[test]
fn the_relay_prefix_survives_the_powershell_encoding_byte_for_byte() {
    // 形状照 `payload::relay_env_prefix_ps` 的产物 + `local_launch_choice` 的探测形。
    let with_relay = "$env:ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/k-0123456789abcdef'; if (Get-Command cc) { cc } else { claude }";
    // 独立算出的期望值（UTF-16LE → 标准 base64）。
    assert_eq!(
        powershell_encoded_command(with_relay),
        "JABlAG4AdgA6AEEATgBUAEgAUgBPAFAASQBDAF8AQgBBAFMARQBfAFUAUgBMAD0AJwBoAHQAdABwADoALwAvADEAMgA3AC4AMAAuADAALgAxADoAOAA3ADgAOAAvAHMALwBjAGwAYQB1AGQAZQAtAGMAbwBkAGUALwBhAGMAYwB0AC0AYQAvAGsALQAwADEAMgAzADQANQA2ADcAOAA5AGEAYgBjAGQAZQBmACcAOwAgAGkAZgAgACgARwBlAHQALQBDAG8AbQBtAGEAbgBkACAAYwBjACkAIAB7ACAAYwBjACAAfQAgAGUAbABzAGUAIAB7ACAAYwBsAGEAdQBkAGUAIAB9AA==",
        "\n★★ **中转前缀在编码这一跳掉了** —— 刀 `D8P36` 的形状：\n\
             体首行把 `$env:ANTHROPIC_BASE_URL=…; ` 剥掉再编码。\n\
             生产后果（Windows）：wt.exe 那条腿起出来的 claude 进程拿不到 base URL ⇒ 直连官方端点，\n\
             而在 `D8` 实测里**全量门禁四个数一格不动**。"
    );
    // 非空对照：把前缀那一段拿掉，同一把编码器必须给出另一个值。
    let without_relay = "if (Get-Command cc) { cc } else { claude }";
    assert_ne!(
        powershell_encoded_command(with_relay),
        powershell_encoded_command(without_relay),
        "带不带中转前缀编出来是同一串 —— 上面那条相等断言在数一个与前缀无关的东西"
    );
}
