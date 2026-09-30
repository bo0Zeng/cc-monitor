//! 编辑计划 —— 批注与文档关联的写,拆成「算出新内容」与「写盘」两层(2026-09-24)。
//!
//! # 为什么拆
//!
//! 批注侧车(`<仓>/.codepicture/annotations/<id>.json`)与文档 frontmatter 的 `covers:` 都是
//! **被分析仓里的文件**。消费方 cc-monitor 规定只有它那一侧的「文件管理」能写用户文件
//! (CAS、备份、回读、围栏都住在那里,且本机与远端同一条路)。拆开前,「新内容是什么」
//! (批注 id、序列化格式、提议不降级已批准的、批准只改状态、`covers:` 怎么增删)与
//! `fs::write` 缠在同一个函数里 —— 想换一个写的人,就得把这些规则抄一份出去。
//!
//! # 三层
//!
//! | 层 | 是什么 | 碰盘吗 |
//! |---|---|---|
//! | `next_*` | **纯**:给定盘上现状(`current`,`None` = 不存在)→ 新内容 | 不碰 |
//! | `plan_*` | 读盘上现状 + `next_*` ⇒ 一份 [`Planned`] | **只读** |
//! | 写盘 | [`crate::annotations::apply`] / [`crate::docs::apply`] | 写 |
//!
//! `Engine` 那六个写方法(`add/propose/approve/remove_annotation`、`write/remove_doc_link`)
//! 签名与行为不变,只是改成「`plan_*` + 写盘」—— 本仓自己的 MCP / 测试照旧用它们。
//! 别的写者(cc-monitor)只用 `plan_*`,把 [`FileEdit`] 交给自己的写口:
//! `before` 当 CAS 期望、`after` 当新全文(`None` = 删)、`parents` = 要不要建父目录。
//!
//! ⚠ 批注 id 与存储格式**只在这里定义一次**(`annotation_id` / `annotations::render`),
//! 外面的写者拿到的是算好的全文,不需要、也不许自己拼。

use crate::annotations;
use crate::docs;
use crate::model::{Annotation, AnnotationOrigin, AnnotationStatus};
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// 一处要落盘的改动。**只描述,不执行**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FileEdit {
    /// 仓相对路径,`/` 分隔。
    pub rel: String,
    /// 算的时候盘上是什么(`None` = 不存在)。写者拿它当 CAS 期望:盘上已经不是它 ⇒ 别写、重算。
    pub before: Option<String>,
    /// 要变成什么(`None` = 这份文件该不存在 = 删)。
    pub after: Option<String>,
    /// 写的时候要不要建父目录(批注目录首写才建 —— 开面板 / 只读查询绝不建它)。
    pub parents: bool,
}

/// 一次计划:原方法的返回值 + 要落盘的那一处(`None` = 盘上已经是想要的样子,什么都不用写)。
///
/// 🔴 `edit` 为 `Some` 时 `before != after` 恒成立(构造处归一),写者不用自己再比一遍。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Planned<T> {
    pub value: T,
    pub edit: Option<FileEdit>,
}

