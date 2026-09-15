//! `K-R55`（2026-09-11）：**「新建一个文件，并在新建那一刻就给它可执行位」**这一族平台原语。
//!
//! # 它从哪来
//!
//! 09-10 `K-W2D` 接线那一拍把这一跳**无门**写在 `sidecars/codepicture/acquire.rs::land` 里：
//! 「新建那一刻就给可执行位」的唯一入口住在 `std::os::unix` 那棵子树下，
//! **在 Windows target 上那个名字根本不存在** ⇒ daemon 从那一刻起在 Windows 上名字解析就过不了
//!（死码也救不了：`allow(dead_code)` 挡不住名字解析），而当天门禁全绿
//!（沙箱那道 `winchk` 射程是 `-p monitor`，不含本 crate）。
//! `K-R52` 给它补了一道 `#[cfg(unix)]` 门并立了 [`super::cfgless_guard`]，
//! 同时在 `land` 的头注里逐字登记了**为什么当时不搬**：
//!
//! > `readonly_guard` 的写面白名单**按文件认**，而 `OpenOptions` / `OpenOptionsExt`
//! > 这两个动词只许出现在签过字的那几个模块里 —— 本文件是其中之一，`platform/` 不是。
//! > 搬过去要同拍改 `readonly_guard.rs`（`K-R52` 写区之外）⇒ **本件不搬，已走上报口**。
//!
//! ⚠ 上面那段是**逐字引 `K-R52` 当时的话**，里面两个动词名今天住在下面的代码里，
//! 而本文件**已经在写面白名单上**了 —— 引它是为了记住这条债怎么来的，不是现在时。
//! `K-R55` 就是那一拍：白名单从 `sidecars/codepicture/acquire.rs` 改钉本文件，
//! **仍然按文件认**（`KR55D2` 逐字点名的失效方向就是「为了搬得动而放宽成按目录认」）。
//!
//! # 边界：搬过来的是**平台原语**，不是那条路的语义
//!
//! 本模块只回答「这台机器上，新建一个可执行文件这件事怎么做、做不做得了」。
//! 「落不下去之后跟用户怎么说」（`Landing` / `Face` 那两个闭集）留在
//! `sidecars/codepicture/`：那是**协议面**，换个平台一个字都不该变。
//! ⇒ 本模块的失败类型 [`LandFailed`] 刻意**不引** sidecar 那边的任何类型，
//! 适配层不该知道调用它的是谁。

use std::path::Path;

/// 落一个可执行文件**没落成**的样子。两格，刻意不合成一条。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum LandFailed {
    /// 🔴 **这台机器上这一跳没有实现** —— 与「去写了、写失败了」不是一件事。
    ///
    /// 非 unix 上没有「可执行位」这个概念，而落点的文件名也不带 `.exe`
    /// ⇒ 就算把字节原样写下去，那一份**也 exec 不起来**。
    /// ⇒ 这一臂**不写盘**，直接说没有。
    /// 🔴 刻意**不**写成「照写并回成功」—— 那就是「假装设置了可执行位」，
    /// 正是 [`super::fallback_guard`] 头注里 `pid_alive` 那个地雷的同一形：
    /// 给一个答不上来的问题编一个看起来无害的答案。
    Unsupported,
    /// 真的去写了，撞上一个 IO 错。**原样把那一类错带回去**，
    /// 怎么翻成给人看的话是调用方的事（适配层不认识那套词）。
    Io(std::io::ErrorKind),
}

/// 把 `bytes` 落到 `path` 上：**O_EXCL 新建 + 可执行位，一次写完**。
///
/// 🔴 **只准新增**：不截断、不追加、不覆盖、不改名、不删除 —— 那是只读护栏白名单层
/// 逐字要求的形状，也是 daemon 能在用户机器上写东西的**唯一**一条路。
/// 撞上一份同名的既有文件 ⇒ 失败并出声。
///
/// ⚠ 可执行位在**新建那一刻**给（`mode`），不是事后改：事后改要另一个动词
/// （`set_permissions`），而那个动词在只读白名单里根本不存在 ——
/// 让它写不出来，比事后检测强一档。
pub(crate) fn land_executable(path: &Path, bytes: &[u8]) -> Result<(), LandFailed> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true).mode(0o755);
        let mut f = match opts.open(path) {
            Ok(f) => f,
            Err(e) => return Err(LandFailed::Io(e.kind())),
        };
        match f.write_all(bytes) {
            Ok(()) => Ok(()),
            Err(e) => Err(LandFailed::Io(e.kind())),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (path, bytes);
        Err(LandFailed::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    /// ★ 非 unix 上：**说得出「本平台没实现」**，而且**没有去写盘**。
    ///
    /// ⚠ 如实写：这一条在 Linux 上**不进编译单元** ⇒ 沙箱门禁跑不到它。
    /// 它守的那一格今天由**编译器**守（`cargo check --target x86_64-pc-windows-msvc`），
    /// 本条只是让那一格在跑得到的机器上也有一句断言。
    #[test]
    #[cfg(not(unix))]
    fn off_unix_the_landing_says_it_is_not_implemented_and_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("kr55-landing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let p = dir.join("x");
        assert_eq!(
            super::land_executable(&p, b"payload"),
            Err(super::LandFailed::Unsupported),
            "非 unix 上竟然说落成了 —— 那就是「假装设置了可执行位」"
        );
        assert!(!p.exists(), "说了没实现，却在盘上留下了一份字节");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
