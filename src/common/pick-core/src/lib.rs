//! **选中这几项 ⇒ 能做什么**（唯一一处）：选中的是哪一种（[`picked_kind`]）· 一张表里这一种给哪几项（[`groups_of`]；桌面那张
//! 住本机后端 `files/menu.rs`，手机文件页那张住手机核心）· 表里有、这一份做不了的为什么（[`blocked`]）· 一件事的字（[`label`]）。
//! 本机后端（桌面文件窗口的菜单成品）与手机嵌的核心都调它，两端不另写 `if`。图标 · 快捷键归各自的出口。
//!
//! 事实照 `files-ls` 的一行填（[`Pick`] · [`PickIn::of_ls`]）：是不是目录 · 链接 · 断链 · 名字读不读得出 · 种类 · 三道门，都是那台后端判好的。
//! 此刻做不到的（那台断着 · 那台说做不到那条命令）也在这里判（[`refused_here`]）。

use cmd_wire::files::{FileClass, FileKind, Gate, Gates, LinkTo, LsEntry, PreviewGate};
use copy_core::copy_text;

/// 一项的事实（`files-ls` 的一行，照 [`PickIn`] 解）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick<'a> {
    pub name: &'a str,
    pub is_dir: bool,
    pub link: bool,
    /// 链接指向目录（点得进去；删 / 复制按链接本身算）。
    pub link_dir: bool,
    pub link_broken: bool,
    /// 名字不是合法 UTF-8。
    pub lossy: bool,
    /// 有损名带着原始字节（寻址得到）。
    pub raw: bool,
    pub class: FileClass,
    pub gates: Option<&'a Gates>,
}

impl Pick<'_> {
    /// 点得进去（目录 · 指向目录的链接）。
    pub fn opens_as_dir(&self) -> bool {
        self.is_dir || self.link_dir
    }

    /// 寻址得到（名字读得出，或带着原始字节）。
    fn named(&self) -> bool {
        !self.lossy || self.raw
    }

    /// 复制得了（行上那颗按钮 · 复制到另一栏 · 改名复制）：名字寻址得到；目录也行（后端整棵复制）。
    pub fn copyable(&self) -> bool {
        self.named()
    }

    /// 查看页开得了（那台给了查看那道门）。
    pub fn viewable(&self) -> bool {
        !self.opens_as_dir() && !self.lossy && self.gates.is_some_and(|g| g.view.is_some())
    }
}

/// 一项自己拥有的那几格（同 [`Pick`]）：从 `files-ls` 那一行认（[`PickIn::of_ls`]）。
#[derive(Debug, Clone, PartialEq)]
pub struct PickIn {
    pub name: String,
    pub is_dir: bool,
    pub link: bool,
    pub link_dir: bool,
    pub link_broken: bool,
    pub lossy: bool,
    pub raw: bool,
    pub class: FileClass,
    pub gates: Option<Gates>,
}

impl PickIn {
    /// 🔴 `files-ls` 的一行 ⇒ 那几格（桌面文件窗口 · 手机交来的原行都走它）。有损看名字那一段的**字节**
    /// （不看转出来的串里有没有 U+FFFD）；线上那一行带着字节，有损名恒寻址得到。
    pub fn of_ls(e: &LsEntry) -> Self {
        let link = e.kind == FileKind::Symlink;
        let lossy = std::str::from_utf8(e.name.bytes()).is_err();
        Self {
            name: String::from_utf8_lossy(e.name.bytes()).into_owned(),
            is_dir: e.kind == FileKind::Dir,
            link,
            link_dir: link && e.link_dir == Some(true),
            link_broken: link && e.link_to == Some(LinkTo::Missing),
            lossy,
            raw: lossy,
            class: e.class,
            gates: e.gates.clone(),
        }
    }

    pub fn pick(&self) -> Pick<'_> {
        Pick {
            name: &self.name,
            is_dir: self.is_dir,
            link: self.link,
            link_dir: self.link_dir,
            link_broken: self.link_broken,
            lossy: self.lossy,
            raw: self.raw,
            class: self.class,
            gates: self.gates.as_ref(),
        }
    }
}

