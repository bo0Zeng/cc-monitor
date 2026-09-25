use super::*;

// 🔴 **围栏那一族的判据已经不在本文件了**〔步 H2 09-21，用户裁「拆」〕。
// `guard_write` 与 `is_protected_claude_data_path` 搬去了 `crate::claude_data_fence`，
// 它们的判据跟着搬进 `tests/bridge/claude_data_fence_tests.rs`（两条原样保留、并各自
// 补了相等断言那一半）。本文件从此只放**池子自己**那几件事：死连分类 · 下载落点的围栏（〔第四波 S4〕取消登记那一条随它测的表删了）。
//
// 〔F7c 收尾 09-24〕可编辑性（`decode_editable_guards`〔散文墓碑〕）· 有损名（`lossy_name_detection`〔散文墓碑〕）·
// 排序（`list_dir_sort_dirs_first_then_lowercase`〔散文墓碑〕）· 改权限的线上形状
// （`the_chmod_attrs_never_put_a_size_on_the_wire`〔散文墓碑〕· `the_chmod_mode_is_masked_down_to_permission_bits`〔散文墓碑〕）
// 那五条随它们测的那几条池子命令一起走了：读文本 / 列目录 / 改权限今天是后端 `files-*`，
// 各自的判据住后端那棵树（`设计/60 §13b`）。

#[test]
fn dead_conn_classification() {
    assert!(looks_like_dead_conn("channel closed by peer"));
    assert!(looks_like_dead_conn("Broken pipe (os error 32)"));
    assert!(looks_like_dead_conn("connection reset"));
    assert!(!looks_like_dead_conn("No such file or directory"));
    assert!(!looks_like_dead_conn("permission denied"));
}

// 〔第四波 S4〕这里原先是「取消登记按 `ptr_eq` 摘、不误删同 id 他人」那一条：它测的那张按 id 的取消表
//   是老 Tauri 传输那一路的，最后一个用它的命令（零流量复制）退役时一起删了；传输台撤就是停订。

// ═════════════════════════════════════════════════════════════════════════════
// `设计/60 §5.4c`：`sftp_chmod` 发上线的那个 `SETSTAT` 包，**不许带 size**
// ═════════════════════════════════════════════════════════════════════════════
//
// 🔴 **它治的是本仓真机上被咬过的那一口，不是一条假想的风险。**
//
// `sftp.rs::upload_atomic` 的尾注逐字记着：「在 OpenSSH sftp-server 上 setstat
// （即便只设 permissions、`size=None`）会把刚 rename 好的文件**截断成 0 字节**」——
// 后端因此变 0 字节不可 exec → 连接 EOF → marker 变空 → 无限重部署。
// 那条注释因此逐字禁掉了「rename 之后 `set_metadata` 兜底 chmod」这一整个动作。
//
// 而 `设计/60 §5.4c` 裁定 `sftp_chmod` 时逐字写的是「协议侧没有障碍
// （`russh_sftp::client::SftpSession::set_metadata` 现成）」—— **那句话没有提到这一口**。
// ⇒ 落这条命令之前把它读到底（现打 `russh-sftp` 3.0.0 那份属性块序列化器；
// ⚠ 住址刻意不写成「文件::符号」那一形 —— 它在依赖树里不在本仓，写成那一形会被
//   `structural_scan` 当成一处本仓符号地址）：`SSH_FILEXFER_ATTR_SIZE`（`0x1`）
// 这个标志位**只在 `size.is_some()` 时才置**，`size` 字段也只在那时才写进包里。
// ⇒ 只要 `size` 是 `None`，线上那个包里既没有 SIZE 标志也没有 size 字段。
//
// ## 判法：**逐字节相等**，不是「不含某个子串」
//
// 序列化整个属性块，把**字节**与手写的期望逐字节比。这样三种改法都会红：
// ① 有人给 `chmod_attrs` 补一个 `size: Some(..)`（那正是事故成因）；
// ② 有人顺手加 uid/gid/atime/mtime（包变长、标志位变）；
// ③ `russh-sftp` 升版改了线上编码（那时该回来重读一遍这一口，而不是静默放行）。
//
// ## ⚠ 它买不到什么（如实登记，不假装覆盖）
//
// 它买的是「**线上那个包的形状**」。它**买不到**「真机上 OpenSSH 收到这个包不会截断」——
// 那要一趟真机，本仓今天没有（`sftp.rs` 那条注释所依据的 e2e 是当时跑的，今天复现不了）。
// ⇒ 本条排除的是**本仓那次事故的成因**（把 size 一起送上去），不是一个更大的声称。

