//! 这份后端是谁：身份（[`BUILD_ID`]）· 身份戳的拼法（[`build_stamp`]）· 二进制的名字（[`BIN_NAME`]）· 出自哪一版（[`VERSION`]）。
//!
//! 住纯工具这一层：库里谁都够得到它（本机进程内那条路没有 `main.rs`，身份不能住在只有一个宿主看得见的地方），它又不引任何上层。
//! 烤进字节的那段戳（`CC_MONITOR_BUILD_STAMP`）是这份二进制的属性，住主 crate 的 `lib.rs`，由 [`build_stamp`] 编译期拼出。
//!
//! 读它源码的只有发版核对那一侧（字节自报 == 这里的 `BUILD_ID`）：
//!   · `.github/workflows/release.yml`（内嵌校验 · 版本对账）
//!   · `tests/scripts/re-embed.sh`（`--check`）· `tests/scripts/bump-build-id.sh`（打版本号）
//!   · `tests/backend/build_id_guard.rs`（真身份住址的逐条核对）
//!   · 手机 `src/mobile/core-claude/build.gradle.kts`（手机自己的身份抄这一行）
//! monitor 不读它：monitor 的「我这一版」是手上那份内嵌字节自报的 id。

/// 后端身份（`Hello` 帧的 `build_id`，也烤进二进制的身份戳）：monitor 拿它比内嵌那份的，不同 ⇒ 那台该换后端。
/// 后端发出去的东西变了（子命令集 · 冻结格 · 形状指纹 · 行为）就打一个新的：`tests/scripts/bump-build-id.sh`，再 re-embed。
pub const BUILD_ID: &str = "p11h-one-core";

/// 这份二进制的名字（`[[bin]]` 那一格）：落盘的 `cc-monitor-backend-<build>` 也以它打头。
/// 写成字面量而不读 `env!("CARGO_PKG_NAME")`：那读的是编进这一段的包叫什么，代码换了包它就静默变成别的名字。
pub const BIN_NAME: &str = "cc-monitor-backend";

/// 这份二进制出自哪一版（仓根 `package.json` 顶层那行 `"version"`，本包 `build.rs` 编译期交）。
pub const VERSION: &str = env!("CC_MONITOR_VERSION");

// 身份戳的两个界标住契约 crate（`deploy_contract::STAMP_OPEN` / `STAMP_CLOSE`）：monitor 扫字节用的是同一份。

/// 身份戳的字节数：`<开>` ＋ `BUILD_ID` ＋ `<关>`。
pub const BUILD_STAMP_LEN: usize =
    deploy_contract::STAMP_OPEN.len() + BUILD_ID.len() + deploy_contract::STAMP_CLOSE.len();

/// 编译期把 `<开>` ＋ `BUILD_ID` ＋ `<关>` 拼成一段**定长字节**。
///
/// 🔴 **为什么必须是 `static [u8; N]` 而不是一个 `&str` 常量** —— 这一条是这里的支点，
/// 它治的是 monitor `src/frontend/shell/build.rs` 里逐字记着的那次失败（`embed_backends` 头注）：
/// 「编译器可把 BUILD_ID 优化成立即数指令（字符串在字节里**不连续**），
///  运行时 `bytes_contain` 启发式会误拒正品二进制」⇒ 当时的出路是**旁挂一份清单**，
/// 也就是「把标签抄到旁边」。
/// 一个带地址、被 `#[used]` 钉住的 `static` 数组**不可能**被拆成立即数：它有地址、要进
/// `.rodata`、字节按定义连续。⇒ 「扫字节问身份」从一条启发式变成一条**结构性成立**的事。
/// 〔实测：release + `lto` + `strip` 与 debug 测试壳两侧都扫得出，且**恰好一处**。〕
pub const fn build_stamp() -> [u8; BUILD_STAMP_LEN] {
    let mut out = [0u8; BUILD_STAMP_LEN];
    let mut i = 0usize;
    let open = deploy_contract::STAMP_OPEN.as_bytes();
    let mut j = 0usize;
    while j < open.len() {
        out[i] = open[j];
        i += 1;
        j += 1;
    }
    let id = BUILD_ID.as_bytes();
    let mut j = 0usize;
    while j < id.len() {
        out[i] = id[j];
        i += 1;
        j += 1;
    }
    let close = deploy_contract::STAMP_CLOSE.as_bytes();
    let mut j = 0usize;
    while j < close.len() {
        out[i] = close[j];
        i += 1;
        j += 1;
    }
    out
}
