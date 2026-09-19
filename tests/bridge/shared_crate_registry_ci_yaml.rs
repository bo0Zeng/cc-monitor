use std::path::Path;

fn repo_root() -> std::path::PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

pub(crate) fn yml() -> String {
    std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("ci.yml 读不到")
}

/// `ci.yml` 的**有效行**（整行注释剔掉）。
///
/// ⚠ 住在这里而不是各判据自己写一遍：`ci.yml` 怎么读、怎么剔注释是**一个事实**（E3）。
/// 08-08 `e2e_gate_registry` 要用同一件事时，`structural_scan` 的「剥注释转换器必须登记」
/// 当场逮住了那份新拷贝 —— 于是把原来住在 `mod tests` 里的 `ci_live_lines` 搬到这里，
/// **是收口不是新增**（共享原语 `guard_core::strip_comment_lines` 接不住这一半：
/// 它认的是 `//` / `/*` 那套 Rust/TS 形态，而 YAML 的注释是 `#`）。
pub(crate) fn live_lines() -> String {
    guard_core::strip_hash_comment_lines(&yml())
}

/// 切出某个顶层 job 的行范围（剔注释）。
///
/// 顶层 job 键的形状是**两个空格 + 名字 + 冒号**（`  daemon:`），下一个同缩进的键即块尾。
/// ⚠ 用它而不是整份 `contains` 的理由见 `ci_actually_runs_the_daemon_four_steps`：
/// 有的步骤命令是别的 job 里某条命令的**子串**，整份查会被盖住。
pub(crate) fn job_block(name: &str) -> String {
    let head = format!("  {name}:");
    let yml = yml();
    let mut out = Vec::new();
    let mut inside = false;
    for line in yml.lines() {
        if line == head {
            inside = true;
            continue;
        }
        if inside {
            // 同缩进的下一个键 = 块尾（两空格开头、非空白第三字符、以冒号结尾）。
            let is_next_key = line.len() > 2
                && line.starts_with("  ")
                && !line.as_bytes()[2].is_ascii_whitespace()
                && line.trim_end().ends_with(':')
                && !line.trim_start().starts_with('#');
            if is_next_key {
                break;
            }
            if !line.trim_start().starts_with('#') {
                out.push(line);
            }
        }
    }
    out.join("\n")
}