/// 计划算不出来的两类:调用方给错了东西 / 盘上那份读不出来。
#[derive(Debug)]
pub enum PlanError {
    /// 空正文、越界路径、非法 id 之类 —— 换个输入才行。
    Invalid(String),
    /// 读盘上现状失败(不是「不存在」:权限、编码、是个目录……)。**绝不当成不存在去覆盖**。
    Io(std::io::Error),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::Invalid(m) => f.write_str(m),
            PlanError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PlanError {}

impl From<std::io::Error> for PlanError {
    fn from(e: std::io::Error) -> Self {
        PlanError::Io(e)
    }
}

/// 构造处归一:算出来与盘上逐字相同 ⇒ 没有要写的。
fn planned<T>(
    value: T,
    rel: String,
    before: Option<String>,
    after: Option<String>,
    parents: bool,
) -> Planned<T> {
    let edit = (before != after).then_some(FileEdit {
        rel,
        before,
        after,
        parents,
    });
    Planned { value, edit }
}

/// 批注 id:内容哈希(file|symbol|body),确定性;status 不入哈希 → approve 不改 id。
/// ⚠ `DefaultHasher` 跨 Rust 版本不保证稳定;id 是提交进 git 的侧车文件名,换工具链后同内容
/// 再 `add` 会算出新 id(生成重复文件而非覆盖,旧文件成孤儿)。要跨版本稳定可换固定算法。
pub fn annotation_id(file: &str, symbol: Option<&str>, body: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    file.hash(&mut h);
    symbol.hash(&mut h);
    body.hash(&mut h);
    format!("{:x}", h.finish())
}

/// 读仓里一份文件的现状:不存在 ⇒ `None`;别的读错误上抛(绝不当成不存在)。
fn read_current(repo: &Path, rel: &str) -> Result<Option<String>, PlanError> {
    match std::fs::read_to_string(repo.join(rel)) {
        Ok(c) => Ok(Some(c)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PlanError::Io(e)),
    }
}

fn annotation_rel_of(id: &str) -> Result<String, PlanError> {
    annotations::rel_path(id).ok_or_else(|| PlanError::Invalid(format!("非法批注 id:{id}")))
}

fn new_annotation(
    file: &str,
    symbol: Option<&str>,
    body: &str,
    author: &str,
    status: AnnotationStatus,
    origin: AnnotationOrigin,
) -> Result<(String, String, String), PlanError> {
    if body.trim().is_empty() {
        return Err(PlanError::Invalid("批注正文不能为空".into()));
    }
    let id = annotation_id(file, symbol, body);
    let rel = annotation_rel_of(&id)?;
    let text = annotations::render(&Annotation {
        id: id.clone(),
        file: file.to_string(),
        symbol: symbol.map(String::from),
        body: body.to_string(),
        author: author.to_string(),
        status,
        origin,
    })
    .map_err(PlanError::Io)?;
    Ok((id, rel, text))
}

// ── 纯的一层:给定现状 → 新内容 ──────────────────────────────────────────────

/// 人写批注,直接 Active(人写永远赢:同内容的 Proposed 被提为 Active)。`value` = 批注 id。
pub fn next_add_annotation(
    file: &str,
    symbol: Option<&str>,
    body: &str,
    author: &str,
    current: Option<&str>,
) -> Result<Planned<String>, PlanError> {
    let (id, rel, text) = new_annotation(
        file,
        symbol,
        body,
        author,
        AnnotationStatus::Active,
        AnnotationOrigin::Human,
    )?;
    Ok(planned(
        id,
        rel,
        current.map(String::from),
        Some(text),
        true,
    ))
}

/// agent 提议批注,Proposed。**门禁保护**:同内容已是 Active ⇒ 不降级,什么都不写。`value` = id。
pub fn next_propose_annotation(
    file: &str,
    symbol: Option<&str>,
    body: &str,
    author: &str,
    current: Option<&str>,
) -> Result<Planned<String>, PlanError> {
    let (id, rel, text) = new_annotation(
        file,
        symbol,
        body,
        author,
        AnnotationStatus::Proposed,
        AnnotationOrigin::Agent,
    )?;
    let before = current.map(String::from);
    let already_active = current
        .and_then(annotations::parse)
        .is_some_and(|a| a.status == AnnotationStatus::Active);
    if already_active {
        return Ok(Planned {
            value: id,
            edit: None,
        });
    }
    Ok(planned(id, rel, before, Some(text), true))
}

/// 人审批准:Proposed → Active,**只改 `status`**。`value` = 这条批注在不在(非法 id / 不存在 /
/// 读不懂 ⇒ `false`,什么都不写)。
pub fn next_approve_annotation(
    id: &str,
    current: Option<&str>,
) -> Result<Planned<bool>, PlanError> {
    let Some(rel) = annotations::rel_path(id) else {
        return Ok(Planned {
            value: false,
            edit: None,
        });
    };
    let Some(mut a) = current.and_then(annotations::parse) else {
        return Ok(Planned {
            value: false,
            edit: None,
        });
    };
    a.status = AnnotationStatus::Active;
    let text = annotations::render(&a).map_err(PlanError::Io)?;
    Ok(planned(
        true,
        rel,
        current.map(String::from),
        Some(text),
        true,
    ))
}

/// 删批注。`value` = 原本在不在(非法 id / 不存在 ⇒ `false`)。
pub fn next_remove_annotation(id: &str, current: Option<&str>) -> Planned<bool> {
    match (annotations::rel_path(id), current) {
        (Some(rel), Some(c)) => planned(true, rel, Some(c.to_string()), None, false),
        _ => Planned {
            value: false,
            edit: None,
        },
    }
}

/// 在 `doc` 的 `covers:` 加一条关联(只动 covers、保留其余;不存在则以纯 frontmatter 新建)。
pub fn next_write_doc_link(
    doc: &str,
    target: &str,
    current: Option<&str>,
) -> Result<Planned<()>, PlanError> {
    docs::guard_doc_rel(doc).map_err(|e| PlanError::Invalid(e.to_string()))?;
    let after = docs::add_covers(current.unwrap_or(""), target);
    Ok(planned(
        (),
        doc.to_string(),
        current.map(String::from),
        Some(after),
        false,
    ))
}

/// 从 `doc` 的 `covers:` 删一条关联。`value` = 原本在不在(不存在的文档 ⇒ `false`)。
pub fn next_remove_doc_link(
    doc: &str,
    target: &str,
    current: Option<&str>,
) -> Result<Planned<bool>, PlanError> {
    docs::guard_doc_rel(doc).map_err(|e| PlanError::Invalid(e.to_string()))?;
    let Some(c) = current else {
        return Ok(Planned {
            value: false,
            edit: None,
        });
    };
    if !docs::frontmatter_covers(c).iter().any(|x| x == target) {
        return Ok(Planned {
            value: false,
            edit: None,
        });
    }
    let after = docs::remove_covers(c, target);
    Ok(planned(
        true,
        doc.to_string(),
        Some(c.to_string()),
        Some(after),
        false,
    ))
}

// ── 算的一层:读现状 + 纯的一层(只读,零写)──────────────────────────────────

/// [`next_add_annotation`] + 读现状。
pub fn plan_add_annotation(
    repo: &Path,
    file: &str,
    symbol: Option<&str>,
    body: &str,
    author: &str,
) -> Result<Planned<String>, PlanError> {
    // id 只由 (file, symbol, body) 定 ⇒ 先算落点、再读那一份。
    let rel = annotation_rel_of(&annotation_id(file, symbol, body))?;
    let current = read_current(repo, &rel)?;
    next_add_annotation(file, symbol, body, author, current.as_deref())
}

/// [`next_propose_annotation`] + 读现状。
pub fn plan_propose_annotation(
    repo: &Path,
    file: &str,
    symbol: Option<&str>,
    body: &str,
    author: &str,
) -> Result<Planned<String>, PlanError> {
    let rel = annotation_rel_of(&annotation_id(file, symbol, body))?;
    let current = read_current(repo, &rel)?;
    next_propose_annotation(file, symbol, body, author, current.as_deref())
}

/// [`next_approve_annotation`] + 读现状。
pub fn plan_approve_annotation(repo: &Path, id: &str) -> Result<Planned<bool>, PlanError> {
    let current = match annotations::rel_path(id) {
        Some(rel) => read_current(repo, &rel)?,
        None => None,
    };
    next_approve_annotation(id, current.as_deref())
}

/// [`next_remove_annotation`] + 读现状。
pub fn plan_remove_annotation(repo: &Path, id: &str) -> Result<Planned<bool>, PlanError> {
    let current = match annotations::rel_path(id) {
        Some(rel) => read_current(repo, &rel)?,
        None => None,
    };
    Ok(next_remove_annotation(id, current.as_deref()))
}

/// [`next_write_doc_link`] + 读现状。
pub fn plan_write_doc_link(repo: &Path, doc: &str, target: &str) -> Result<Planned<()>, PlanError> {
    docs::guard_doc_rel(doc).map_err(|e| PlanError::Invalid(e.to_string()))?;
    let current = read_current(repo, doc)?;
    next_write_doc_link(doc, target, current.as_deref())
}

/// [`next_remove_doc_link`] + 读现状。
pub fn plan_remove_doc_link(
    repo: &Path,
    doc: &str,
    target: &str,
) -> Result<Planned<bool>, PlanError> {
    docs::guard_doc_rel(doc).map_err(|e| PlanError::Invalid(e.to_string()))?;
    let current = read_current(repo, doc)?;
    next_remove_doc_link(doc, target, current.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_content_plans_nothing() {
        let p = next_write_doc_link("d.md", "src/a.rs", Some("---\ncovers: [src/a.rs]\n---\n"))
            .unwrap();
        assert_eq!(
            p.edit, None,
            "已经关联过 ⇒ 盘上就是想要的样子,不许产出一份 before == after 的改动"
        );
    }

    #[test]
    fn remove_of_an_absent_annotation_is_not_an_edit() {
        let p = next_remove_annotation("abc123", None);
        assert!(!p.value);
        assert_eq!(p.edit, None);
        let p = next_remove_annotation("../x", Some("{}"));
        assert!(!p.value, "非法 id 不许算出落点");
        assert_eq!(p.edit, None);
    }
}
