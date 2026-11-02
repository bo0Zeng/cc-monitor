//! **按名字判一项是哪一种**（唯一一处）：扩展名表 · 没有扩展名却一看就是代码的那几个名字。
//! 那一种的词（[`label`]）也住这里。后端文件管理（`files/class.rs`：图标 · 「类型」那一列 · 几道门）、聊天附件的出口（`session-attachment-commit` 的回包）与
//! 各家适配层认出的附件（记录里 `said.attachments[].class`）都调它，两端照 [`FileClass`] 画。

use cmd_wire::files::FileClass;

const TEXT: &[&str] = &[
    "txt", "md", "markdown", "log", "csv", "tsv", "rst", "ini", "cfg", "conf", "env", "org", "adoc",
];
const CODE: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "c", "h", "cpp", "hpp", "cc", "cxx",
    "java", "kt", "kts", "swift", "rb", "php", "sh", "bash", "zsh", "fish", "ps1", "psm1", "lua",
    "sql", "html", "htm", "css", "scss", "sass", "less", "json", "jsonl", "toml", "yaml", "yml",
    "xml", "vue", "svelte", "cs", "dart", "r", "pl", "scala", "zig", "nim", "ex", "exs", "hs",
    "ml", "vim", "diff", "patch", "gradle", "cmake", "mk", "proto", "graphql", "tf",
];
const IMAGE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "tif", "tiff", "avif", "heic",
];
const ARCHIVE: &[&str] = &[
    "zip", "tar", "gz", "tgz", "bz2", "tbz", "tbz2", "xz", "txz", "zst", "7z", "rar", "jar", "deb",
    "rpm",
];
/// 没有扩展名、但一看名字就是代码的那几个。
const CODE_NAMES: &[&str] = &[
    "makefile",
    "dockerfile",
    "justfile",
    "rakefile",
    "cmakelists.txt",
];

/// 扩展名（ASCII 小写；`.bashrc` 这种只有前导点的不算；不是合法 UTF-8 ⇒ 不认）。
pub fn ext_of(name: &[u8]) -> Option<String> {
    let i = name.iter().rposition(|&b| b == b'.')?;
    let e = std::str::from_utf8(&name[i + 1..]).ok()?;
    (i > 0 && !e.is_empty()).then(|| e.to_ascii_lowercase())
}

/// 按名字（与是不是目录）判的那一种；链接由调用方定。
pub fn class_of_name(name: &[u8], is_dir: bool) -> FileClass {
    if is_dir {
        return FileClass::Folder;
    }
    if CODE_NAMES
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n.as_bytes()))
    {
        return FileClass::Code;
    }
    let Some(ext) = ext_of(name) else {
        return FileClass::Other;
    };
    let e = ext.as_str();
    if TEXT.contains(&e) {
        FileClass::Text
    } else if CODE.contains(&e) {
        FileClass::Code
    } else if IMAGE.contains(&e) {
        FileClass::Image
    } else if ARCHIVE.contains(&e) {
        FileClass::Archive
    } else if e == "pdf" {
        FileClass::Pdf
    } else {
        FileClass::Other
    }
}

/// 那一种的词（文件管理「类型」那一列 · 聊天附件 chip 上的类别字同一份）。
pub fn label(c: FileClass) -> String {
    use copy_core::copy_text;
    match c {
        FileClass::Folder => copy_text("gridMonitor.fact.dir", &[]),
        FileClass::Text => copy_text("beFilesKind.label.text", &[]),
        FileClass::Code => copy_text("beFilesKind.label.code", &[]),
        FileClass::Image => copy_text("beFilesKind.label.image", &[]),
        FileClass::Archive => copy_text("beFilesKind.label.archive", &[]),
        FileClass::Pdf => copy_text("beFilesKind.label.pdf", &[]),
        FileClass::Link => copy_text("beFilesKind.label.link", &[]),
        FileClass::Other => copy_text("mobDrawer.item.files", &[]),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/common/file_class_tests.rs"]
mod tests;
