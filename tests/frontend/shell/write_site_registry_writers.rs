/// 所有会写盘的函数名（去重）。**从 `WRITE_SITES` 派生**，不是手写。
pub(crate) fn names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = super::tests::WRITE_SITES
        .iter()
        .map(|(_, f, _, _)| *f)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// 一段生产代码里调到的写者。`self_file` 是调用方自己的文件名 ——
/// 它自己的写点不算「委托」（那由 `WRITE_SITES` 直接管）。
pub(crate) fn called_by(prod: &str, self_file: &str) -> Vec<&'static str> {
    super::tests::WRITE_SITES
        .iter()
        .filter(|(f, _, _, _)| *f != self_file)
        .map(|(_, fname, _, _)| *fname)
        .filter(|fname| prod.contains(&format!("{fname}(")))
        .collect()
}
