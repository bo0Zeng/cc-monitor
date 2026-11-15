//! **下载到本机**：那一台的一项（存盘框给落点）或几项（选目录框给目录）—— 都是一组寻址本机的 `transfer-copy`
//! （这一台 → 本机，[`super::transfer`]），一组一行「进度」。
//!
//! - 落点由系统框问（一项 ⇒ 存盘框，盖不盖它自己问过了 ⇒ 同名照盖；几项 ⇒ 选目录框，同名由本机后端探、冲突框问一次）。
//! - 名字：存盘框里那一段没改 ⇒ 不交名字，本机后端照源那一段原始字节落、落本机那一段改成这台认的合法名（有损名也是）；
//!   改了 ⇒ 就用他敲的。
//! - 目录不收（本机后端那张表管：`files-menu` 成品里「下载」那一件灰着说为什么）。

use copy_core::copy_text;

use super::source::Origin;
use cmd_wire::files::WirePath;
use cmd_wire::transfer::CopyFrom;

/// 行上那颗按钮。
pub static DOWNLOAD_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinDownload.label.download", &[]));

/// 下载一项：`there` 那台的 `src`（`shown` ＝ 它显示的名字）落到存盘框给的 `dest`（同名照盖）。拆不出目录与名字 ⇒ `None`。
pub fn one_plan(
    there: &Origin,
    src: WirePath,
    shown: &str,
    dest: &str,
) -> Option<batch_core::Plan> {
    let (dir, name) = split_dest(dest.as_bytes(), crate::platform::BACKSLASH_IS_SEP)?;
    let name = (name.bytes() != shown.as_bytes()).then_some(name);
    let req = super::transfer::copy_req(
        there,
        &Origin::local(),
        vec![CopyFrom { path: src, name }],
        dir,
        Some(cmd_wire::batch::OnExists::Overwrite),
    );
    Some(super::transfer::plan(
        batch_core::Verb::Download,
        &req,
        vec![shown.to_string()],
        dest.to_string(),
        there.to_string(),
    ))
}

/// 下载几项：`there` 那台的 `(路径, 显示名)` 落进本机 `dir`（落名照源那一段；同名问）。
pub fn many_plan(there: &Origin, rows: Vec<(WirePath, String)>, dir: &str) -> batch_core::Plan {
    let (items, names): (Vec<CopyFrom>, Vec<String>) = rows
        .into_iter()
        .map(|(path, name)| (CopyFrom { path, name: None }, name))
        .unzip();
    let req = super::transfer::copy_req(there, &Origin::local(), items, WirePath::from(dir), None);
    super::transfer::plan(
        batch_core::Verb::Download,
        &req,
        names,
        dir.to_string(),
        there.to_string(),
    )
}

/// 本机落点（原始字节）拆成 `(目录, 落名)`：按最后一个分隔符切 —— `/` 恒是，`backslash` ＝ 本机路径也认 `\`（Windows，
/// [`crate::platform::BACKSLASH_IS_SEP`]）。目录是盘根那一级（`/` · `C:\`）⇒ 带着那个分隔符（Windows 上 `C:` 是「那个盘的当前目录」）。
/// 没有分隔符 · 落名是空的 ⇒ `None`。下载落点只经这一处拆。
pub fn split_dest(b: &[u8], backslash: bool) -> Option<(WirePath, WirePath)> {
    let cut = b
        .iter()
        .rposition(|c| *c == b'/' || (backslash && *c == b'\\'))?;
    let (head, name) = (&b[..cut], &b[cut + 1..]);
    if name.is_empty() {
        return None;
    }
    let drive = backslash && head.len() == 2 && head[0].is_ascii_alphabetic() && head[1] == b':';
    let dir = if head.is_empty() || drive {
        &b[..=cut]
    } else {
        head
    };
    Some((WirePath(dir.to_vec()), WirePath(name.to_vec())))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/download_tests.rs"]
mod tests;
