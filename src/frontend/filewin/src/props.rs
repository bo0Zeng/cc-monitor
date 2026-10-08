//! 右键「属性」：问那台后端 `files-stat`，摆一个框列路径 · 种类 · 大小 · 修改时间 · 权限 · 所有者 · 链接指向。
//!
//! 只读、单选；问不到就把原话摆在框里（不猜）。框是窗口状态，换目录 / 关掉就没。

use copy_core::copy_text;
use std::sync::{Arc, Mutex};

use super::shell::{FileWindow, NO_LINE};
use super::source::Listed;

/// 问 `files-stat` 的期限（同读侧那几问的量级）。
pub const STAT_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 那台后端答回来的、框里要列的那几格。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stat {
    pub path: String,
    pub size: u64,
    /// 修改时间的完整写法（那台后端按它的本地钟写好；照抄）。
    pub mtime_full: Option<String>,
    /// unix 权限位（非 unix 远端缺席）。
    pub mode: Option<u32>,
    pub owner: Option<String>,
    /// 路径本身是链接 ⇒ 它指向哪（原文）。
    pub link_target: Option<String>,
}

/// 一趟 `files-stat` 的 `data` → [`Stat`]。`path` 解不出来就是错（不猜）。
pub fn stat_from_reply(d: &serde_json::Value) -> Result<Stat, String> {
    let path = d
        .get("path")
        .and_then(super::find::decode_path)
        .ok_or_else(|| copy_text("rsFilewinSource.path.badShape", &[]))?;
    let text = |v: Option<&serde_json::Value>| -> Option<String> {
        let v = v?;
        if v.is_null() {
            return None;
        }
        super::find::decode_path(v).map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    Ok(Stat {
        path: String::from_utf8_lossy(&path).into_owned(),
        size: d
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        mtime_full: d
            .get("mtime_full")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        mode: super::writeops::mode_of(d),
        owner: d
            .get("owner")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        link_target: text(d.get("link_target")),
    })
}

/// `0o755` → `rwxr-xr-x (755)`；特殊位只写进八进制那一截。
pub fn mode_text(m: u32) -> String {
    let bits = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    let rwx: String = bits
        .iter()
        .map(|(b, c)| if m & b != 0 { *c } else { '-' })
        .collect();
    format!("{rwx} ({m:o})")
}

/// 框里那一摞的状态（问的那一趟写、UI 线程读）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Loading,
    Ready(Stat),
    Failed(String),
}

/// 一个「属性」框。
pub struct Props {
    /// 问的是哪一行（名字 ＋ 列表里看得到的那几格：种类、是不是链接）。
    pub row: Listed,
    pub state: Arc<Mutex<State>>,
}

impl Props {
    /// 框里一行一行列什么（标签, 值）。判据与界面看同一个值。
    pub fn lines(&self) -> Vec<(String, String)> {
        let st = self.state.lock().unwrap().clone();
        let mut out = vec![(
            copy_text("rsFilewinProps.label.kind", &[]),
            super::kind::type_text(&self.row),
        )];
        let unknown = || copy_text("rsFilewinProps.value.unknown", &[]);
        match st {
            State::Loading => out.push((
                copy_text("rsFilewinProps.label.path", &[]),
                copy_text("rsFilewinProps.value.loading", &[]),
            )),
            State::Failed(why) => out.push((copy_text("rsFilewinProps.label.path", &[]), why)),
            State::Ready(s) => {
                out.push((copy_text("rsFilewinProps.label.path", &[]), s.path.clone()));
                if !self.row.is_dir {
                    out.push((
                        copy_text("rsFilewinProps.label.size", &[]),
                        copy_text(
                            "rsFilewinProps.value.size",
                            &[
                                ("human", &super::rows::human_size(s.size)),
                                ("bytes", &s.size.to_string()),
                            ],
                        ),
                    ));
                }
                out.push((
                    copy_text("rsFilewinProps.label.mtime", &[]),
                    s.mtime_full.clone().unwrap_or_else(unknown),
                ));
                out.push((
                    copy_text("rsFilewinProps.label.mode", &[]),
                    s.mode.map(mode_text).unwrap_or_else(unknown),
                ));
                out.push((
                    copy_text("rsFilewinProps.label.owner", &[]),
                    s.owner.unwrap_or_else(unknown),
                ));
                if let Some(t) = s.link_target {
                    out.push((copy_text("rsFilewinProps.label.linkTarget", &[]), t));
                }
            }
        }
        out
    }
}

impl FileWindow {
    /// 摆出第 `i` 行的「属性」框并去问那台后端。没运行时 / 没通道 ⇒ 框里说清，不发。
    pub fn begin_props(&mut self, i: usize, ctx: Option<egui::Context>) -> bool {
        let Some(row) = self.listing.rows.lock().unwrap().get(i).cloned() else {
            return false;
        };
        let path = self.row_path(&row).wire();
        let state = Arc::new(Mutex::new(State::Loading));
        self.props = Some(Props {
            row,
            state: state.clone(),
        });
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            *state.lock().unwrap() = State::Failed(NO_LINE.to_string());
            return true;
        };
        let origin = self.source.origin();
        h.spawn(async move {
            let got = super::source::ask(
                &line,
                &origin,
                CMD_STAT,
                &serde_json::json!({ "path": path }),
                STAT_BUDGET,
            )
            .await
            .and_then(|d| stat_from_reply(&d));
            *state.lock().unwrap() = match got {
                Ok(s) => State::Ready(s),
                Err(e) => State::Failed(e),
            };
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 「属性」框开着吗（判据用）。
    pub fn props(&self) -> Option<&Props> {
        self.props.as_ref()
    }

    /// 画「属性」框（开着才画）。点「关闭」或框外那个 × 就收。
    pub(super) fn props_ui(&mut self, ui: &mut egui::Ui) {
        let Some(p) = &self.props else {
            return;
        };
        let title = copy_text("rsFilewinProps.box.title", &[("name", &p.row.name)]);
        let lines = p.lines();
        let mut open = true;
        let mut close = false;
        egui::Window::new(title)
            .id(egui::Id::new("filewin-props"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                let p = super::theme::palette(ui.ctx());
                egui::Grid::new("filewin-props-grid")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        for (k, v) in &lines {
                            ui.colored_label(p.text2, k);
                            ui.add(egui::Label::new(v).wrap());
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
                if ui
                    .button(copy_text("rsFilewinProps.box.close", &[]))
                    .clicked()
                {
                    close = true;
                }
            });
        if close || !open {
            self.props = None;
        }
    }
}

/// 问一个路径的元数据（与改权限那个框问现值同一条命令）。
pub const CMD_STAT: &str = "files-stat";

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/props_tests.rs"]
mod tests;
