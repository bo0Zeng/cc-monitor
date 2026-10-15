//! 同一台上的复制：「复制为」那个框（单选、只改名字、名字里不许带 `/`）与「复制到另一栏」都是一组一条 `files-copy`
//! （[`super::batch`]：带票 · 同名由后端探、回 `exists` 画冲突框 · 结局照后端那一行）；字节在那台机器上搬，不过网。
//! 本层只剩：这一行能不能复制 · 一件复制的路径 · 「复制为」框里的名字。

use copy_core::copy_text;

use super::source::Row;

/// 行上那颗按钮的字面。**唯一住址** —— 判据按同一个常量去找它画出来的那几个字，
/// 不在判据里手抄第二份（抄一份就会漂）。
pub static COPY_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinCopy.label.copy", &[]));

/// 这一行能不能复制。**唯一住址** —— 列表画不画那颗按钮（[`super::rows`]）
/// 与状态机接不接那一跳（[`super::shell::FileWindow::begin_copy`]），问的都是这一个函数。
///
/// **目录能复制了**：后端 `files-copy` 带 `recursive: true` 复制整棵（逐条目过路径解析）。
/// 今天不能的只剩一档：
///
/// - **有损名** —— 非 UTF-8 文件名经库有损解码之后**寻址不到真字节**，
///   一切写操作灰置（同旧面板 `panel.ts::mkRowBtn` 的 `disabled = e.lossyName`）。
pub fn is_copyable(r: &Row) -> bool {
    !r.lossy_name
}

/// 〔「有损名的…复制」〕**窗口里一行**能不能复制：名字寻址得到，或者有损但带着原始字节
/// （后端 `files-ls` 送的，`Listed::raw_name`）—— 线上那一形走字节（[`super::source::RemotePath`]）。
/// 行上那颗按钮、右键菜单、「复制到另一栏」问的都是它；[`is_copyable`] 留着给只有那五格的地方。
pub fn copyable(l: &super::source::Listed) -> bool {
    !l.lossy_name || l.raw_name.is_some()
}

/// 一件待复制：**同一台远端、同一个目录**，`from` → `to`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyJob {
    pub from: String,
    pub to: String,
    /// 显示名（＝ 目标那一侧的 basename）。
    pub name: String,
    /// 源是目录 ⇒ 线上带 `recursive: true`（复制整棵）；目录不问覆盖（不合并）。
    pub is_dir: bool,
    /// 〔有损名全寻址〕`from` / `to` 不是合法 UTF-8 时的**整条路径**原始字节（`None` ⇒ 那个串就是真字节）。
    pub from_raw: Option<Vec<u8>>,
    pub to_raw: Option<Vec<u8>>,
}

impl CopyJob {
    /// 「把 `from` 复制成同一个目录 `dir` 里的 `new_name`」。
    ///
    /// ⚠ 远端路径**恒用 `/`** 拼（同 [`super::source::parent_dir`] 那条理由：
    /// 拿 `std::path` 切远端路径，在 Windows 上会把 `\` 也当分隔符）。
    ///
    /// 三档回 `None`，**一个都不许兜底编一个名字出来**：
    /// - 空名字；
    /// - 名字里带 `/` —— 那是「复制到别处」，这里不做，更不许让人在一个
    ///   「改个名」的框里不小心写出一条别的路径（那会把文件放到他没在看的目录里）；
    /// - 目标算出来**就是源自己** —— 覆盖那一形是「写旁名、再换名顶掉目标」，
    ///   `to == from` 就是拿一份复制品顶掉源自己（后端那一侧也拒，这里先不发）。
    pub fn beside(from: &str, dir: &str, new_name: &str) -> Option<Self> {
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name.contains('/') {
            return None;
        }
        let base = dir.trim_end_matches('/');
        let to = format!("{base}/{new_name}");
        if to == from {
            return None;
        }
        Some(Self {
            from: from.to_string(),
            to,
            name: new_name.to_string(),
            is_dir: false,
            from_raw: None,
            to_raw: None,
        })
    }

    /// 源的整条路径（显示串 ＋ 可能有的字节）。
    pub fn from_path(&self) -> super::source::RemotePath {
        super::source::RemotePath::of(&self.from, self.from_raw.as_deref())
    }

    /// 目标的整条路径。
    pub fn to_path(&self) -> super::source::RemotePath {
        super::source::RemotePath::of(&self.to, self.to_raw.as_deref())
    }

    /// 同一件，标上「源是目录」。
    pub fn dir(mut self, is_dir: bool) -> Self {
        self.is_dir = is_dir;
        self
    }
}

/// 复制那条线上命令的名字。
pub const CMD_COPY: &str = "files-copy";

/// 「复制为」那个框：UI 线程自己的状态（正在输入的那个名字只有 UI 线程碰得到）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyPrompt {
    /// 源的绝对路径（＝ 那一行的 `path`）。
    pub from: String,
    /// 源在列表上的名字（只用来说话）。
    pub src_name: String,
    /// 目标落在哪个目录（＝ 当前目录）。
    pub dir: String,
    /// 正在编辑的新名字。
    pub new_name: String,
    /// 源是目录（复制整棵）。
    pub is_dir: bool,
    /// 〔有损名全寻址〕源 / 当前目录不是合法 UTF-8 时的原始字节（窗口按行与当前目录填）。
    pub from_raw: Option<Vec<u8>>,
    pub dir_raw: Option<Vec<u8>>,
}

impl CopyPrompt {
    /// 缺省的新名字 —— 同旧面板（`panel.ts::copyFile` 的 `${e.name}.copy`）。
    /// **别让两个面板两套手感。**
    pub fn suggest(name: &str) -> String {
        format!("{name}.copy")
    }

    pub fn for_row(dir: &str, r: &Row) -> Self {
        Self {
            from: r.path.clone(),
            src_name: r.name.clone(),
            dir: dir.to_string(),
            new_name: Self::suggest(&r.name),
            is_dir: r.is_dir,
            from_raw: None,
            dir_raw: None,
        }
    }

    /// 框里那个名字变成一趟真复制。名字不合法 ⇒ `None`（调用方据此**出声**）。
    pub fn to_job(&self) -> Option<CopyJob> {
        let mut job = CopyJob::beside(&self.from, &self.dir, &self.new_name)?.dir(self.is_dir);
        // 有损那一侧：源的字节原样带上；目标 ＝ 当前目录的字节 ＋ `/` ＋ 新名字（新名字恒是框里敲的 UTF-8）。
        job.from_raw = self.from_raw.clone();
        if let Some(d) = &self.dir_raw {
            let mut t = d.clone();
            if t.last() != Some(&b'/') {
                t.push(b'/');
            }
            t.extend_from_slice(self.new_name.trim().as_bytes());
            job.to_raw = Some(t);
        }
        Some(job)
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/copy_testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/copy_tests.rs"]
mod tests;
