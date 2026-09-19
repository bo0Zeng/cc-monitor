
fn probe_cfg() -> RemoteConfig {
    RemoteConfig {
        host: "这个主机一定不存在-audit0805".into(),
        label: "probe".into(),
        port: 1,
        user: "nobody".into(),
        key_path: None,
        daemon_path: "/tmp/nope".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// ★★ **部署入口真的过了目录围栏吗**〔audit-0805 08-08，Phase G 第 54 件〕。
///
/// `is_safe_remote_acct_iso_dir` 有直接的行为判据，但主语是**围栏本身**。
/// 08-08 实测：把 `deploy_remote_acct_iso` 里那句 `if !is_safe_…` 短路，
/// **全仓 986 条判据一条不红** —— 而那条路会往用户远端机器上的**任意目录**部署。
/// 与 F47/F48/F53 同一族（围栏有判据、接线没人钉）。
///
/// 围栏在任何 I/O 之前 ⇒ 本条跑真路：喂一个非法目录，要求**零网络**就被拒。
#[tokio::test]
async fn the_deploy_entry_point_actually_checks_the_destination() {
    let err = deploy_remote_acct_iso(probe_cfg(), "/etc".into())
        .await
        .expect_err("非法部署目录竟然没被拒 —— 围栏没接上");
    assert!(
        err.contains("部署目录不安全"),
        "拒绝了，但不是目录围栏拒的（错误：{err}）—— \
             说明它已经越过围栏去连主机了，而下一步是往那个目录里写东西。"
    );
}

/// ★ **shellinit 入口真的过了围栏吗**〔audit-0805 08-07，Phase G 第 49 件下半〕。
///
/// 下面那几条判的是 `validate_shellinit_output` **这个函数本身**（截断、空、缺标记）。
/// 08-07 实测：把 `remote_acct_iso_shellinit` 的尾表达式换成 `Ok(out)`（跳过围栏），
/// **全仓 980 条判据一条不红** —— 与删除路 / 建分支路 / 端口转发同一族的第四例。
///
/// # ⚠ 本条是**源码层**，不是真路 —— 这是刻意的降级，理由写在这里
///
/// 姐妹三条都能跑真路（围栏在 I/O 之前）。**这一条不行**：围栏在远端
/// `exec_collect` **之后**，跑真路就得有一台远端 —— 本区红线不许起真进程。
/// ⇒ 只能判「那行还在」，而**判源码是代理不是标的**（第 41 件刚记过这条教训）。
///
/// 它挡得住：有人把尾表达式改成 `Ok(out)`、或把围栏调用整个删掉。
/// 它挡不住：围栏还在但被喂了别的值（例如先把 `out` 洗一遍再交给它）。
/// 后者进 `ROADMAP §5`，解锁条件 = 把远端 exec 抽成可注入的 trait，那时能跑真路。
#[test]
fn the_shellinit_entry_point_still_ends_with_the_fence() {
    let src = include_str!("../../src/bridge/src/acct_iso_deploy.rs");
    let prod = guard_core::production_code(src);
    // 抽取器自检：切没了就零命中地绿。
    assert!(
        prod.contains("pub async fn remote_acct_iso_shellinit"),
        "生产段里没有 `remote_acct_iso_shellinit` —— 切点变了，本条会零命中地绿"
    );
    // 取那个函数的体（到下一个顶格行；`where` / `)` 顶格的算头 —— 本会话栽过三次）。
    let at = prod
        .find("pub async fn remote_acct_iso_shellinit")
        .expect("上面已确认它存在");
    let mut body = Vec::new();
    for (i, line) in prod[at..].lines().enumerate() {
        let cont = line.starts_with("where") || line.starts_with(')') || line.trim() == "{";
        if i > 0 && !line.is_empty() && !line.starts_with(char::is_whitespace) && !cont {
            break;
        }
        body.push(line);
    }
    let body = body.join("\n");
    assert!(
        body.contains("validate_shellinit_output("),
        "`remote_acct_iso_shellinit` 不再调 `validate_shellinit_output` —— \n\
             远端输出会**原样**回到前端，而那正是围栏存在的理由（fail-closed）。\n\
             ⚠ 本条只看「那行还在」（源码层，理由见头注）；\n\
             围栏还在却被喂了洗过的值，本条看不见 —— 那一半已进 `ROADMAP §5`。"
    );
}
use super::*;

/// ★ Z05 跨语言双写点守卫：`SHELLINIT_FENCE_BEGIN` 必须与 vendored `cc-acct-iso`
/// 里 `cmd_shellinit` 真正打印的那行**逐字一致**。
///
/// **为什么需要它**：Rust 侧拿这个围栏当「片段产出成功」的判据（没有它就报错、
/// 绝不把半截东西交给前端当待贴文本）。bash 那边哪天改了围栏措辞，表现是
/// **功能整体失灵但错误文案听起来像用户的错**（「远端没能产出 rc 片段」）。
///
/// 做法同 `tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync` 与
/// `accounts_query.rs` 的 Z06 守卫：`include_str!` 读 **vendored** 副本 + 锚定那一行。
/// **`cp -a` 保 mtime ⇒ 本地 re-vendor 后要 `touch` 本文件，否则判的是上次的结果**（Z06 实测）。
#[test]
fn acct_iso_shellinit_fence_matches_vendored_script() {
    let script = include_str!("../../src/bridge/vendor/cc-acct-iso/scripts/cc-acct-iso");
    for fence in [SHELLINIT_FENCE_BEGIN, SHELLINIT_FENCE_END] {
        assert!(
            script.contains(&format!("printf '{fence}\\n'")),
            "Z05 双写点漂移：vendored cc-acct-iso 里找不到打印 {fence:?} 的那行。\n\
                 Rust 侧拿这两条围栏当「片段完整」的判据，两边必须一致。"
        );
    }
    // 反向自检：断言的是「源真读进来了」，不是「命中若干条」。
    assert!(
        script.len() > 1000,
        "include_str! 没读到 vendored 脚本，上面的断言是空转"
    );
}

/// fail-closed：**半截片段绝不放行**（贴进 rc 会让登录 shell 报错）。
#[test]
fn shellinit_validation_is_fail_closed() {
    let good = format!("{SHELLINIT_FENCE_BEGIN}\nzcc() {{ :; }}\n{SHELLINIT_FENCE_END}\n");
    assert_eq!(validate_shellinit_output(good.clone()).unwrap(), good);

    // 只有 BEGIN（输出被截断）⇒ 拒，且诊断要说「不完整」而不是「没产出」
    let truncated = format!("{SHELLINIT_FENCE_BEGIN}\nzcc() {{ :; }}\n");
    let e = validate_shellinit_output(truncated).unwrap_err();
    assert!(e.contains("不完整"), "诊断该说是截断，实得：{e}");
    assert!(e.contains("别贴"), "必须明确劝阻，实得：{e}");

    // 压根没跑成（没装 / PATH 不对）⇒ 拒，诊断给可执行的下一步
    let e = validate_shellinit_output(String::new()).unwrap_err();
    assert!(e.contains("未安装"), "诊断该给下一步，实得：{e}");

    // 只有 END（不可能但也是坏数据）⇒ 拒
    assert!(validate_shellinit_output(SHELLINIT_FENCE_END.to_string()).is_err());
}

#[test]
fn safe_dir_accepts_conventional_paths() {
    assert!(is_safe_remote_acct_iso_dir(
        "/home/z/.cc-monitor/cc-acct-iso"
    ));
    assert!(is_safe_remote_acct_iso_dir("/opt/cc-acct-iso"));
    assert!(is_safe_remote_acct_iso_dir("/home/z/.cc-monitor/x")); // 含 .cc-monitor
}

#[test]
fn safe_dir_rejects_dangerous() {
    assert!(!is_safe_remote_acct_iso_dir("")); // 空
    assert!(!is_safe_remote_acct_iso_dir("/")); // 根
    assert!(!is_safe_remote_acct_iso_dir("relative/cc-acct-iso")); // 相对
    assert!(!is_safe_remote_acct_iso_dir("/home/../cc-acct-iso")); // ..
    assert!(!is_safe_remote_acct_iso_dir("/home/z/projects")); // 无标记词
    assert!(!is_safe_remote_acct_iso_dir("/tmp/evil")); // 无标记词
}

#[test]
fn vendor_id_is_nonempty_trimmed() {
    let v = vendor_id();
    assert!(!v.is_empty());
    assert_eq!(v, v.trim());
    assert!(!v.contains('\n'));
}
