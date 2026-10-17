//! CLI 面的**词**：流模式的显式词 · 子命令后面的选项词（与帧面请求信封里同名的那几格同义）· 帧命令名到子命令的拼法。
//! 收发两头（本进程的 CLI 面 · 问远端那台的一跳 · 起常驻 · 资产同步）都只认这里的字面量；
//! 哪几个词算选项、哪几个算流旗标那三张表（`crate::SUBCOMMANDS` · `crate::SUBCOMMAND_OPTIONS` · `crate::STREAM_FLAGS`）是进程身份，住根上。

/// `--stream`：「我是流模式后端」的**显式词**。二进制叫 `ccm` 时零参数是「起会话」，
/// 起流那几发（monitor 远端流 / 测试连接探针 / 本机宿主 / 远端常驻子进程）一律带它打头，由 `control::ccm::intercept` 按本表分流。
/// 它不对应任何能力（老后端不认 ⇒ 按未知旗标照常进流模式，同 U6b-2 的降级）。
pub const STREAM_FLAG_EXPLICIT: &str = "--stream";

/// **看的人那一台的时区**：`--tz <IANA 名>`（任意位置；流模式与一次性 CLI 面同一个旗标）与帧面请求信封的 `tz` 同名同义：
/// 回包与推送里「几点」「今天 / 昨天」按它写，不按这台后端的钟。缺 · 缺值 · 认不得 ⇒ UTC（不拒）。
/// 常驻那条连接的 attach 行带同名一格 `tz`（`cmd_wire::attach::AttachLine`）。
pub const TZ_FLAG: &str = "--tz";

/// **「只读一行 stdin」的入口**：跟在子命令后面（`--assets-catalog-merge --stdin-line`）。
///
/// 为什么要它：远端那一跳（`remote_ask::DialRemote`，capture）**不关远端的 stdin** ⇒ 默认那种「读到 EOF」会一直等；
/// 此前的出路是把载荷拼进命令行 `printf '%s\n' '<json>' | …`，那要求远端登录 shell 认 POSIX 单引号与管道 ——
/// fish 一类不认（`'…\\…'` 在 fish 的单引号里会被当转义吃掉一个反斜杠，JSON 就坏了），而且一趟受 `sh -c` 那一个参数的上限。
/// 有了它，命令行里只剩后端路径与两个旗标（不含载荷），载荷经 capture 写进远端进程的 stdin，本入口读到换行就动手。
/// 上限同默认那一形（`control/cli_args.rs::MAX_CLI_STDIN`，超了拒、不截断）。
///
/// 住这里（argv 三分表旁边）而不住 `stream::cli`：它是 [`crate::SUBCOMMAND_OPTIONS`] 的一员；发它的一方（`asset_sync`）
/// 只该认得这个字面量，不该因此在引用图上连到 CLI 面的分派口（`target_parity_guard` 那条「够不够得着 tmux」按文件级引用图走）。
pub const STDIN_LINE_FLAG: &str = "--stdin-line";

/// **问这台的常驻**：`--resident-call --stdin-line [--within-ms <毫秒>] [--tz <IANA 名>]`，stdin 一行 `{cmd, args}`（[`crate::wire::listen::ResidentCall`]）⇒
/// 连这台家里常驻后端的套接字问那一条、stdout 一行它的回包（同 CLI 面的信封：成功 ＝ 回包 · 失败 ＝ stderr 一行失败信封、退出 2）。
/// 后端问后端（`remote_ask::ask_resident`）发它：问到的是那台热着的常驻，不是一次性的冷进程。住这里同 [`STDIN_LINE_FLAG`]：收发两头只认这个字面量。
pub const RESIDENT_CALL_FLAG: &str = "--resident-call";

/// **argv 形载荷口**：跟在子命令后面、位置不限（`--history-read --args-b64 <base64 的 JSON>`）⇒ 入参从这里取，不碰 stdin。
///
/// 为什么要它：第二个前端的执行通道**只有 stdout、写不了 stdin**，而 CLI 面上收入参的命令占了绝大多数。base64 而不是裸 JSON：
/// 一个参数里只剩 `[A-Za-z0-9+/=]`，过哪一家登录 shell 的引号都不变形（fish 吃反斜杠那一类，见 [`STDIN_LINE_FLAG`] 的头注）。
/// 与 [`STDIN_LINE_FLAG`] 二选一；上限与系统单个参数的上限见 `control/cli_args.rs::MAX_ARGS_B64_LEN`。
/// 住这里同 [`STDIN_LINE_FLAG`]：它是 [`crate::SUBCOMMAND_OPTIONS`] 的一员。
pub const ARGS_B64_FLAG: &str = "--args-b64";

/// **「给人看」那一形**：跟在 CLI 控制面那一族（`--<帧命令>`）后面 ⇒ 回包里核心写好的格（顶上 `text` · 每一处 `rows`）拼成字
/// （`control/ship_text.rs`，所有命令通用、不按业务写）。缺省仍是 JSON 进 JSON 出。住这里同 [`STDIN_LINE_FLAG`]：它是 [`crate::SUBCOMMAND_OPTIONS`] 的一员。
pub const TEXT_FLAG: &str = "--text";

/// **期限口**：跟在 CLI 控制面那一族（`--<帧命令>`）后面、位置不限（`--terminals-list --within-ms 10000`）⇒ 与帧面请求信封的 `within_ms`
/// 同名同义：发起方这一发愿意等多久（毫秒），减余量换成截止时刻，装总期限的命令都收紧到它；值不是正整数 ⇒ 当没带（同帧面那一格的宽读）。
/// 住这里同 [`STDIN_LINE_FLAG`]：它是 [`crate::SUBCOMMAND_OPTIONS`] 的一员。
pub const WITHIN_MS_FLAG: &str = "--within-ms";

/// **出口的声明口**：跟在 CLI 控制面那一族（`--<帧命令>`）后面、位置不限（`--history-read --args-b64 <…> --view <…>`）⇒ 与帧面请求信封的
/// `view` 同名同义（要哪几格 · 哪几格不要，`faces/project.rs`）。值是声明的 JSON 原样（`{` 打头），或它的 base64（过哪一家登录 shell 都不变形，
/// 同 [`ARGS_B64_FLAG`]）。住这里同 [`STDIN_LINE_FLAG`]：它是 [`crate::SUBCOMMAND_OPTIONS`] 的一员。
pub const VIEW_FLAG: &str = "--view";

/// 帧命令名 → 它的 CLI 子命令（`launch` → `--launch`）。**唯一一处拼法**：本进程的 CLI 面（`stream/cli.rs::flag_of`）
/// 与问远端那台 CLI 面的那一跳（`remote_ask::ask_json`）都经它 —— 住这里而不住 `stream::cli`，是为了让
/// `remote_ask` 不必引 `control/`（引了，按文件画的引用图就把问远端的几条命令连到 tmux 上）。
pub fn cli_flag(name: &str) -> String {
    format!("--{name}")
}
