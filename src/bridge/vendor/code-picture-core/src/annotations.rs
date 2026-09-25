//! 批注侧车存储(F07):`.codepicture/annotations/<id>.json`。
//! 侧车文件为**唯一真相**(人写、可版本化)。写 IO 面(账本 §5:IO = index/git/scan/annotations)。

use crate::edits::FileEdit;
use crate::model::Annotation;
use std::fs;
use std::path::Path;

/// 批注目录的**仓相对**住址(恒仓内,与索引落点解耦,F72)。`/` 分隔。
pub const ANNOTATIONS_REL: &str = ".codepicture/annotations";

/// 一条批注侧车文件的仓相对路径(非法 id ⇒ `None`,防路径穿越)。
pub fn rel_path(id: &str) -> Option<String> {
    valid_id(id).then(|| format!("{ANNOTATIONS_REL}/{id}.json"))
}

/// 侧车文件的全文(存储格式**只在这里**定义:pretty JSON)。
pub fn render(ann: &Annotation) -> std::io::Result<String> {
    serde_json::to_string_pretty(ann).map_err(std::io::Error::other)
}

/// 读懂一份侧车文件的全文(读不懂 ⇒ `None`,与 [`list`] 跳过损坏文件同口径)。
pub fn parse(content: &str) -> Option<Annotation> {
    serde_json::from_str(content).ok()
}

/// id 必须是非空十六进制(内容哈希)。防路径穿越:approve/remove/get 是 public API。
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 读全部批注(损坏的 json / 非 .json 跳过);按 id 排序保证确定性。**F72 起 `dir` 直接是批注
/// 目录**(`Engine.annotations_dir` = `<repo>/.codepicture/annotations`,与 index 落点解耦)。
pub fn list(dir: &Path) -> Vec<Annotation> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().map(|x| x == "json").unwrap_or(false) {
            if let Ok(content) = fs::read_to_string(&p) {
                if let Ok(a) = serde_json::from_str::<Annotation>(&content) {
                    out.push(a);
                }
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn get(dir: &Path, id: &str) -> Option<Annotation> {
    if !valid_id(id) {
        return None;
    }
    let content = fs::read_to_string(dir.join(format!("{}.json", id))).ok()?;
    parse(&content)
}

/// 写一条批注(**原子**:temp + rename,防半截丢批注);首写 lazy 建目录。**F72**:`dir` = 批注
/// 目录(恒仓内 `<repo>/.codepicture/annotations`),`.gitignore` 由 `Engine::open` 在 index 侧写、
/// 不在这里(批注目录本就该提交、不需 gitignore)。
pub fn write(dir: &Path, ann: &Annotation) -> std::io::Result<()> {
    if !valid_id(&ann.id) {
        return Err(std::io::Error::other("非法批注 id"));
    }
    write_text(&dir.join(format!("{}.json", ann.id)), &render(ann)?, true)
}

/// 原子写一份侧车全文:temp + rename(防半截丢批注);`parents` ⇒ 先建目录(首写 lazy 建)。
fn write_text(dest: &Path, text: &str, parents: bool) -> std::io::Result<()> {
    if parents {
        if let Some(d) = dest.parent() {
            fs::create_dir_all(d)?;
        }
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, dest)
}

/// **写盘那一层**(批注):把 [`crate::edits`] 算好的一份改动落到 `repo` 上 —— 与拆开前同法
/// (原子换名、首写建目录;`after = None` ⇒ 删)。只收批注目录里的落点。
pub fn apply(repo: &Path, edit: &FileEdit) -> std::io::Result<()> {
    let name = edit
        .rel
        .strip_prefix(ANNOTATIONS_REL)
        .and_then(|r| r.strip_prefix('/'))
        .and_then(|r| r.strip_suffix(".json"))
        .filter(|id| valid_id(id))
        .ok_or_else(|| std::io::Error::other(format!("不是批注侧车的落点:{}", edit.rel)))?;
    let dest = repo.join(ANNOTATIONS_REL).join(format!("{name}.json"));
    match &edit.after {
        Some(text) => write_text(&dest, text, edit.parents),
        None => match fs::remove_file(&dest) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        },
    }
}

pub fn remove(dir: &Path, id: &str) -> std::io::Result<bool> {
    if !valid_id(id) {
        return Ok(false);
    }
    let p = dir.join(format!("{}.json", id));
    if p.is_file() {
        fs::remove_file(p)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 在 `.codepicture/.gitignore` 里忽略派生的 index.db,让 annotations/ 可提交。
/// 由 `Engine::open` 调用,保证纯查询场景也保护 index.db。
pub fn ensure_gitignore(dot: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dot)?;
    let p = dot.join(".gitignore");
    if !p.exists() {
        fs::write(
            &p,
            "# 索引是派生的,别提交;批注(annotations/)是人写的,提交它\n/index.db\n",
        )?;
    }
    Ok(())
}
