//! 一项是哪一种、读得起来的那几道门：图标 · 「类型」那一列 · 按类型排 · 预览 / 看图 / 编辑许读多少、不许时那一句。
//! 只在这里判；文件窗口与手机照画。

use cmd_wire::files::{FileClass, Gate, Gates, PreviewGate, TextPreview};
use copy_core::{copy_text, size_text};

/// 预览当文本读的上限：预览跟着光标走（每挪一下一趟），比编辑上限小得多。超了不读、不截断。
pub const PREVIEW_TEXT_MAX_BYTES: u64 = 64 * 1024;
/// 预览当图读的上限（超了不读）。
pub const PREVIEW_IMAGE_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// 查看页读一张图的上限：一块 256 KiB 一趟，32 MiB 是 128 趟（远端上十几秒到半分钟，再大该下载到本机看）。
pub const VIEW_IMAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// 编辑上限 ＝ 读一趟的天花板：打得开的就存得回（`files-commit-text` 收的也是这个数）。
pub const EDIT_MAX_BYTES: u64 = super::READ_TEXT_MAX_BYTES as u64;

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
/// 两端自己解得开的那几种图（其余的图不当图读）。
const DECODABLE: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];
const ARCHIVE: &[&str] = &[
    "zip", "tar", "gz", "tgz", "bz2", "tbz", "tbz2", "xz", "txz", "zst", "7z", "rar", "jar", "deb",
    "rpm",
];
/// 没有扩展名、但一看名字就是代码的那几个。
const CODE_NAMES: &[&str] = &["makefile", "dockerfile", "justfile", "rakefile", "cmakelists.txt"];

/// 扩展名（ASCII 小写；`.bashrc` 这种只有前导点的不算；不是合法 UTF-8 ⇒ 不认）。
pub fn ext_of(name: &[u8]) -> Option<String> {
    let i = name.iter().rposition(|&b| b == b'.')?;
    let e = std::str::from_utf8(&name[i + 1..]).ok()?;
    (i > 0 && !e.is_empty()).then(|| e.to_ascii_lowercase())
}

/// 按名字（与是不是目录）判的那一种；链接由调用方定（[`class_of_entry`]）。
pub fn class_of_name(name: &[u8], is_dir: bool) -> FileClass {
    if is_dir {
        return FileClass::Folder;
    }
    if CODE_NAMES.iter().any(|n| name.eq_ignore_ascii_case(n.as_bytes())) {
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

/// 列表里一项画哪种图标：链接画它指向的那一种（指向目录 ⇒ 文件夹，其余按名字）。
pub fn class_of_entry(name: &[u8], is_dir: bool, link: bool, link_dir: bool) -> FileClass {
    class_of_name(name, is_dir || (link && link_dir))
}

/// 按「类型」那一列排的键：链接自成一种，再扩展名。
pub fn type_key(name: &[u8], is_dir: bool, link: bool) -> (FileClass, Option<String>) {
    let class = if link {
        FileClass::Link
    } else {
        class_of_name(name, is_dir)
    };
    (class, ext_of(name))
}

fn label(c: FileClass) -> String {
    let key = match c {
        FileClass::Folder => "beFilesKind.label.folder",
        FileClass::Text => "beFilesKind.label.text",
        FileClass::Code => "beFilesKind.label.code",
        FileClass::Image => "beFilesKind.label.image",
        FileClass::Archive => "beFilesKind.label.archive",
        FileClass::Pdf => "beFilesKind.label.pdf",
        FileClass::Link => "beFilesKind.label.link",
        FileClass::Other => "beFilesKind.label.other",
    };
    copy_text(key, &[])
}

/// 「类型」那一列写什么：那一种的词，文件再跟上扩展名（`代码 · RS`）；链接写「链接 → 文件夹」，断的写「链接 · 目标不存在」。
pub fn type_text(name: &[u8], is_dir: bool, link: bool, link_dir: bool, broken: bool) -> String {
    if link {
        if broken {
            return copy_text("beFilesKind.type.linkBroken", &[]);
        }
        return copy_text(
            "beFilesKind.type.linkTo",
            &[("kind", &label(class_of_name(name, link_dir)))],
        );
    }
    let c = class_of_name(name, is_dir);
    match (c, ext_of(name)) {
        (FileClass::Folder | FileClass::Pdf, _) | (_, None) => label(c),
        (_, Some(e)) => copy_text(
            "beFilesKind.type.withExt",
            &[("kind", &label(c)), ("ext", &e.to_uppercase())],
        ),
    }
}

/// 一份文件（不是目录、不是指向目录的链接）读得起来的那几道门。`size` 读不到 ⇒ 按 0 判（读的那一趟后端再判一次）。
pub fn gates_of(name: &[u8], size: u64) -> Gates {
    let class = class_of_name(name, false);
    let ext = ext_of(name);
    let decodable = class == FileClass::Image
        && ext.as_deref().is_some_and(|e| DECODABLE.contains(&e));
    let preview = if decodable {
        if size > PREVIEW_IMAGE_MAX_BYTES {
            PreviewGate::No(copy_text(
                "beFilesOpen.preview.imageTooBig",
                &[
                    ("size", &size_text(size)),
                    ("limit", &size_text(PREVIEW_IMAGE_MAX_BYTES)),
                ],
            ))
        } else {
            PreviewGate::Image(PREVIEW_IMAGE_MAX_BYTES)
        }
    } else if class == FileClass::Image && ext.as_deref() != Some("svg") {
        PreviewGate::No(copy_text("beFilesOpen.preview.imageKind", &[]))
    } else if matches!(class, FileClass::Archive | FileClass::Pdf) {
        PreviewGate::No(copy_text("beFilesOpen.preview.binaryKind", &[]))
    } else if size > PREVIEW_TEXT_MAX_BYTES {
        PreviewGate::No(copy_text(
            "beFilesOpen.preview.tooBig",
            &[
                ("size", &size_text(size)),
                ("limit", &size_text(PREVIEW_TEXT_MAX_BYTES)),
            ],
        ))
    } else {
        let lang = ext.filter(|e| class == FileClass::Code || e == "md" || e == "svg");
        PreviewGate::Text(TextPreview {
            cap: PREVIEW_TEXT_MAX_BYTES,
            lang,
        })
    };
    let view = decodable.then(|| {
        if size > VIEW_IMAGE_MAX_BYTES {
            Gate::No(copy_text(
                "beFilesOpen.view.tooBig",
                &[
                    ("size", &size_text(size)),
                    ("limit", &size_text(VIEW_IMAGE_MAX_BYTES)),
                ],
            ))
        } else {
            Gate::Cap(VIEW_IMAGE_MAX_BYTES)
        }
    });
    // 编辑「太大」报多了多少字节：边界附近两个四舍过的大小会写成同一个数，什么都没说。
    let edit = if size > EDIT_MAX_BYTES {
        Gate::No(copy_text(
            "beFilesOpen.edit.tooBig",
            &[
                ("size", &size_text(size)),
                ("bytes", &size.to_string()),
                ("limit", &size_text(EDIT_MAX_BYTES)),
                ("over", &(size - EDIT_MAX_BYTES).to_string()),
            ],
        ))
    } else {
        Gate::Cap(EDIT_MAX_BYTES)
    };
    Gates {
        preview,
        view,
        edit,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/files/class_tests.rs"]
mod tests;
