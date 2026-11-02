//! **算大小** —— 「算目录大小」的窗口那一侧。
//!
//! 右键菜单 / 键盘动作「算大小」（[`super::select::Action::Size`]）：选中的每一项发一趟后端 `files-size`（读族第九条，
//! 在那台机器上走一遍、只回几个数 ——「零流量」），**顺序**发（后端阻塞档，一趟一件），跑完一句话逐项摆出来。
//! 设计住 §2.10。
//!
//! # 本层只有三件
//!
//! ① 发（[`size_remote`]）· ② 把应答解成几个数（[`sized_from_reply`]，缺字段就是解析失败，不补默认值）·
//! ③ 摆一句话（[`outcome_text`]，数原样画：字节数照写、另给一份人读的）。算大小是纯读 ⇒ 跑完**不**重列目录。
//!
//! # 买不到什么
//!
//! - 取消与进度：后端阻塞档一趟做完才回话（与复制同）⇒ 只有「正在那台机器上算 …」一行。
//! - 真远端：合成后端上真跑（线上那一行逐格相等），后端那一侧在本机临时目录上真跑，连起来经真远端没有读数。

use copy_core::copy_text;

/// 菜单上那一项的字。**唯一住址**。
pub static SIZE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinSize.label.size", &[]));

/// 那条线上命令（后端读族第九条）。
pub const CMD_SIZE: &str = <cmd_wire::files::SizeReq as cmd_wire::Call>::NAME;

/// 一趟的往返上限：与递归删同一个上界（走一整棵树、取消不掉 ⇒ 期限是它唯一的上界）。
pub const SIZE_BUDGET: std::time::Duration = super::writeops::TREE_BUDGET;

/// 一项算下来的几个数（后端报的，原样）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sized {
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub links: u64,
    pub skipped_mounts: u64,
    pub unreadable_dirs: u64,
}

/// 一项的结局：成了 · 没成（名字 ＋ 原话）。
pub type SizeResult = Result<Sized, (String, String)>;

/// `files-size` 的回包 → [`Sized`]。缺字段在按回包类型解那一步就是失败（不悄悄当 0：「0 字节」是一个真读数）。
pub fn sized_from_reply(name: &str, d: cmd_wire::files::SizeReply) -> Sized {
    Sized {
        name: name.to_string(),
        bytes: d.bytes,
        files: d.files,
        dirs: d.dirs,
        links: d.links,
        skipped_mounts: d.skipped_mounts,
        unreadable_dirs: d.unreadable_dirs,
    }
}

/// 一项的那一句。字节数照写（后端报的那个数），前面给一份人读的。零值的几格不说。
pub fn size_line(s: &Sized) -> String {
    let mut t = copy_text(
        "rsFilewinSize.outcome.line",
        &[
            ("name", &s.name),
            ("human", &copy_core::size_text(s.bytes)),
            ("bytes", &s.bytes.to_string()),
            ("files", &s.files.to_string()),
            ("dirs", &s.dirs.to_string()),
        ],
    );
    if s.links > 0 {
        t.push_str(&copy_text(
            "rsFilewinSize.outcome.links",
            &[("n", &s.links.to_string())],
        ));
    }
    if s.skipped_mounts > 0 {
        t.push_str(&copy_text(
            "rsFilewinSize.outcome.mounts",
            &[("n", &s.skipped_mounts.to_string())],
        ));
    }
    if s.unreadable_dirs > 0 {
        t.push_str(&copy_text(
            "rsFilewinSize.outcome.unreadable",
            &[("n", &s.unreadable_dirs.to_string())],
        ));
    }
    t
}

/// 一摞的那一句（逐项，按发的顺序；没成的带原话）。
pub fn outcome_text(results: &[SizeResult]) -> String {
    results
        .iter()
        .map(|r| match r {
            Ok(s) => size_line(s),
            Err((name, why)) => copy_text(
                "rsFilewinSize.outcome.failed",
                &[("name", name), ("why", why)],
            ),
        })
        .collect::<Vec<_>>()
        .join(&copy_text("kit.text.sep", &[]))
}

/// 真发一趟（经通道问后端 `files-size`，按 `SizeReq` 发、按 `SizeReply` 解）。
pub async fn size_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    path: &cmd_wire::files::WirePath,
    name: &str,
) -> Result<Sized, String> {
    let req = cmd_wire::files::SizeReq { path: path.clone() };
    let r = super::source::ask_typed(line, origin, &req, SIZE_BUDGET)
        .await
        .map_err(|f| f.said)?;
    Ok(sized_from_reply(name, r))
}

/// 窗口上的样子：在算哪一项 · 上一摞的结局 —— 就是通用的那一格（[`super::trip::Lane`]），算大小没有自己独有的。
pub type SizeBoard = super::trip::Lane<Vec<SizeResult>>;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/size_tests.rs"]
mod tests;
