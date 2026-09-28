//! 〔OSA · `设计/99 §1` V156〕**PowerShell 那几句写法** —— 本机（Windows）起会话载荷与中转前缀只调这里
//! （原各自在 `control/launch_render/{payload,local}.rs` 里手写，逐字搬来，产出逐字节不变）。
//!
//! ⚠ 这一臂只到「编得过」：本机没有 PowerShell，这几句一次都没被 PowerShell 解析过（`K-H2` 那条登记原样延续）。
//! 值的形状由调用方先判过（单引号里无插值，这里不再转义值 —— 照搬原写法）。

/// `$env:VAR='<值>'; `
pub(crate) fn set_env(var: &str, value: &str) -> String {
    format!("$env:{var}='{value}'; ")
}

/// `$env:VAR=$null; `（清掉这个变量）。
pub(crate) fn clear_env(var: &str) -> String {
    format!("$env:{var}=$null; ")
}

/// 有那条命令 ⇒ 跑 `then`，否则跑 `otherwise`。
pub(crate) fn if_command(name: &str, then: &str, otherwise: &str) -> String {
    format!(
        "if (Get-Command {name} -ErrorAction SilentlyContinue) {{ {then} }} else {{ {otherwise} }}"
    )
}

/// `VAR` 已经有值 ⇒ 说一句 `say`（原话；单引号里 `'` 写成 `''`）、不动它；否则设成 `expr`（一个 PowerShell 表达式）。
pub(crate) fn set_unless_set(var: &str, expr: &str, say: &str) -> String {
    let say = say.replace('\'', "''");
    format!("if ($env:{var}) {{ Write-Host '{say}' }} else {{ $env:{var}={expr} }}; ")
}

/// 一个表达式：`'<head>'` ＋ 家目录底下 `rel` 那份文件的内容（现读、去首尾空白）＋ `'<tail>'`。
/// PowerShell 的 `$HOME` 即 `USERPROFILE`（与后端 `door::key_path` 的退路同一个）。
pub(crate) fn home_file_between(head: &str, rel: &str, tail: &str) -> String {
    format!(
        "'{head}' + (Get-Content -Raw -LiteralPath (Join-Path $HOME '{rel}')).Trim() + '{tail}'"
    )
}
