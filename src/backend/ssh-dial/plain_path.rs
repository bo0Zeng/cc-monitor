//! 一段相对路径拆成普通段：本机写面（`root_path::lexical_in_root`）· 远端写围栏（`sftp::fence_lexical`）· 解压条目（`files_extract::entry_rel`）
//! 同一道词法判定，都调这里；各自多出来的那一层（远端「只许两个根」、解压「`.` 剥掉」）留在各自那里。
//! 住拨号核心：后端与手机都链它，本机那两处也在后端里，判定只这一份。

/// `.` 段怎么办：写面与远端拒（让调用方送干净的段），解压剥掉（包里常见 `./a`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dots {
    Reject,
    Strip,
}

/// 不是「根底下的一段」的那几形。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// 以分隔符起头，或（`\` 也算分隔符时）盘符起头。
    Absolute,
    /// 有 `..` 段。
    Parent,
    /// 有 `.` 段（[`Dots::Reject`] 时）。
    Dot,
}

/// 按字节拆：分隔符是 `/`，`backslash_is_sep` 时 `\` 也是（Windows 本机 · zip）。空段剥掉；`.` 照 `dots`；`..` · 绝对 ⇒ 拒。
/// 回剩下的普通段（可以是空的：空串、或全剥光了）。
pub fn plain_segments(
    path: &[u8],
    backslash_is_sep: bool,
    dots: Dots,
) -> Result<Vec<&[u8]>, Fault> {
    let sep = |b: &u8| *b == b'/' || (backslash_is_sep && *b == b'\\');
    if path.first().is_some_and(sep) {
        return Err(Fault::Absolute);
    }
    let mut segs = Vec::new();
    for (i, seg) in path.split(sep).enumerate() {
        match seg {
            b"" => {}
            b"." if dots == Dots::Strip => {}
            b"." => return Err(Fault::Dot),
            b".." => return Err(Fault::Parent),
            s if i == 0
                && backslash_is_sep
                && s.len() >= 2
                && s[1] == b':'
                && s[0].is_ascii_alphabetic() =>
            {
                return Err(Fault::Absolute)
            }
            s => segs.push(s),
        }
    }
    Ok(segs)
}

#[cfg(test)]
#[path = "../../../tests/backend/ssh_dial_plain_path_tests.rs"]
mod tests;
