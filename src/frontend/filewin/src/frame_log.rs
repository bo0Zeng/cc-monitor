//! 帧日志 —— 收集「一闪一闪」的证据用。
//!
//! 设了环境变量 [`ENV`]（值 = 一个文件路径）才开：之后每帧往那份文件追加一行，说这一帧为什么画、画面与字的布局比上一帧变没变。
//! 没设 ⇒ [`FrameLog::off`]，[`FrameLog::record`] 第一句就回去：不读画面、不算哈希、不写任何东西。
//!
//! 一行长这样（空格分隔的 `键=值`，好 grep）：
//! `frame=12 t_ms=3456 pass=0 events=1 pointer=512.0,88.5 ppp=1.50 paint=changed layout=same shapes=214 why=…`
//! - `pass`：这一帧里的第几趟（egui 丢掉一趟重排时 > 0）；
//! - `events`：这一帧收到几个输入事件（鼠标一动就是 1）；
//! - `ppp`：每个逻辑点几个像素（显示缩放 150% ⇒ 1.50）；
//! - `paint`：所有图形的外框比上一帧变没变（悬停高亮这类只改画面的也算）；
//! - `layout`：所有文字的位置与内容变没变（布局跳了才会变）；
//! - `why`：egui 记下的、促成这一帧的重画请求（`文件:行 原因`，几条用 `|` 连）。

use std::hash::{Hash, Hasher};
use std::io::Write as _;

/// 打开帧日志的环境变量（值 = 日志文件路径）。
pub const ENV: &str = "CCM_FILEWIN_FRAME_LOG";

/// 帧日志。关着的那一形什么都不做。
#[derive(Debug, Default)]
pub struct FrameLog {
    out: Option<std::io::BufWriter<std::fs::File>>,
    started: Option<std::time::Instant>,
    frames: u64,
    last: Option<(u64, u64)>,
}

impl FrameLog {
    /// 关着。
    pub fn off() -> Self {
        Self::default()
    }

    /// 照环境变量开：没设 / 空 ⇒ 关着；开不了那份文件 ⇒ 关着并记一条警告（不拦开窗）。
    pub fn from_env() -> Self {
        match std::env::var_os(ENV) {
            Some(p) if !p.is_empty() => {
                Self::to_file(std::path::Path::new(&p)).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "frame log not opened");
                    Self::off()
                })
            }
            _ => Self::off(),
        }
    }

    /// 往这份文件追加（没有就建）。
    pub fn to_file(path: &std::path::Path) -> std::io::Result<Self> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            out: Some(std::io::BufWriter::new(f)),
            started: Some(std::time::Instant::now()),
            frames: 0,
            last: None,
        })
    }

    /// 这一帧画完之后调：记一行。关着 ⇒ 立刻回去。写不进去 ⇒ 关掉自己（不再每帧撞同一个错）。
    pub fn record(&mut self, ctx: &egui::Context) {
        let Some(out) = self.out.as_mut() else {
            return;
        };
        self.frames += 1;
        let (shapes, paint, layout) = fingerprint(ctx);
        let paint_word = verdict(self.last.map(|l| l.0), paint);
        let layout_word = verdict(self.last.map(|l| l.1), layout);
        self.last = Some((paint, layout));
        let (events, pointer, ppp) =
            ctx.input(|i| (i.events.len(), i.pointer.latest_pos(), i.pixels_per_point));
        let why: Vec<String> = ctx
            .repaint_causes()
            .iter()
            .map(|c| {
                format!(
                    "{}:{} {}",
                    c.file.rsplit(['/', '\\']).next().unwrap_or(c.file),
                    c.line,
                    c.reason
                )
                .replace(' ', "_")
            })
            .collect();
        let t_ms = self.started.map_or(0, |t| t.elapsed().as_millis());
        let line = format!(
            "frame={} t_ms={t_ms} pass={} events={events} pointer={} ppp={ppp:.2} paint={paint_word} layout={layout_word} shapes={shapes} why={}",
            self.frames,
            ctx.current_pass_index(),
            pointer.map_or_else(|| "none".to_string(), |p| format!("{:.1},{:.1}", p.x, p.y)),
            if why.is_empty() { "-".to_string() } else { why.join("|") },
        );
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            self.out = None;
        }
    }
}

/// 判据看的两格（只给测试）。
#[cfg(test)]
impl FrameLog {
    pub(crate) fn is_on(&self) -> bool {
        self.out.is_some()
    }

    /// 记过几帧（关着恒 0）。
    pub(crate) fn frames(&self) -> u64 {
        self.frames
    }
}

fn verdict(last: Option<u64>, now: u64) -> &'static str {
    match last {
        None => "first",
        Some(l) if l == now => "same",
        Some(_) => "changed",
    }
}

/// `(图形个数, 所有图形外框的哈希, 所有文字位置与内容的哈希)`：只读这一帧已经画进各层的东西。
fn fingerprint(ctx: &egui::Context) -> (usize, u64, u64) {
    let layers: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    let mut paint = std::collections::hash_map::DefaultHasher::new();
    let mut layout = std::collections::hash_map::DefaultHasher::new();
    let mut n = 0usize;
    let q = |v: f32| (v * 2.0).round() as i64;
    ctx.graphics(|g| {
        for id in &layers {
            let Some(list) = g.get(*id) else { continue };
            for cs in list.all_entries() {
                n += 1;
                let r = cs.shape.visual_bounding_rect();
                (id.id, q(r.min.x), q(r.min.y), q(r.max.x), q(r.max.y)).hash(&mut paint);
                if let egui::Shape::Text(t) = &cs.shape {
                    (id.id, q(t.pos.x), q(t.pos.y), t.galley.text()).hash(&mut layout);
                }
            }
        }
    });
    (n, paint.finish(), layout.finish())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/frame_log_tests.rs"]
mod tests;
