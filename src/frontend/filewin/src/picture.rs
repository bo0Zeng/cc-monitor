//! **一张图读一次、解一次**：预览 · 「看一眼」· 查看页摆的是同一份。
//!
//! 设计稿 `文件与历史.md` 甲4 ⑬。
//!
//! # 一、读：后端按块给字节，窗口解码
//!
//! 经后端 `files-read-chunk` 逐块读回（每块 [`super::lossy_pull::PULL_CHUNK`]，到后端那一条的上限），
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
pub const DECODE_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// 一张图最多读多少（字节）。**唯一住址**；登记在 `byte_cap_registry`。查看页的上限就是它；
/// 预览跟着光标走，另有更小的 [`super::preview::IMAGE_MAX_BYTES`] 在发之前预判。
/// 为什么是这么大：一块 256 KB 一趟，32 MB 是 128 趟（远端上十几秒到半分钟，再大该下载到本机看）；
/// 相机原图、整屏截图都在里面。超了：不读、出声。
pub const PICTURE_MAX_BYTES: u64 = 32 * 1024 * 1024;

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
    pub wire: serde_json::Value,
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

    /// 逐块读回；读到一半被扔了 ⇒ `Ok(None)`（不再读下一块）。
    async fn read(&self, key: &PicKey, l: &Load) -> Result<Option<Vec<u8>>, String> {
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            let d = super::source::ask(
                &l.line,
                &l.origin,
                CMD_READ_CHUNK,
                &serde_json::json!({ "path": l.wire, "offset": bytes.len(), "len": super::lossy_pull::PULL_CHUNK }),
                super::chunk_upload::CHUNK_BUDGET,
            )
            .await?;
            let body = d["content"]["b16"]
                .as_str()
                .and_then(super::lossy_pull::unhex)
                .ok_or_else(|| copy_text("rsFilewinLossyPull.reply.badChunk", &[]))?;
            bytes.extend_from_slice(&body);
            if bytes.len() as u64 > l.cap {
                return Err(copy_text(
                    "rsFilewinPicture.read.grew",
                    &[("limit", &copy_core::size_text(l.cap))],
                ));
            }
            let total = d["size"].as_u64().unwrap_or(key.size);
            match self.slots.lock().unwrap().get_mut(key) {
                Some(Slot::Reading { got, total: t }) => {
                    *got = bytes.len() as u64;
                    *t = total;
                }
                _ => return Ok(None),
            }
            if let Some(c) = &l.ctx {
                c.request_repaint();
            }
            if d["eof"].as_bool() != Some(false) || body.is_empty() {
                return Ok(Some(bytes));
            }
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

    /// 此刻留着几张（判据看）。
    pub fn len(&self) -> usize {
        self.slots.lock().unwrap().len()
    }

    /// 一张都没留着。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
    limits.max_alloc = Some(DECODE_MAX_BYTES);
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

/// 按字节寻址分块读回（读族；与有损名下载同一条）。
pub const CMD_READ_CHUNK: &str = "files-read-chunk";

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/picture_tests.rs"]
mod tests;
