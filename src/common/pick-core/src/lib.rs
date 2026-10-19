//! **选中这几项 ⇒ 能做什么**（唯一一处）：选中的是哪一种（[`picked_kind`]）· 每种出口给哪几项（[`table`]，桌面文件窗口的右键菜单
//! 与手机文件页的顶栏 ＋ ⋯ 各一张）· 表里有、这一份做不了的为什么（[`blocked`]）。桌面文件窗口与手机嵌的核心都调它，
//! 两端不另写 `if`。图标 · 快捷键 · 字的排法归各自的出口；手机那一份连字一起出（[`phone_menu`]：每项 `id · text · enabled · why`）。
//!
//! 事实照 `files-ls` 的一行填（[`Pick`] · [`PickIn::of_ls`]）：是不是目录 · 链接 · 断链 · 名字读不读得出 · 种类 · 三道门，都是那台后端判好的。
//! 此刻做不到的（那台断着 · 那台说做不到那条命令）也在这里判（[`refused_here`]）。

use cmd_wire::files::{FileClass, FileKind, Gate, Gates, LinkTo, LsEntry, PreviewGate};
use copy_core::copy_text;

/// 一项的事实（`files-ls` 的一行；桌面借窗口那一行的格，手机照 [`PickIn`] 解）。
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

    /// 拉得下来：是文件（不是目录、不是指向目录的链接）且名字寻址得到（有损但带字节的按字节拉）。
    pub fn downloadable(&self) -> bool {
        !self.opens_as_dir() && self.named()
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
    /// 🔴 `files-ls` 的一行 ⇒ 那几格（桌面文件窗口列一屏 · 手机交来的原行都走它）。有损看名字那一段的**字节**
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

/// 对选中那几项能做的一件事。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// 进这个目录。
    Open,
    /// 编辑这份文本。
    Edit,
    /// 开一个查看页看这张图。
    View,
    /// 零流量复制（「复制为」那个框）。
    Copy,
    /// 下载到本机（桌面往外拖 ·「存到哪儿」；手机落进系统文件夹）。
    Download,
    /// 算大小 —— 一项或多项。
    Size,
    /// 解压到这里 —— 恰好一份压缩包。
    Extract,
    /// 复制到另一台机器 —— 一项或多项。
    CrossCopy,
    /// 复制到另一栏（双栏时）。
    Across,
    /// 复制到同一台的另一个目录（手机没有另一栏）。
    CopyTo,
    Rename,
    Chmod,
    Delete,
    /// 属性（路径 · 大小 · 修改时间 · 权限 · 所有者 · 链接指向）。
    Properties,
}

/// 表里的一项：对选中那几项的一件事 ·「在计划里看」· 对这个目录的那几件 · 全选。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Act(Action),
    /// 在计划里看（单选的那一份归某一格时才有，[`PLAN_GROUP`]）。
    InPlan,
    Mkdir,
    NewFile,
    Upload,
    Term,
    /// 选中这一目录的全部（手机没有 Ctrl+A）。
    SelectAll,
}

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

/// 哪一个出口：桌面文件窗口的右键菜单 · 手机文件页（第一组是顶栏那几颗，第二组是 ⋯ 里那几项）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outlet {
    Desk,
    Phone,
}

use Action as A;

