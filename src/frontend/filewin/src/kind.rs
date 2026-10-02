//! 文件种类：列表的图标与「类型」那一列写什么、按类型怎么排 —— 只在这里判（这是怎么画的事，住窗口）。

use copy_core::copy_text;
use egui_phosphor::regular as ph;

use super::source::Listed;

/// 种类（闭集）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Folder,
    Text,
    Code,
    Image,
    Archive,
    Pdf,
    Link,
    Other,
}

/// 全部种类（判据与「类型」列的字逐个对）。
pub const ALL: [Kind; 8] = [
    Kind::Folder,
    Kind::Text,
    Kind::Code,
    Kind::Image,
    Kind::Archive,
    Kind::Pdf,
    Kind::Link,
    Kind::Other,
];

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

/// 扩展名（小写；`.bashrc` 这种只有前导点的不算扩展名）。
pub fn ext_of(name: &str) -> Option<String> {
    let i = name.rfind('.')?;
    (i > 0 && i + 1 < name.len()).then(|| name[i + 1..].to_lowercase())
}

/// 名字以 `.` 起头 ⇒ 隐藏文件（列表里淡一级，可以切成不显示）。
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

/// 一行是哪一种。链接先于目录判（指向目录的链接也画成链接）。
pub fn kind_of(r: &Listed) -> Kind {
    if r.link {
        return Kind::Link;
    }
    if r.is_dir {
        return Kind::Folder;
    }
    if CODE_NAMES.contains(&r.name.to_lowercase().as_str()) {
        return Kind::Code;
    }
    let Some(ext) = ext_of(&r.name) else {
        return Kind::Other;
    };
    let e = ext.as_str();
    if TEXT.contains(&e) {
        Kind::Text
    } else if CODE.contains(&e) {
        Kind::Code
    } else if IMAGE.contains(&e) {
        Kind::Image
    } else if ARCHIVE.contains(&e) {
        Kind::Archive
    } else if e == "pdf" {
        Kind::Pdf
    } else {
        Kind::Other
    }
}

/// 种类 → 图标。
pub fn icon(k: Kind) -> &'static str {
    match k {
        Kind::Folder => ph::FOLDER_SIMPLE,
        Kind::Text => ph::FILE_TEXT,
        Kind::Code => ph::FILE_CODE,
        Kind::Image => ph::FILE_IMAGE,
        Kind::Archive => ph::FILE_ZIP,
        Kind::Pdf => ph::FILE_PDF,
        Kind::Link => ph::LINK_SIMPLE,
        Kind::Other => ph::FILE,
    }
}

/// 种类 → 「类型」列的那个词。
pub fn label(k: Kind) -> String {
    match k {
        Kind::Folder => copy_text("rsFilewinKind.label.folder", &[]),
        Kind::Text => copy_text("rsFilewinKind.label.text", &[]),
        Kind::Code => copy_text("rsFilewinKind.label.code", &[]),
        Kind::Image => copy_text("rsFilewinKind.label.image", &[]),
        Kind::Archive => copy_text("rsFilewinKind.label.archive", &[]),
        Kind::Pdf => copy_text("rsFilewinKind.label.pdf", &[]),
        Kind::Link => copy_text("rsFilewinKind.label.link", &[]),
        Kind::Other => copy_text("rsFilewinKind.label.other", &[]),
    }
}

/// 「类型」列写什么：种类那个词，文件再跟上扩展名（`代码 · RS`）。
pub fn type_text(r: &Listed) -> String {
    let k = kind_of(r);
    match (k, ext_of(&r.name)) {
        (Kind::Folder | Kind::Link | Kind::Pdf, _) | (_, None) => label(k),
        (_, Some(e)) => copy_text(
            "rsFilewinKind.type.withExt",
            &[("kind", &label(k)), ("ext", &e.to_uppercase())],
        ),
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/kind_tests.rs"]
mod tests;
