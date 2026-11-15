//! 送进会话的字：附件引用写法 `AttachRefer` 与送进终端的两句命令。

use super::*;

/// 粘贴就收成图片的扩展名（小写）：**通用表，只此一张**。一家在它之外还认的写在 [`AttachRefer::images_extra`]。
pub const PASTED_IMAGES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// 一家的**附件引用写法**（[`Adapter::attach`]）：聊天里随一句发的一份附件，送进那一家时写成哪一形。
/// 落点（`.ccm-attach/<编号>/<原名>`）住通用层（`common::attachment_dir`）；是不是图片由通用层判（[`is_image`]），
/// 拼成一句的那一步只此一处（[`typed_with`]）。
#[derive(Debug, Clone, Copy)]
pub struct AttachRefer {
    /// 一张图片（[`is_image`] 认的）送进去的样子。
    pub(crate) image: fn(&AttachRef<'_>) -> Refer,
    /// 别的文件送进去的样子。
    pub(crate) file: fn(&AttachRef<'_>) -> Refer,
    /// 这一家在通用表（[`PASTED_IMAGES`]）之外还认的图片扩展名（小写）；没申报 ⇒ `None`，只认通用表。
    pub images_extra: Option<&'static [&'static str]>,
}

/// 名叫 `name` 的一份附件对 `face` 那一家算不算图片：扩展名（不分大小写）在通用表里，或在那一家申报的里。
/// 没有扩展名 · 只有点开头的（`.png`）不算。
pub fn is_image(face: &AttachRefer, name: &str) -> bool {
    let Some((_, ext)) = name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    PASTED_IMAGES
        .iter()
        .chain(face.images_extra.into_iter().flatten())
        .any(|e| *e == ext)
}

/// 会话那台上落好的一份附件：原名 · 绝对路径 · 相对工作目录的路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachRef<'a> {
    pub name: &'a str,
    pub abs: &'a str,
    pub rel: &'a str,
}

/// 一份附件送进去的样子：在正文之前单独粘一次（图片路径 ⇒ 那一家收成图片附件）· 正文后面的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refer {
    Paste(String),
    Line(String),
}

/// 一句连同它的附件写成送进去的样子：各单独粘一次的 · 正文（引用行接在后面）· 终端没了要恢复时随起交的第一句（粘贴那一形交不进去 ⇒ 全写成行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typed {
    pub pastes: Vec<String>,
    pub text: String,
    pub first_say: String,
}

/// 认不出是哪一家 · 这一家没有 [`Adapter::attach`] ⇒ 全写相对路径、一行一个（路径在那台读得到，模型照路径自己读）。
fn refer_plain(a: &AttachRef<'_>) -> Refer {
    Refer::Line(a.rel.to_string())
}

/// `kind` 那一家把 `text` 连同 `refs` 写成送进去的样子（[`Typed`]）。
pub fn typed_with(kind: &str, text: &str, refs: &[AttachRef<'_>]) -> Typed {
    typed_among(REGISTRY, kind, text, refs)
}

/// [`typed_with`] 在给定注册表上（判据喂含夹具家的那一张）。
pub(crate) fn typed_among(
    registry: &[Adapter],
    kind: &str,
    text: &str,
    refs: &[AttachRef<'_>],
) -> Typed {
    let face = adapter_among(registry, kind).and_then(|a| a.attach);
    let refer = |r: &AttachRef<'_>| match &face {
        Some(f) if is_image(f, r.name) => (f.image)(r),
        Some(f) => (f.file)(r),
        None => refer_plain(r),
    };
    let join = |head: &str, lines: &[String]| {
        std::iter::once(head.to_string())
            .filter(|h| !h.is_empty())
            .chain(lines.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let (mut pastes, mut lines, mut all) = (Vec::new(), Vec::new(), Vec::new());
    for r in refs {
        match refer(r) {
            Refer::Paste(p) => {
                all.push(p.clone());
                pastes.push(p);
            }
            Refer::Line(l) => {
                all.push(l.clone());
                lines.push(l);
            }
        }
    }
    Typed {
        pastes,
        text: join(text, &lines),
        first_say: join(text, &all),
    }
}

/// 某一家清空会话用的那一句（[`Adapter::clear_request`]）。认不出这家 / 这一家不支持 ⇒ `None`。
pub fn clear_request_of(kind: &str) -> Option<&'static str> {
    adapter_among(REGISTRY, kind).and_then(|a| a.clear_request)
}

/// 某一家请求压缩上下文用的那一句（[`Adapter::compact_request`]）。认不出这家 / 这一家不支持 ⇒ `None`。
pub fn compact_request_of(kind: &str) -> Option<&'static str> {
    adapter_among(REGISTRY, kind).and_then(|a| a.compact_request)
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/attach_face_tests.rs"]
mod tests;
