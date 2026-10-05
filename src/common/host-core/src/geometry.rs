//! 窗口夹进工作区〔原 `src/frontend/shell/src/lib.rs`，逐字搬来〕：Tauri 那几扇窗（monitor）与文件窗口进程同一个判定、同一个类型
//! （文件窗口拿到的工作区随开窗种子交过去）。

/// 一台显示器的**工作区**（去掉任务栏 / 程序坞那一块，物理像素）。文件窗口进程拿它夹自己（随种子交过去）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// **一扇窗夹进工作区**（纯函数；物理像素）：外框放不下 ⇒ 内框缩到「工作区 − 边框与标题栏」；
/// 再把外框挪进工作区。回 `(新内框, 新外框左上)`；本来就在里面 ⇒ `None`。
/// 真机读数：屏 1280×760、工作区 712 高，主窗初始外框 780 高、设置窗 780 高 ⇒ 底边压在任务栏下（toast、测试连接最后一行看不见）。
pub fn fit_into_work_area(
    outer_pos: (i32, i32),
    outer: (u32, u32),
    inner: (u32, u32),
    work: WorkArea,
) -> Option<((u32, u32), (i32, i32))> {
    let chrome = (
        outer.0.saturating_sub(inner.0),
        outer.1.saturating_sub(inner.1),
    );
    let fitted = (outer.0.min(work.w), outer.1.min(work.h));
    let new_inner = (
        fitted.0.saturating_sub(chrome.0),
        fitted.1.saturating_sub(chrome.1),
    );
    let slide = |p: i32, lo: i32, span: u32, len: u32| -> i32 {
        let hi = lo.saturating_add(i32::try_from(span - len).unwrap_or(i32::MAX));
        p.clamp(lo, hi)
    };
    let pos = (
        slide(outer_pos.0, work.x, work.w, fitted.0),
        slide(outer_pos.1, work.y, work.h, fitted.1),
    );
    (new_inner != inner || pos != outer_pos).then_some((new_inner, pos))
}

/// **一扇窗摆在工作区正中**（纯函数；物理像素）：尺寸照 [`fit_into_work_area`] 夹，左上只由尺寸与工作区定 ——
/// 与它原来在哪无关。monitor 主窗与设置窗每次起都这么摆：不记位置时交给系统的默认落点，
/// Windows 每开一扇新窗就挪一格（重启一次左移 26 px）。回 `(新内框, 新外框左上)`。
pub fn center_in_work_area(
    outer: (u32, u32),
    inner: (u32, u32),
    work: WorkArea,
) -> ((u32, u32), (i32, i32)) {
    let chrome = (
        outer.0.saturating_sub(inner.0),
        outer.1.saturating_sub(inner.1),
    );
    let fitted = (outer.0.min(work.w), outer.1.min(work.h));
    let new_inner = (
        fitted.0.saturating_sub(chrome.0),
        fitted.1.saturating_sub(chrome.1),
    );
    let mid = |lo: i32, span: u32, len: u32| -> i32 {
        lo.saturating_add(i32::try_from((span - len) / 2).unwrap_or(0))
    };
    (
        new_inner,
        (mid(work.x, work.w, fitted.0), mid(work.y, work.h, fitted.1)),
    )
}
