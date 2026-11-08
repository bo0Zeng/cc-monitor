//! **一张图读一次、解一次**：预览 · 「看一眼」· 查看页摆的是同一份。
//!
//!
//! # 一、读：后端按块给字节，窗口解码
//!
//! 经后端 `files-read-chunk` 逐块读回（每块 [`chunk_core::CHUNK`]，到后端那一条的上限），
//! 整份不超过调用方给的上限（超了就停、出声：读的途中文件变大了）；读完在后台那条线上解码，
//! 最长边过了 `side`（[`MAX_SIDE`] 与这块显卡认的最大贴图边取小）就先等比缩，原图多大照记。
//!
//! # 二、共用：一把钥匙一份
//!
//! 钥匙 ＝ 路径 ＋ 大小 ＋ 修改时间（列表那一行上的）：文件换了，钥匙就换了。
//! 谁先要谁读（[`Pictures::load`]，已有就不再起），后到的直接看（[`Pictures::look`]）；
//! 解好的那一张在 UI 线程第一次被看时交给显存（[`egui::TextureHandle`]，克隆只是多一个引用）。
//! 窗口每帧报一次「哪几张还摆着」（[`Pictures::keep_only`]），不在里面的扔掉 —— 显存跟着放。
//!
//! # ⚠ 买不到什么
//!
//! - 动图只解第一帧；EXIF 方向不转。
//! - 读到一半被扔掉的那一趟不撤回线上那一块（下一块之前看见被扔了就停）。

use copy_core::copy_text;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// 解出来的图最长边的上限（再与显卡认的最大贴图边取小）。8192² 的 RGBA 是 256 MB，再大一张就把显存吃光了。
pub const MAX_SIDE: u32 = 8192;

/// 解码那一步最多占多少内存（字节）。超了说「像素过多」，不说「解不开」。
pub const DECODE_MAX_BYTES: host_core::Cap = host_core::Cap {
    bytes: 512 * 1024 * 1024,
    over: host_core::Over::Reject,
};

/// 一张图的钥匙：路径 ＋ 大小 ＋ 修改时间（列表那一行上的）。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PicKey {
    pub path: String,
    pub size: u64,
    pub mtime: Option<u64>,
}

impl PicKey {
    /// 列表那一行的钥匙。
    pub fn of(l: &super::source::Listed) -> Self {
        Self {
            path: l.row.path.clone(),
            size: l.row.size,
            mtime: l.mtime_secs,
        }
    }
}

/// 一张解好、上了显存的图。
#[derive(Clone)]
pub struct Pic {
    pub tex: egui::TextureHandle,
    /// 贴图多大（缩过就比原图小）。
    pub shown: [usize; 2],
    /// 原图多大。
    pub native: [u32; 2],
}

impl std::fmt::Debug for Pic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pic({:?} of {:?})", self.shown, self.native)
    }
}

/// 此刻那一张怎么样了。
#[derive(Clone, Debug)]
pub enum Look {
    /// 在读（读了多少 / 一共多少）。
    Reading {
        got: u64,
        total: u64,
    },
    Ready(Pic),
    /// 没成，那一句。
    Failed(String),
}

enum Slot {
    Reading {
        got: u64,
        total: u64,
    },
    Decoded {
        img: egui::ColorImage,
        native: [u32; 2],
    },
    Ready(Pic),
    Failed(String),
}

/// 起一趟读要的那几样（都是调用方手上现成的）。
pub struct Load {
    pub rt: tokio::runtime::Handle,
    pub line: super::source::Line,
    pub origin: super::source::Origin,
    /// 发出去的路径（有损名按字节）。
    pub wire: cmd_wire::files::WirePath,
    /// 读到多少就停。
    pub cap: u64,
    /// 解出来最长边不超过它。
    pub side: u32,
    /// 读完叫一次重画。
    pub ctx: Option<egui::Context>,
}

/// 窗口里所有图（一扇窗一份，所有标签页 · 预览 · 「看一眼」拿同一份）。
#[derive(Clone, Default)]
pub struct Pictures {
    slots: Arc<Mutex<HashMap<PicKey, Slot>>>,
}

/// 贴图的取样：缩小时走多级纹理（大图缩进面板里不起锯齿）。
const TEX_OPTIONS: egui::TextureOptions = egui::TextureOptions {
    mipmap_mode: Some(egui::TextureFilter::Linear),
    ..egui::TextureOptions::LINEAR
};

impl Pictures {
    /// 这一张此刻怎么样（`None` ＝ 没人要过 / 已经扔了）。解好还没上显存的，有 `ctx` 就这一下交上去。
    pub fn look(&self, key: &PicKey, ctx: Option<&egui::Context>) -> Option<Look> {
        let mut slots = self.slots.lock().unwrap();
        let slot = slots.get_mut(key)?;
        if let (Slot::Decoded { .. }, Some(c)) = (&*slot, ctx) {
            if let Slot::Decoded { img, native } =
                std::mem::replace(slot, Slot::Failed(String::new()))
            {
                let shown = img.size;
                let tex = c.load_texture(format!("filewin-picture:{}", key.path), img, TEX_OPTIONS);
                *slot = Slot::Ready(Pic { tex, shown, native });
            }
        }
        Some(match slot {
            Slot::Reading { got, total } => Look::Reading {
                got: *got,
                total: *total,
            },
            Slot::Decoded { .. } => Look::Reading {
                got: key.size,
                total: key.size,
            },
            Slot::Ready(p) => Look::Ready(p.clone()),
            Slot::Failed(s) => Look::Failed(s.clone()),
        })
    }

