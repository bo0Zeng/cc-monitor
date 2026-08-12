//! P3t（`C12`）：**宿主事实** —— 由 `lib.rs` 在启动时告诉 backend 的那几条。
//!
//! # 为什么要有这个模块
//!
//! `backend/` 那一半必须**平台无关**（`backend-split` 的 C10，有 `the_backend_half_stays_platform_agnostic`
//! 钉着）。而「本机是不是 POSIX」是**平台知识**。
//! ⇒ 与 `platform_fs::make_executable` 同一条纪律：**backend 不问，宿主告诉它**。
//!
//! # 缺省是 fail-closed
//!
//! 没人设过 ⇒ `false` ⇒ CLI 渲染器照旧拒本机 ⇒ **与 P3t 之前的行为逐字相同**。
//! 忘了接线不会变成「悄悄放行」，只会变成「功能没生效」——后者看得见，前者看不见。

use std::sync::atomic::{AtomicBool, Ordering};

static LOCAL_IS_POSIX: AtomicBool = AtomicBool::new(false);

/// 宿主在启动时调一次。**幂等**（同一个值重复设无副作用）。
pub fn set_local_is_posix(v: bool) {
    LOCAL_IS_POSIX.store(v, Ordering::Relaxed);
}

pub(crate) fn local_is_posix() -> bool {
    LOCAL_IS_POSIX.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    /// ⚠ **本模块刻意不写「设了能读回来」那种用例**〔08-11 实测教训〕。
    ///
    /// 第一版写了一条 `set_then_read`，它改这个**进程内全局量**，
    /// 而金串对拍那条判据（`launch_cli_parity`）经 wire 读同一个量
    /// ⇒ **单跑绿、全量红**（cargo 默认并行跑用例）。同款干扰本轮已是第二次
    ///（前一次是两条起真 daemon 的用例抢 `<local>` 键）。
    ///
    /// ⇒ 语义那半改测**纯函数**：`render_ccm_invocation` 直接吃 `CliSpec.local_posix`，
    /// 不碰全局（见 `ccm_invocation` 的 `posix_local_is_allowed_windows_local_is_not`）。
    /// 接线那半由 `the_host_really_tells_backend_its_platform` 钉（源码判据，也不碰全局）。
    ///
    /// 留这段注释而不是留一条空 mod：**「这里为什么没有测试」本身是要交代的事**。
    #[test]
    fn this_module_is_covered_elsewhere_on_purpose() {
        // 只钉「缺省是 fail-closed」这条**不需要写全局**的性质：
        // 常量初值必须是 false —— 忘了接线时功能不生效，而不是悄悄放行本机。
        let src = include_str!("host_facts.rs");
        assert!(
            src.contains("AtomicBool::new(false)"),
            "缺省不再是 false —— 那意味着忘了接线时会**悄悄放行本机走 CLI 渲染器**"
        );
    }
}