type Table = &'static [(Picked, &'static [&'static [Item]])];

/// 🔴 **桌面那张表**：一种选中 ⇒ 几组项，组与组之间一道线，危险的那一组在最后。算大小只给目录与多选 · 解压只给压缩包 ·
/// 查看给图片 · 编辑给文本。
const DESK: Table = &[
    (
        Picked::Here,
        &[&[Item::Mkdir, Item::NewFile, Item::Upload], &[Item::Term]],
    ),
    (
        Picked::Dir,
        &[
            &[Item::Act(A::Open), Item::Act(A::Copy)],
            &[
                Item::Act(A::Size),
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::DirLink,
        &[
            &[Item::Act(A::Open), Item::Act(A::Copy)],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::Text,
        &[
            &[
                Item::Act(A::Edit),
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::Image,
        &[
            &[
                Item::Act(A::View),
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::Archive,
        &[
            &[
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Extract),
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::File,
        &[
            &[
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Copy),
            ],
            &[
                Item::Act(A::Rename),
                Item::Act(A::Chmod),
                Item::Act(A::Properties),
            ],
            &[Item::Act(A::Delete)],
        ],
    ),
    (
        Picked::BrokenLink,
        &[
            &[Item::Act(A::Rename), Item::Act(A::Properties)],
            &[Item::Act(A::Delete)],
        ],
    ),
    (Picked::NamelessDir, &[&[Item::Act(A::Open)]]),
    (Picked::Nameless, &[]),
    (
        Picked::Many,
        &[
            &[
                Item::Act(A::Download),
                Item::Act(A::CrossCopy),
                Item::Act(A::Across),
            ],
            &[Item::Act(A::Size), Item::Act(A::Chmod)],
            &[Item::Act(A::Delete)],
        ],
    ),
    (Picked::ManyNameless, &[]),
];

/// 手机那一组顶栏：下载 · 删除（下载只收文件：选中里有目录 ⇒ 灰着、说哪几项）。
const BAR: &[Item] = &[Item::Act(A::Download), Item::Act(A::Delete)];
/// 手机选一个文件 ⋯ 里那几项。
const ONE: &[Item] = &[
    Item::Act(A::Rename),
    Item::Act(A::CopyTo),
    Item::Act(A::Chmod),
    Item::SelectAll,
];

/// 手机选一份文本 ⋯ 里那几项（打头是预览 · 编辑）。
const TEXT: &[Item] = &[
    Item::Act(A::View),
    Item::Act(A::Edit),
    Item::Act(A::Rename),
    Item::Act(A::CopyTo),
    Item::Act(A::Chmod),
    Item::SelectAll,
];

/// 🔴 **手机那张表**：第一组顶栏、第二组 ⋯。查看在手机上是文本预览（手机不开看图页）；没有解压 · 算大小 · 另一台 · 另一栏
/// （手机直连每一台，同一台换个目录就是另一栏那件事）。
const PHONE: Table = &[
    (Picked::Here, &[]),
    (Picked::Dir, &[BAR, ONE]),
    (Picked::DirLink, &[BAR, ONE]),
    (Picked::Text, &[BAR, TEXT]),
    (Picked::Image, &[BAR, ONE]),
    (Picked::Archive, &[BAR, ONE]),
    (Picked::File, &[BAR, ONE]),
    (
        Picked::BrokenLink,
        &[
            &[Item::Act(A::Delete)],
            &[Item::Act(A::Rename), Item::SelectAll],
        ],
    ),
    (Picked::NamelessDir, &[&[], &[Item::SelectAll]]),
    (Picked::Nameless, &[&[], &[Item::SelectAll]]),
    (
        Picked::Many,
        &[
            BAR,
            &[Item::SelectAll, Item::Act(A::CopyTo), Item::Act(A::Chmod)],
        ],
    ),
    (Picked::ManyNameless, &[&[], &[Item::SelectAll]]),
];

/// 🔴 **手机点一下那一行做什么**（同桌面双击）：目录 ⇒ 进去 · 文本 ⇒ 预览 · 别的文件 ⇒ 下载；断链 · 名字读不出的文件 ⇒ 不做。
const PHONE_TAP: &[(Picked, Action)] = &[
    (Picked::Dir, A::Open),
    (Picked::DirLink, A::Open),
    (Picked::NamelessDir, A::Open),
    (Picked::Text, A::View),
    (Picked::Image, A::Download),
    (Picked::Archive, A::Download),
    (Picked::File, A::Download),
];

/// 这个出口那张表。
pub fn table(o: Outlet) -> Table {
    match o {
        Outlet::Desk => DESK,
        Outlet::Phone => PHONE,
    }
}

/// 桌面单选的那一份归某一格 ⇒ 多这一组，插在危险那一组之前。
pub const PLAN_GROUP: &[Item] = &[Item::InPlan];

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

/// 这一种选中在这个出口的那几组；`planned` ⇒ 危险那一组之前多 [`PLAN_GROUP`]（只桌面有计划）。
pub fn menu_groups(o: Outlet, kind: Picked, planned: bool) -> Vec<Vec<Item>> {
    let mut groups: Vec<Vec<Item>> = table(o)
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, g)| g.iter().map(|g| g.to_vec()).collect())
        .unwrap_or_default();
    if planned && o == Outlet::Desk {
        let at = groups
            .iter()
            .position(|g| g.contains(&Item::Act(A::Delete)))
            .unwrap_or(groups.len());
        groups.insert(at, PLAN_GROUP.to_vec());
    }
    groups
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
        (!v.is_empty()).then(|| v.join(&copy_text("rsFilewinCopy.batch.listSep", &[])))
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

/// 🔴 **对这几项能做什么**（这个出口）—— 表里这一种的那几件，去掉这一份做不了的。顺序同表。空 ⇒ 什么都做不了。
pub fn actions_for(o: Outlet, picked: &[Pick]) -> Vec<Action> {
    menu_groups(o, picked_kind(picked), false)
        .into_iter()
        .flatten()
        .filter_map(|i| match i {
            Item::Act(a) if blocked(a, picked).is_none() => Some(a),
            _ => None,
        })
        .collect()
}

/// 此刻那一台的样子：断在哪一段（连着 ⇒ `None`）· 那台说做不到的那几条命令（`hello.unavailable`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Here {
    pub offline: Option<Cut>,
    pub unavailable: Vec<String>,
}

/// 断在哪一段：那台机器（在重连 / 离线）· 窗口与 cc-monitor 之间（桌面文件窗口才有；手机没有这一段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    Machine,
    Monitor,
}

/// 这一件落到哪条命令上（那台说做不到它 ⇒ 这一件做不到）。
fn command_of(a: Action) -> Option<&'static str> {
    match a {
        Action::Chmod => Some(<cmd_wire::files::FilesChmodReq as cmd_wire::files::Call>::NAME),
        _ => None,
    }
}

/// 🔴 **此刻做不到的那一件 ⇒ 为什么（唯一一处）**：那台说做不到那条命令 ⇒ 这台不支持；断着、而这一件要碰那台
/// （进目录除外：进去那一下自己会说断开）⇒ 断在哪一段说哪一段（[`write_refused`]）。桌面菜单 · 键盘 · 手机顶栏与 ⋯ 都问它。
pub fn refused_here(a: Action, here: &Here) -> Option<String> {
    if command_of(a).is_some_and(|c| here.unavailable.iter().any(|u| u == c)) {
        return Some(copy_text("rsFilewinShell.menu.unavailableHere", &[]));
    }
    if a == Action::Open {
        return None;
    }
    write_refused(here.offline)
}

/// 🔴 **断着 ⇒ 写类为什么做不了（唯一一处）**：菜单那几件（[`refused_here`]）· 命令栏 · 编辑页的［保存］· 关窗那一问都问它。
/// 那台断着 ⇒ 离线 · 只读；与 cc-monitor 断开 ⇒ 照实说那一段，不说那台离线。连着 ⇒ `None`。
pub fn write_refused(cut: Option<Cut>) -> Option<String> {
    cut.map(|c| match c {
        Cut::Machine => copy_text("rsFilewinChrome.link.readOnly", &[]),
        Cut::Monitor => copy_text("rsFilewinChrome.link.severedReadOnly", &[]),
    })
}

/// 手机那一项：交回的词 · 字 · 能不能点 · 灰着的为什么（手机没有悬停，点了在底部说这一句）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    pub id: &'static str,
    pub text: String,
    pub enabled: bool,
    pub why: Option<String>,
}

/// 手机那一项交回的词。桌面没有的那几项不在手机表里，问到了 ⇒ `None`。
fn phone_id(i: Item) -> Option<&'static str> {
    Some(match i {
        Item::Act(A::Open) => "open",
        Item::Act(A::View) => "preview",
        Item::Act(A::Edit) => "edit",
        Item::Act(A::Download) => "download",
        Item::Act(A::Delete) => "delete",
        Item::Act(A::Rename) => "rename",
        Item::Act(A::CopyTo) => "copyTo",
        Item::Act(A::Chmod) => "chmod",
        Item::SelectAll => "selectAll",
        _ => return None,
    })
}

/// 手机那一项的字（`n` ＝ 选中了几项：⋯ 里改权限对多项说「改权限 · n 项」）。
fn phone_text(i: Item, n: usize) -> String {
    match i {
        Item::Act(A::View) => copy_text("rsFilewinWorkspace.label.preview", &[]),
        Item::Act(A::Edit) => copy_text("rsFilewinEditor.label.edit", &[]),
        Item::Act(A::Download) => copy_text("rsFilewinDownload.label.download", &[]),
        Item::Act(A::Delete) => copy_text("rsFilewinWriteops.label.delete", &[]),
        Item::Act(A::Rename) => copy_text("rsFilewinWriteops.label.rename", &[]),
        Item::Act(A::CopyTo) => copy_text("mobFiles.menu.copyTo", &[]),
        Item::Act(A::Chmod) if n > 1 => {
            copy_text("rsFilewinSelect.label.chmodMany", &[("n", &n.to_string())])
        }
        Item::Act(A::Chmod) => copy_text("rsFilewinWriteops.label.chmod", &[]),
        _ => copy_text("mobFiles.menu.selectAll", &[]),
    }
}

/// 手机文件页选中这几项时交回的：顶栏一组 · ⋯ 一组（每项 `id · text · enabled · why`）· 单选时点一下那一行做什么。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PhoneMenu {
    pub bar: Vec<Entry>,
    pub more: Vec<Entry>,
    pub tap: Option<&'static str>,
}

/// ★ 手机文件页选中这几项（此刻那一台 [`Here`]）⇒ [`PhoneMenu`]。
pub fn phone_menu(picked: &[Pick], here: &Here) -> PhoneMenu {
    let kind = picked_kind(picked);
    let mut groups = menu_groups(Outlet::Phone, kind, false)
        .into_iter()
        .map(|g| {
            g.into_iter()
                .filter_map(|i| {
                    let id = phone_id(i)?;
                    let why = match i {
                        Item::Act(a) => blocked(a, picked).or_else(|| refused_here(a, here)),
                        _ => None,
                    };
                    Some(Entry {
                        id,
                        text: phone_text(i, picked.len()),
                        enabled: why.is_none(),
                        why,
                    })
                })
                .collect::<Vec<_>>()
        });
    let bar = groups.next().unwrap_or_default();
    let more = groups.next().unwrap_or_default();
    let tap = PHONE_TAP
        .iter()
        .find(|(k, _)| *k == kind)
        .and_then(|(_, a)| phone_id(Item::Act(*a)));
    PhoneMenu { bar, more, tap }
}

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

#[cfg(test)]
#[path = "../../../../tests/common/pick-core/pick_core_tests.rs"]
mod tests;