    /// 要这一张：没有就起一趟读（回 `true`），已有（在读 / 好了 / 没成）就不再起（回 `false`）。
    pub fn load(&self, key: &PicKey, l: Load) -> bool {
        {
            let mut slots = self.slots.lock().unwrap();
            if slots.contains_key(key) {
                return false;
            }
            slots.insert(
                key.clone(),
                Slot::Reading {
                    got: 0,
                    total: key.size,
                },
            );
        }
        let me = self.clone();
        let key = key.clone();
        l.rt.clone().spawn(async move {
            let r = match me.read(&key, &l).await {
                Ok(Some(bytes)) => {
                    let side = l.side;
                    tokio::task::spawn_blocking(move || decode(&bytes, side))
                        .await
                        .unwrap_or_else(|_| Err(undecodable("decoder panicked")))
                }
                Ok(None) => return,
                Err(why) => Err(why),
            };
            {
                let mut slots = me.slots.lock().unwrap();
                if let Some(slot) = slots.get_mut(&key) {
                    *slot = match r {
                        Ok((img, native)) => Slot::Decoded { img, native },
                        Err(why) => Slot::Failed(why),
                    };
                }
            }
            if let Some(c) = l.ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 经 [`chunk_core::pull_capped`] 逐块读回；读到一半被扔了 ⇒ `Ok(None)`（不再读下一块）。
    async fn read(&self, key: &PicKey, l: &Load) -> Result<Option<Vec<u8>>, String> {
        let ask = Asking(super::chunk_upload::Over {
            line: &l.line,
            origin: &l.origin,
        });
        let mut watch = Reading { pics: self, key, l };
        let bytes = chunk_core::pull_capped(
            &ask,
            l.wire.clone(),
            l.cap,
            &mut watch,
            || None,
            |t| {
                Some(copy_text(
                    "rsFilewinPicture.read.grew",
                    &[("limit", &copy_core::size_text(t.cap))],
                ))
            },
        )
        .await;
        match bytes {
            Ok(_) if chunk_core::Watch::stopped(&watch) => Ok(None),
            Ok(b) => Ok(Some(b)),
            Err(None) => Ok(None),
            Err(Some(why)) => Err(why),
        }
    }

    /// 扔掉这一张（「重试」用：下一次 [`Self::load`] 重新读）。
    pub fn forget(&self, key: &PicKey) {
        self.slots.lock().unwrap().remove(key);
    }

    /// 只留这几张（窗口每帧报一次此刻还摆着的）。
    pub fn keep_only(&self, keep: &HashSet<PicKey>) {
        self.slots.lock().unwrap().retain(|k, _| keep.contains(k));
    }
}

/// 读一张图问那台的那一口（就是分块那一圈的 [`super::chunk_upload::Over`]）：没成 ⇒ `Some(那一句)`；`None` ＝ 那一张被扔了。
struct Asking<'a>(super::chunk_upload::Over<'a>);

impl chunk_core::Ask for Asking<'_> {
    type Err = Option<String>;
    async fn ask<C: cmd_wire::Call + Sync>(&self, req: &C) -> Result<C::Reply, Self::Err> {
        chunk_core::Ask::ask(&self.0, req)
            .await
            .map_err(|f| Some(f.said))
    }
}

/// 读那一趟的进度记进那一格（读了多少 · 一共多少）；那一格不在读了（被扔了）⇒ 撤。
struct Reading<'a> {
    pics: &'a Pictures,
    key: &'a PicKey,
    l: &'a Load,
}

impl chunk_core::Watch for Reading<'_> {
    fn stopped(&self) -> bool {
        !matches!(
            self.pics.slots.lock().unwrap().get(self.key),
            Some(Slot::Reading { .. })
        )
    }
    fn moved(&mut self, done: u64, total: Option<u64>) {
        if let Some(Slot::Reading { got, total: t }) =
            self.pics.slots.lock().unwrap().get_mut(self.key)
        {
            *got = done;
            if let Some(n) = total {
                *t = n;
            }
            if let Some(c) = &self.l.ctx {
                c.request_repaint();
            }
        }
    }
}

/// 解不开：句子只说解不开，解码器的原话记一行日志。
fn undecodable(e: impl std::fmt::Display) -> String {
    tracing::warn!("filewin: image undecodable: {e}");
    copy_text("rsFilewinPicture.decode.undecodable", &[])
}

/// 字节 → 一张界面能画的图（最长边过了 `side` 先等比缩）＋ 原图多大。解不开 ⇒ 一句（原话进日志）；
/// 过了内存那道闸（[`DECODE_MAX_BYTES`]）⇒ 「像素过多」那一句。
pub fn decode(bytes: &[u8], side: u32) -> Result<(egui::ColorImage, [u32; 2]), String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(undecodable)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(DECODE_MAX_BYTES.bytes);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => {
            tracing::warn!("filewin: image over the decode limit: {e}");
            copy_text("rsFilewinPicture.decode.tooManyPixels", &[])
        }
        e => undecodable(e),
    })?;
    let native = [img.width(), img.height()];
    let side = side.max(1);
    let img = if native[0].max(native[1]) > side {
        img.thumbnail(side, side)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok((
        egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
        native,
    ))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/picture_tests.rs"]
mod tests;
