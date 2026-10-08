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

use crate::Held;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use copy_core::copy_text;

/// 菜单上那一项的字。**唯一住址**。
pub static SIZE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinSize.label.size", &[]));

/// 那条线上命令（后端读族第九条）。
pub const CMD_SIZE: &str = "files-size";

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

/// `files-size` 的 `data` → [`Sized`]。**缺字段 ⇒ 失败**（不悄悄当 0：「0 字节」是一个真读数）。
pub fn sized_from_reply(name: &str, d: &serde_json::Value) -> Result<Sized, String> {
    let n = |k: &str| -> Result<u64, String> {
        d.get(k)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| copy_text("rsFilewinSize.reply.missingField", &[("field", k)]))
    };
    Ok(Sized {
        name: name.to_string(),
        bytes: n("bytes")?,
        files: n("files")?,
        dirs: n("dirs")?,
        links: n("links")?,
        skipped_mounts: n("skipped_mounts")?,
        unreadable_dirs: n("unreadable_dirs")?,
    })
}

/// 一项的那一句。字节数照写（后端报的那个数），前面给一份人读的。零值的几格不说。
pub fn size_line(s: &Sized) -> String {
    let mut t = copy_text(
        "rsFilewinSize.outcome.line",
        &[
            ("name", &s.name),
            ("human", &super::rows::human_size(s.bytes)),
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
        .join(&copy_text("rsFilewinSize.outcome.sep", &[]))
}

/// 真发一趟（经通道问后端 `files-size`）。
pub async fn size_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    path: &serde_json::Value,
    name: &str,
) -> Result<Sized, String> {
    let d = super::source::ask(
        line,
        origin,
        CMD_SIZE,
        &serde_json::json!({ "path": path }),
        SIZE_BUDGET,
    )
    .await?;
    sized_from_reply(name, &d)
}

/// 窗口上的样子：在算哪一项 · 上一摞的结局。**跨线程共享**（UI 线程画，tokio 那条写）。形状照 [`super::copy::CopyBoard`]。
#[derive(Clone, Default)]
pub struct SizeBoard {
    inner: Arc<Mutex<Desk>>,
    rounds: Arc<AtomicU64>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

#[derive(Default)]
struct Desk {
    running: Option<String>,
    last: Option<Vec<SizeResult>>,
}

impl SizeBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.held() = ctx;
    }

    fn poke(&self) {
        if let Some(c) = self.ctx.held().as_ref() {
            c.request_repaint();
        }
    }

    pub fn begin(&self, name: &str) {
        self.inner.held().running = Some(name.to_string());
        self.poke();
    }

    pub fn running(&self) -> Option<String> {
        self.inner.held().running.clone()
    }

    pub fn finish(&self, results: Vec<SizeResult>) {
        {
            let mut d = self.inner.held();
            d.running = None;
            d.last = Some(results);
        }
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    pub fn last(&self) -> Option<Vec<SizeResult>> {
        self.inner.held().last.clone()
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/size_tests.rs"]
mod tests;