/// 一件事 · 菜单上的一项 · 断在哪一段 · 此刻那一台的样子：闭集住契约（桌面窗口照回包读同一份类型）。
pub use cmd_wire::files::{Action, Cut, Here, Item};

/// 选中的是哪一种（闭集）—— 表按它查。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picked {
    /// 什么都没选：对这个目录。
    Here,
    Dir,
    /// 指向目录的链接。
    DirLink,
    /// 文本 · 代码 · 没有扩展名那种（编辑；认不认是文本由后端判）。
    Text,
    /// 查看页开得了的图片。
    Image,
    /// 压缩包。
    Archive,
    /// 别的文件（PDF · 解不开的图 · 二进制）。
    File,
    /// 断了的链接：只能改名、看属性、删除。
    BrokenLink,
    /// 名字读不出（有损、没带原始字节）的目录：只能进去。
    NamelessDir,
    /// 名字读不出的文件：什么都做不了。
    Nameless,
    /// 多选，每一项都寻址得到。
    Many,
    /// 多选，混着名字读不出的：什么都做不了（给「删除 3 项」却只删 2 项是选中态与批量那一摞对不上）。
    ManyNameless,
}

/// 一张表：一种选中 ⇒ 几组项（组与组之间一道线）。
pub type Table = &'static [(Picked, &'static [&'static [Item]])];

/// 这张表里这一种选中的那几组（表里没有这一种 ⇒ 空）。
pub fn groups_of(table: Table, kind: Picked) -> Vec<Vec<Item>> {
    table
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, g)| g.iter().map(|g| g.to_vec()).collect())
        .unwrap_or_default()
}

/// 这几项是哪一种。
pub fn picked_kind(picked: &[Pick]) -> Picked {
    match picked {
        [] => Picked::Here,
        [r] if !r.named() => {
            if r.opens_as_dir() {
                Picked::NamelessDir
            } else {
                Picked::Nameless
            }
        }
        [r] if r.link && r.link_broken => Picked::BrokenLink,
        [r] if r.link_dir => Picked::DirLink,
        [r] if r.is_dir => Picked::Dir,
        [r] if r.viewable() => Picked::Image,
        // 指向文件的链接按它指向的那一种（同列表上的图标）；查看页开不了的图（svg · heic …）当文本那一种。
        [r] => match r.class {
            FileClass::Text | FileClass::Code | FileClass::Other | FileClass::Image => Picked::Text,
            FileClass::Archive => Picked::Archive,
            _ => Picked::File,
        },
        many if many.iter().all(Pick::named) => Picked::Many,
        _ => Picked::ManyNameless,
    }
}

/// 编辑不了的为什么（目录 · 名字寻址不到 · 那台的编辑门说的）；编辑得了 ⇒ `None`。
pub fn why_not_editable(p: &Pick) -> Option<String> {
    if p.is_dir {
        return Some(copy_text("rsFilewinEditor.notEditable.dir", &[]));
    }
    if p.lossy && !p.raw {
        return Some(copy_text("rsFilewinEditor.notEditable.badName", &[]));
    }
    match p.gates.map(|g| &g.edit) {
        None => Some(copy_text("rsFilewinSource.gates.unlisted", &[])),
        Some(Gate::No(said)) => Some(said.clone()),
        Some(Gate::Cap(_)) => None,
    }
}

/// 手机预览不了的为什么（那台的预览门说的 · 没给门）；预览得了 ⇒ `None`。
fn why_not_previewable(p: &Pick) -> Option<String> {
    match p.gates.map(|g| &g.preview) {
        Some(PreviewGate::Text(_)) => None,
        Some(PreviewGate::No(said)) => Some(said.clone()),
        _ => Some(copy_text("rsFilewinSource.gates.unlisted", &[])),
    }
}