// ════════════════════════════════════════════════════════════════════════
// 🔴〔2026-09-21〕下载的**本机落点**也过那道围栏 —— 而这一格此前是空的
// ════════════════════════════════════════════════════════════════════════
//
// # 洞是怎么活下来的：三张账首尾相接地推诿，链子末端一句假话
//
// | 站 | 它说什么 | 真假 |
// |---|---|---|
// | `remote_write_registry::NON_WRITING_COMMANDS` | 「写的是本机 ⇒ 不需要 Claude 数据围栏（那道围栏管的是**远端**那台机器上的文件）」 | 🔴 **假** —— `claude_data_fence` 头注写着 F03b 那一路用它判的正是一条**本机**路径 |
// | `remote_write_registry::REMOTE_WRITES` | 「本机侧那个问题属 `write_site_registry` 的管辖面，不在本表」 | 转手 |
// | `write_site_registry` | 「把远端文件落到**本地缓存**；写的**不是用户既有环境**」 | 🔴 **假** —— `local_path` 由用户在保存对话框里给，指哪写哪 |
//
// ⇒ 实况：把一个远端文件下载到 `~/.claude/projects/<proj>/<sid>.jsonl`，
// `download_inner` 先写 `.part` 再 `rename`，**原子地盖掉**那条会话记录。
// 老面板与原生窗口两条路都走它 ⇒ 围栏补在池子那一层，两条路一起修。
//
// ⚠ **这一摞不起任何连接**：`host` 是 `.invalid`（RFC 2606 保留域），DNS 就解不出来。

/// 🔴 踩线的本机落点 ⇒ 回的是**围栏那句话**，不是连接失败那句话。
///
/// # 为什么要判「是哪一句」而不只判「报错了」
///
/// 只判「报错了」买不到东西 ⇒ 判**码与那一句**，外加一条阴性对照（不踩线的开得了单）。判**那一句**才能分开两件事，
/// 而那正是「围栏排在拨线之前」这条性质在行为侧的唯一抓手。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_onto_a_live_session_file_is_refused_by_the_fence_not_by_the_network() {
    let cfg = crate::ssh_source::RemoteConfig {
        host: "example.invalid".into(),
        label: "fence-dl".into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/nonexistent".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    };
    // 夹具自证：这条路径**真的**被那道判定认作受保护（否则本条在量别的东西）。
    let protected = "/home/u/.claude/projects/dash-proj/abc-123.jsonl";
    assert!(
        crate::claude_data_fence::is_protected_claude_data_path(protected),
        "夹具那条路径不被判定为受保护 —— 本条此刻在量别的东西"
    );

    // 〔F7c 收尾 09-24〕老面板那条 Tauri 下载命令〔已删：`sftp_download`〕删了；今天下载只有一个入口 ——
    //   传输台开单（窗口经通道说 `transfer-download`）。围栏在开单那一刻就过（起跑时 `download_inner` 前再过一次）。
    let (code, e) = crate::sftp_pool::transfer_call(
        cfg.clone(),
        crate::sftp_pool::TRANSFER_DOWNLOAD,
        &serde_json::json!({ "remote_path": "/srv/whatever.txt", "local_path": protected }),
    )
    .await
    .expect_err("往一条正被 Claude 打开的会话文件上下载，竟然开得了单");
    assert_eq!(code, "refused");
    assert!(
        e.contains("拒绝写 Claude 数据源文件"),
        "拒的不是围栏那一句 —— 那说明它先去拨线了，围栏（如果有）在后面：{e}"
    );
    assert!(
        e.contains(protected),
        "拒绝那句话没带上是哪条路径，用户不知道该改什么：{e}"
    );

    // 🔴 阴性对照：同一台机器、一条**不踩线**的落点 ⇒ 开得了单（开单不拨线：起跑挂在订阅上）。
    // 少了它，上面那几比可以靠「恒回围栏那句话」全绿 —— 那时一次下载都做不成。
    let ok_dest = std::env::temp_dir().join("ccm-fence-dl-control.txt");
    let opened = crate::sftp_pool::transfer_call(
        cfg,
        crate::sftp_pool::TRANSFER_DOWNLOAD,
        &serde_json::json!({
            "remote_path": "/srv/whatever.txt",
            "local_path": ok_dest.to_string_lossy(),
        }),
    )
    .await
    .expect("一条不踩线的落点也被拒了 —— 那道判定的射程宽了");
    assert!(
        opened["id"]
            .as_str()
            .is_some_and(|i| i.starts_with("xfer-")),
        "{opened}"
    );
    // 开单不起跑 ⇒ 那条路径**一个字节都不该落地**。
    assert!(
        !ok_dest.exists() && !std::path::Path::new(&format!("{}.part", ok_dest.display())).exists(),
        "只开了单，盘上却出现了文件"
    );
}
