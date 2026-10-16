//! 种类怎么画：后端判好的那一种（`files-ls` · `files-find` 每行的 `class`）→ 图标；名字以点起头 ⇒ 隐藏文件。

use cmd_wire::files::FileClass;
use egui_phosphor::regular as ph;

/// 全部种类（判据逐个对图标）。
pub const ALL: [FileClass; 8] = [
    FileClass::Folder,
    FileClass::Text,
    FileClass::Code,
    FileClass::Image,
    FileClass::Archive,
    FileClass::Pdf,
    FileClass::Link,
    FileClass::Other,
];

/// 名字以 `.` 起头 ⇒ 隐藏文件（列表里淡一级，可以切成不显示）。
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

/// 种类 → 图标。
pub fn icon(k: FileClass) -> &'static str {
    match k {
        FileClass::Folder => ph::FOLDER_SIMPLE,
        FileClass::Text => ph::FILE_TEXT,
        FileClass::Code => ph::FILE_CODE,
        FileClass::Image => ph::FILE_IMAGE,
        FileClass::Archive => ph::FILE_ZIP,
        FileClass::Pdf => ph::FILE_PDF,
        FileClass::Link => ph::LINK_SIMPLE,
        FileClass::Other => ph::FILE,
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/kind_tests.rs"]
mod tests;