/// 表里有、但**这一份**做不了的那一件 ⇒ 为什么（灰着、说哪几项为什么；键盘按了说同一句）。
/// 编辑不了 · 选中里有目录不能下载（下载只收文件）· 多选里名字只有原始字节的那几项不能复制到另一台（那一条按路径串寻址）。
pub fn blocked(a: Action, picked: &[Pick]) -> Option<String> {
    let names = |hit: &dyn Fn(&Pick) -> bool| -> Option<String> {
        let v: Vec<&str> = picked.iter().filter(|r| hit(r)).map(|r| r.name).collect();
        (!v.is_empty()).then(|| v.join(&copy_text("kit.text.sep", &[])))
    };
    match (a, picked) {
        (Action::Edit, [r]) => why_not_editable(r),
        // 桌面的查看是看图页（只给开得了的图）；手机的是文本预览，照预览那道门。
        (Action::View, [r]) if !r.viewable() => why_not_previewable(r),
        (Action::Download, _) => names(&|r: &Pick| r.opens_as_dir())
            .map(|n| copy_text("rsFilewinSelect.why.dirDownload", &[("names", &n)])),
        (Action::CrossCopy, [_, _, ..]) => names(&|r: &Pick| r.lossy)
            .map(|n| copy_text("rsFilewinSelect.why.lossyCross", &[("names", &n)])),
        _ => None,
    }
}

/// 这一件落到哪条命令上（那台说做不到它 ⇒ 这一件做不到）。
fn command_of(a: Action) -> Option<&'static str> {
    match a {
        Action::Chmod => Some(<cmd_wire::files::FilesChmodReq as cmd_wire::Call>::NAME),
        _ => None,
    }
}

/// 🔴 **此刻做不到的那一件 ⇒ 为什么（唯一一处）**：那台说做不到那条命令 ⇒ 这台不支持；断着、而这一件要碰那台
/// （进目录除外：进去那一下自己会说断开）⇒ 断在哪一段说哪一段（`copy_core::write_refused`）。桌面菜单 · 键盘 · 手机顶栏与 ⋯ 都问它。
pub fn refused_here(a: Action, here: &Here) -> Option<String> {
    if command_of(a).is_some_and(|c| here.unavailable.iter().any(|u| u == c)) {
        return Some(copy_text("rsFilewinShell.menu.unavailableHere", &[]));
    }
    if a == Action::Open {
        return None;
    }
    copy_core::write_refused(here.offline)
}

/// 🔴 **一件事的字**：本机后端写桌面菜单成品 · 手机顶栏 · ⋯ 都照它。`n` ＝ 选中了几项：下载 · 删除 · 改权限 ·
/// 复制到另一台对多项说「… n 项」，复制到另一栏照 `n` 说拖几项。
pub fn label(a: Action, n: usize) -> String {
    let many = n > 1;
    let n = n.to_string();
    match a {
        Action::Open => copy_text("mobTerms.row.attach", &[]),
        Action::Edit => copy_text("rsFilewinEditor.label.edit", &[]),
        Action::View => copy_text("tabBatch.result.view", &[]),
        Action::Copy => copy_text("rsFilewinCopy.label.copy", &[]),
        Action::Download if many => copy_text("rsFilewinSelect.label.downloadMany", &[("n", &n)]),
        Action::Download => copy_text("rsFilewinDownload.label.download", &[]),
        Action::Size => copy_text("rsFilewinSize.label.size", &[]),
        Action::Extract => copy_text("rsFilewinExtract.label.extract", &[]),
        Action::CrossCopy if many => copy_text("rsFilewinSelect.label.crossMany", &[("n", &n)]),
        Action::CrossCopy => copy_text("rsFilewinCrossCopy.label.cross", &[]),
        Action::Across => copy_text("rsFilewinWorkspace.drag.hint", &[("n", &n)]),
        Action::CopyTo => copy_text("mobFiles.menu.copyTo", &[]),
        Action::Rename => copy_text("rsFilewinWriteops.label.rename", &[]),
        Action::Chmod if many => copy_text("rsFilewinSelect.label.chmodMany", &[("n", &n)]),
        Action::Chmod => copy_text("rsFilewinWriteops.label.chmod", &[]),
        Action::Delete if many => copy_text("rsFilewinSelect.label.deleteMany", &[("n", &n)]),
        Action::Delete => copy_text("rsFilewinWriteops.label.delete", &[]),
        Action::Properties => copy_text("rsFilewinSelect.label.properties", &[]),
    }
}

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

#[cfg(test)]
#[path = "../../../../tests/common/pick-core/pick_core_tests.rs"]
mod tests;
