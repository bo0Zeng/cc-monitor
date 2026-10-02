/// ★ 围栏本身的行为：**跑出 home 的一律拒绝，home 之内的照常放行**。
///
/// ⚠ 正例那一半不是凑数：围栏收得太紧会**悄悄砍掉「其它文件」**（从前叫 `ProfileKind::Custom`〔散文墓碑〕）
/// （用户指 `~/.config/fish/config.fish` 这种），那是把一个洞换成一个回归。
#[test]
fn the_profile_fence_keeps_writes_inside_home() {
    // 围栏拆成词法（两侧）＋ 符号链接（只本机）两层，入口是 `fence_on(origin, home, raw)`；
    //   本条量的是**本机**那一侧（两层都过），用真 home 当基准只为「父目录真存在」那一格（只解路径、不读不写）。
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("测试需要 home"));
    let home_s = home.display().to_string();
    let fence = |raw: &str| super::fence(&home_s, raw);
    // 正例：home 下的裸文件名 · home 子目录 · `~` 前缀（用户会手打）· 不存在的新文件
    for ok in [
        home.join(".bashrc").to_string_lossy().to_string(),
        home.join(".config/fish/config.fish")
            .to_string_lossy()
            .to_string(),
        "~/.zshrc".to_string(),
        home.join("no-such-file-yet.rc")
            .to_string_lossy()
            .to_string(),
    ] {
        assert!(
            fence(&ok).is_ok(),
            "围栏拒了一个合法路径：{ok:?} —— 收太紧会砍掉「其它文件」这个特性"
        );
    }
    // 反例：绝对路径跑出 home · 相对路径 · `..` 逃逸
    for bad in [
        "/etc/profile".to_string(),
        "/tmp/x.rc".to_string(),
        ".bashrc".to_string(),
        format!("{}/../../etc/profile", home.to_string_lossy()),
        // ⚠ **这一条是变异逼出来的**：上一行那种 `..` 逃逸其实是被**符号链接那条腿**
        // 接住的（父目录 `/etc` 存在 ⇒ canonicalize 成功 ⇒ 前缀检查发现跑出去了）。
        // 把上级目录检查整个删掉，判据**照样绿** —— 因为反例挑得不对。
        // 父目录**不存在**时 canonicalize 会失败、那条腿被跳过，只剩这一条挡着：
        format!(
            "{}/no-such-dir/../../../etc/profile",
            home.to_string_lossy()
        ),
    ] {
        let r = fence(&bad);
        assert!(
            r.is_err(),
            "围栏放行了 {bad:?} —— 那三条命令会往它写/重写/探测存在性"
        );
        assert!(
            r.unwrap_err().starts_with("拒绝写这个配置文件"),
            "拒绝理由要能一眼看出是围栏拒的（调用方与用户都要读它）"
        );
    }
}

/// ★★ **收启动文件路径的命令都必须先过路径围栏**〔audit-0805 08-08，Phase G 第 86 件〕。
///
/// 那三条从前叫 `cc_integration_install` / `_uninstall` / `_scan_path`〔散文墓碑〕，
/// 今天是别名块的 `aliases_block_install` / `aliases_block_remove` ＋ 读回口 `aliases_read` 的「其它文件」
/// （读回口的围栏在 `account_aliases::read_in` 里过：它拿临时目录当 home 才测得了，见下面第二段）。
///
/// # 洞：本机这条路没有围栏，而远端那条有
///
/// `cc_integration_install(path: String, command_name: String, …)` 把 webview 给的路径
/// **原样** `PathBuf::from` 交给 [`install_to_profile`]（文件不存在就创建 —— 本文件
/// 另有一条判据逐字叫 `install_to_nonexistent_path_creates_file`），
/// `cc_integration_uninstall` 会**重写**那个文件，`cc_integration_scan_path` 是任意路径的〔散文墓碑〕
/// **存在性/大小探针**。三条都不看路径。
///
/// 而**远端**那条同名功能有围栏：`sftp.rs` 逐字「profile 只能是 home 下的文件名
///（如 `.bashrc` / `.zshrc`）」。⇒ **同一形态在另一半有围栏、这一半没有**
///（F52/F44 同族；后果那一侧与 F47「任意本机文件删除」同族）。
///
/// ⚠ 写进去的内容也不是全常量：`command_name` 来自调用方，会被渲进那段 shell 代码。
///
/// # 围栏取「在 home 之内」，不取「home 下的裸文件名」
///
/// 远端那条可以严到「裸文件名」，本机不行：`$PROFILE` 的候选（`shell_dialect.rs` 的 PowerShell 那一臂）
/// 本来就是 `~/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1` 这种**子目录**里的路径，
/// 而「其它文件」是产品特性（用户可以指 `~/.config/fish/config.fish`）。
/// ⇒ 围栏只挡「跑出 home」这一类，**不缩小功能**。
#[test]
fn every_profile_command_passes_through_the_fence() {
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/aliases/mod.rs"),
    )
    .expect("读不到 assets/aliases/mod.rs");
    let prod = guard_core::production_code(&lib);
    // 线上那两口（`aliases-block-install` / `-remove`）都先过 `block_target`，它再过围栏 `block::fence`。
    const CMDS: &[(&str, &str)] = &[
        ("answer_block_install", "block_target("),
        ("answer_block_remove", "block_target("),
        ("block_target", "block::fence("),
    ];
    for (cmd, fence) in CMDS {
        let at = prod.find(&format!("fn {cmd}(")).unwrap_or_else(|| {
            panic!("`assets/aliases/mod.rs` 里找不到 `{cmd}` —— 改名了就把本条一起改")
        });
        // 切到该命令函数体的收尾（花括号配平；两个字符字面量成对出现）。
        let bytes = prod.as_bytes();
        let open = (at..bytes.len())
            .find(|&i| bytes[i] == b'{')
            .expect("找不到函数体起点");
        let mut depth = 0i32;
        let mut end = bytes.len();
        for i in open..bytes.len() {
            if bytes[i] == b'{' {
                depth += 1;
            } else if bytes[i] == b'}' {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
        }
        let body = &prod[open..end];
        assert!(
            body.len() > 40,
            "`{cmd}` 切出来只有 {} 字节 —— 配平切错了，本条会零命中地绿",
            body.len()
        );
        assert!(
            body.contains(fence),
            "`{cmd}` 没有过路径围栏 `{fence}`。\n\
                 ★ 它收的是 **webview 给的任意路径**：`install` 会往那里写（不存在就创建）、\n\
                 卸会重写它、扫是存在性探针。\n\
                 ⚠ 远端那条同名功能**有**围栏（`sftp.rs`：「profile 只能是 home 下的文件名」）——\n\
                 同一形态在另一半有围栏、这一半没有，正是本区反复逮到的形状。\n\
                 实得这一段：{body:?}"
        );
    }
}

/// **围栏损坏时中止，而不是吃掉用户内容**（T04 第二步修的真 bug）。
///
/// 修前实测：`装一次` 走追加分支（用户代码还在），`装两次` 时那个损坏的 BEGIN
/// 与新块的 END 配上对，两者之间的 `function cc { }` **被整段替换掉**。
/// 远端侧（`sftp::merge_profile_block`）当初被审计 B1 要求在同一情形 Err 中止，
/// 本机侧漏了这道保护——写的都是"下次开终端就炸"级别的文件。
/// panic 也要清 tempdir。**这是我自己踩的**：用审计那两个变异反验证时测试 panic，
/// 末尾的 `remove_dir_all` 走不到，`/tmp` 下留了两个目录。`Drop` 不受 panic 影响。
struct TmpDir(std::path::PathBuf);
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tmpdir(tag: &str) -> TmpDir {
    let d = std::env::temp_dir().join(format!(
        "ccm-fence-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建 tempdir");
    TmpDir(d)
}

/// profile 的读写经「门」（生产 = 本机后端的文件管理那一面）。
/// 判据用落在临时目录上的替身门，home = 那份文件所在的目录；写的规则（备份 · 原子替换 · 回读 · 回滚）
/// 住后端，由 `files_write_tests.rs` 判。
fn door_at(p: &std::path::Path) -> super::super::tests::HomeDoor {
    super::super::tests::HomeDoor(p.parent().expect("有父目录").to_path_buf())
}
/// 后端这一族是同步的；恒等包装，判据正文不必逐处改写。
fn run<T>(x: T) -> T {
    x
}

/// **围栏损坏时一个字节都不许写** —— 真行为断言（T07 审计① 换掉的那条安慰剂）。
///
/// ## 上一版是安慰剂，两个变异实证
///
/// 上一版按**字节偏移顺序**扫自身源码（`body.find(fence) < body.find(write)`）。
/// 审计用两个**编译得过且真写盘**的变异让它保持绿：
/// ① 在围栏前插 `std::fs::write(path, "MUTANT CLOBBER\n")` —— 扫描不认这个 API 名
///    → 函数返回「已中止」而**用户文件已被清成 `"MUTANT CLOBBER\n"`**；
/// ② 把同一个 `std::fs::copy` 挪进窗口之外的 helper → **泄漏文件真的产生**。
/// 它还能被**注释文本**骗红（提到 `"atomic_write_string"` 的注释就让它 FAILED），
/// 而且 `str::find` 只取第一处 `copy`（窗口内各有 3 处）
/// —— **恰好命中真备份纯属排序运气**。
///
/// **顺序/长相是代理指标，不是性质。** 换成直接量：造一个坏围栏文件、调真函数、
/// 断言 **Err 且文件字节与调用前逐字相同**。用 tempdir，不碰任何用户文件。
#[test]
fn damaged_fence_leaves_the_file_byte_identical() {
    let td = tmpdir("bad");
    let dir = &td.0;
    let path = dir.join("Microsoft.PowerShell_profile.ps1");

    // 坏围栏：有 BEGIN、没有配对 END，**下面是用户自己的代码**
    let original = "# my stuff\n# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host mine }\n";
    std::fs::write(&path, original).expect("写 tempdir 样本");
    let before = std::fs::read(&path).expect("读原文");

    for (what, r) in [
        ("install", run(install_to_profile(&door_at(&path), &path))),
        (
            "uninstall",
            run(uninstall_from_profile(&door_at(&path), &path)),
        ),
    ] {
        // Ok 是 `()`；意外成功会被下面的字节断言 + 「应因围栏损坏中止」两条同时抓住
        let e = match r {
            Ok(()) => "(意外返回 Ok —— 围栏损坏本该中止)".to_string(),
            Err(e) => e,
        };
        let after = std::fs::read(&path).expect("读回");
        assert_eq!(
            after,
            before,
            "{what}：围栏损坏时文件必须逐字未变。实得 {:?}（错误/返回：{e}）",
            String::from_utf8_lossy(&after)
        );
        assert!(
            e.contains("找不到配对的 END"),
            "{what}：应因围栏损坏中止，实得：{e}"
        );
        // 顺带：不许留下备份/临时文件（上一版变异② 的泄漏形态）
        let leaked: Vec<String> = std::fs::read_dir(&dir)
            .expect("列 tempdir")
            .filter_map(|x| x.ok())
            .map(|x| x.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "Microsoft.PowerShell_profile.ps1")
            .collect();
        assert!(leaked.is_empty(), "{what}：不该留下 {leaked:?}");
    }
}

/// 反向自检：围栏**完好**时这条路必须真能写成（否则上一条可能是"什么都不做"而恒绿）。
#[test]
fn intact_fence_actually_writes() {
    let td = tmpdir("ok");
    let path = td.0.join("Microsoft.PowerShell_profile.ps1");
    std::fs::write(&path, "# mine\n").expect("写样本");
    let before = std::fs::read_to_string(&path).unwrap();
    run(install_to_profile(&door_at(&path), &path)).expect("围栏完好时应写成");
    let after = std::fs::read_to_string(&path).unwrap();
    assert_ne!(after, before, "围栏完好时必须真写进去");
    assert!(after.contains("# mine"), "块外内容要保留：{after}");
    assert!(after.contains("cc-monitor BEGIN"), "块要写进去：{after}");
}

#[test]
fn damaged_fence_aborts_instead_of_eating_user_content() {
    const BLOCK: &str = "# === cc-monitor BEGIN v1 ===\nNEW\n# === cc-monitor END ===";
    // 用户 profile：有个损坏的 BEGIN（上次安装中断/手改坏），**下面是用户自己的代码**
    let damaged = "# my stuff\n# === cc-monitor BEGIN v1 ===\nfunction cc { }\n";
    // **修后：第一次就 Err 中止，用户内容一个字节都不动。**
    let e = replace_or_append_block(damaged, BLOCK, "C:/x/profile.ps1").unwrap_err();
    assert!(e.contains("找不到配对的 END"), "{e}");
    assert!(e.contains("已中止"), "要让用户知道我们没动文件：{e}");
    // T04 审计⑥：`what` 现在传**真路径**而不是类别名（调用方手里一直有它）。
    // 原断言写的是类别名，与它自己的注释"要说清是哪个文件"不符。
    assert!(
        e.contains("C:/x/profile.ps1"),
        "要说清是哪个文件的真路径：{e}"
    );
    // 修前实测的退化链（留作记录，见 fenced_block 模块文档）：
    //   装一次 → 追加，用户代码还在；装两次 → 损坏的 BEGIN 与新块的 END 配对
    //   → `function cc { }` **被吃掉**。
}
use super::*;
use std::path::PathBuf;

/// PowerShell 别名块只装拉前握手 `__ccm_bind` ＋ 接上别名文件那一行：`cc` 进了清单，块里一个 `function cc` 都没有。
#[test]
fn the_powershell_block_carries_only_the_handshake() {
    let out = render_cc_code(std::path::Path::new("/_"));
    assert!(out.contains("__ccm_bind"));
    // 按词，不按子串〔§5 2l〕：语料里一出现 `function ccm`，`contains` 就假红。
    assert!(!guard_core::contains_word(&out, "function cc"));
    assert!(!out.contains("{{"), "模板里还有没填的占位：{out}");
    // v5 → v6：`function cc` 挪进清单。
    assert!(out.contains("BEGIN v6"));
    assert!(out.contains("cc-monitor END"));
}

#[test]
fn find_conflict_in_user_function() {
    let content = r#"
function Get-Stuff { Write-Host x }
function cc { Write-Host my-cc }
"#;
    // PowerShell 认同名不分大小写：清单里叫 `CC` 也算撞。
    let conflicts = find_conflicting_functions(Shell::PowerShell, content, &["CC".to_string()]);
    assert_eq!(
        conflicts,
        vec![NameClash {
            name: "CC".to_string(),
            line: 3
        }]
    );
}

#[test]
fn find_conflict_ignores_inside_ccm_block() {
    // 同名 function 在 ccm 块里 → 不算冲突（是我们自己写的）
    let content = r#"
function Get-Stuff { Write-Host x }
# === cc-monitor BEGIN v1 ===
function cc { __ccm_bind; & claude $args }
# === cc-monitor END ===
"#;
    let conflicts = find_conflicting_functions(Shell::PowerShell, content, &["cc".to_string()]);
    assert!(conflicts.is_empty());
}

#[test]
fn find_block_version_v1() {
    let content = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===\n";
    let (present, ver) = find_block_version(content, BEGIN_MARKER);
    assert!(present);
    assert_eq!(ver, Some("v1".to_string()));
}

/// ★**旧版别名块认得出来**：PowerShell 那一对块头版本串 ≠ 这一版模板的那个 ⇒ `outdated`（界面据此说「重装一次」）；
/// 刚装的那一份（同一个 `render_cc_code`）不旧；POSIX 那一对没有版本串 ⇒ 恒不旧；块不在 ⇒ 不旧。
/// 两侧异源：「这一版」从模板文件现读（`current_block_version`），「旧的」是手写的 v2 块。
#[test]
fn an_older_powershell_block_is_flagged_and_a_fresh_one_is_not() {
    let cur = current_block_version().expect("模板第一行读不出版本串");
    // v3 → v4：`__ccm_bind` 找 monitor 数据目录改走唯一出口（渲染时填，跟 `CCM_DATA_DIR`）——
    //   块内容变了就抬版本，装着 v3 的人在机器页看到「重装一次」。
    // v4 → v5：数据目录搬到 `~/.cc-monitor`，v4 块的 `$ccmDir` 是旧住址 ⇒ 装着 v4 的人也重装一次。
    // v5 → v6：`function cc` 挪进清单，块只剩握手 ＋ 接上别名文件 ⇒ 装着 v5 的人重装一次。
    assert_eq!(
        cur, "v6",
        "模板版本串变了就来改这里（并想清楚：旧块的人要不要重装）"
    );
    let ps = std::path::Path::new("/h/p.ps1");
    for old_ver in ["v2", "v3", "v4", "v5"] {
        let old = format!(
            "# === cc-monitor BEGIN {old_ver} ===\nfunction __ccm_bind {{}}\n# === cc-monitor END ===\n"
        );
        let b = block_state(ps, &old, &[]);
        assert!(b.present && b.outdated, "{old_ver}：{b:?}");
    }
    let fresh = render_cc_code(std::path::Path::new("/_"));
    let b = block_state(ps, &fresh, &[]);
    assert!(b.present && !b.outdated, "刚渲染的那一份被判成旧的：{b:?}");
    assert!(
        !block_state(ps, "# nothing\n", &[]).outdated,
        "块不在也报旧"
    );
    let rc = std::path::Path::new("/h/.bashrc");
    let posix = plan_install(Shell::Posix, "", "t", "/h").expect("装");
    let b = block_state(rc, &posix, &[]);
    assert!(
        b.present && !b.outdated && b.version.as_deref() == Some(super::POSIX_BLOCK_VERSION),
        "刚装的 POSIX 块带这一版的版本串、不报旧：{b:?}"
    );
    // 从前的 POSIX 块：BEGIN 那一行不带版本串，块里还定义着 cc / cct / cca ⇒ 认得是块、判成旧版（界面给「重新接入」）。
    let old_posix = "# mine\n# === cc-monitor remote ccm BEGIN ===\nif ! declare -f cc >/dev/null 2>&1; then\ncc()  { ccm \"$@\"; }\nfi\n# === cc-monitor remote ccm END ===\n";
    let b = block_state(rc, old_posix, &[]);
    assert!(
        b.present && b.outdated && b.version.is_none(),
        "不带版本串的旧 POSIX 块没被判成旧版：{b:?}"
    );
    // 重新接入 = 整块换成新版，块外一行不动。
    let again = plan_install(Shell::Posix, old_posix, "t", "/h").expect("重装");
    assert!(
        again.starts_with("# mine\n") && !again.contains("cc()  {"),
        "{again}"
    );
    assert!(!block_state(rc, &again, &[]).outdated);
}

/// 用户原话「**不写搬家代码、不认老路径**」；已装的块由既有的「版本不同 ⇒ 需要重装」那条认出来：
/// 装在用户 `$PROFILE` 里的上一版块（v4）把数据目录的**旧住址**写死在 `$ccmDir` 里 ⇒ 它必须被判成旧的（界面说「重装一次」），
/// 重装那一份指到新住址。旧块手写（异源），「新住址」手写。
#[test]
fn a_block_installed_with_the_old_data_dir_is_flagged_for_reinstall() {
    let ps = std::path::Path::new("/h/p.ps1");
    let installed = "# === cc-monitor BEGIN v4 ===\nfunction __ccm_bind {\n    $ccmDir = 'C:\\Users\\u\\.claude\\claudecode-frontend'\n}\n# === cc-monitor END ===\n";
    let b = block_state(ps, installed, &[]);
    assert!(
        b.present && b.outdated,
        "装着旧住址的那一版没被认出来：{b:?}"
    );
    let fresh = render_cc_code(std::path::Path::new("C:\\Users\\u\\.cc-monitor"));
    assert!(
        fresh.contains("$ccmDir = 'C:\\Users\\u\\.cc-monitor'"),
        "{fresh}"
    );
    assert!(
        !block_state(ps, &fresh, &[]).outdated,
        "重装的那一份仍被判成旧的"
    );
}

#[test]
fn replace_existing_block_keeps_other_content() {
    let existing = r#"# user stuff before
Set-Alias g git

# === cc-monitor BEGIN v1 ===
function cc { Write-Host old-version }
# === cc-monitor END ===

# user stuff after
$PSDefaultParameterValues = @{}
"#;
    let new_block =
        "# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host new-version }\n# === cc-monitor END ===";
    let out = replace_or_append_block(existing, new_block, "C:/x/profile.ps1").unwrap();
    assert!(out.contains("Set-Alias g git"));
    assert!(out.contains("$PSDefaultParameterValues = @{}"));
    assert!(out.contains("new-version"));
    assert!(!out.contains("old-version"));
}

#[test]
fn append_to_empty_profile() {
    let new_block =
        "# === cc-monitor BEGIN v1 ===\nfunction cc { Write-Host hi }\n# === cc-monitor END ===";
    let out = replace_or_append_block("", new_block, "C:/x/profile.ps1").unwrap();
    assert!(out.starts_with("# === cc-monitor BEGIN"));
    assert!(out.ends_with("END ===\n"));
}

#[test]
fn append_to_existing_profile_with_no_block() {
    let existing = "Set-Alias g git\n";
    let new_block = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===";
    let out = replace_or_append_block(existing, new_block, "C:/x/profile.ps1").unwrap();
    assert!(out.starts_with("Set-Alias g git"));
    assert!(out.contains("BEGIN v1"));
}

#[test]
fn strip_block_removes_only_block() {
    let existing = r#"Set-Alias g git
function ccm { Write-Host "我自己的，别动" }
# === cc-monitor BEGIN v1 ===
function cc { Write-Host hi }
# === cc-monitor END ===
$PSDefaultParameterValues = @{}
"#;
    let out = strip_block(existing, "C:/x/profile.ps1").unwrap();
    assert!(out.contains("Set-Alias g git"));
    assert!(out.contains("$PSDefaultParameterValues"));
    assert!(!out.contains("BEGIN"));
    // ★ **`contains` 不行，要按词**〔audit-0805 §5 2l，08-06〕：
    // 用户自己那句 `function ccm` 会让 `contains("function cc")` 命中 ⇒ **假红**
    // （代码是对的：strip_block 只该删自己的块，用户的同前缀函数必须原样留着）。
    // 上面那行语料就是为这条加的 —— 换回 `contains` 会当场红。
    assert!(!guard_core::contains_word(&out, "function cc"));
    // 反向：用户那个同前缀的函数**必须还在**。少了这一条，上面那句会被人
    // 「顺手」改回 contains 再把语料删掉，于是两边一起退回原样。
    assert!(out.contains("function ccm"), "用户自己的同前缀函数被删掉了");
}

#[test]
fn strip_block_no_op_when_no_block() {
    let content = "Set-Alias g git\n";
    assert_eq!(strip_block(content, "C:/x/profile.ps1").unwrap(), content);
}

#[test]
fn install_preserves_crlf_line_endings() {
    // Windows 用户 profile 普遍 CRLF（notepad/VSCode/git autocrlf 三大来源）。
    // 早期 .lines().join("\n") 会静默把 CRLF → LF。这里验保留。
    let crlf = "# my profile\r\nSet-Alias g git\r\nfunction prompt { 'PS> ' }\r\n";
    let new_block = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===";
    let out = replace_or_append_block(crlf, new_block, "C:/x/profile.ps1").unwrap();
    // 用户原内容仍带 CRLF
    assert!(
        out.contains("# my profile\r\n"),
        "用户首行 CRLF 丢了：{out:?}"
    );
    assert!(
        out.contains("Set-Alias g git\r\n"),
        "用户 alias CRLF 丢了：{out:?}"
    );
    // 新插入的 ccm 块也应是 CRLF
    assert!(
        out.contains("# === cc-monitor BEGIN v1 ===\r\n"),
        "新块的 BEGIN 行 EOL 不是 CRLF：{out:?}"
    );
    // 整文件**不应出现**任何裸 \n（除了作为 \r\n 的一部分）
    let bare_lf_count = out.matches('\n').count() - out.matches("\r\n").count();
    assert_eq!(bare_lf_count, 0, "出现了裸 LF，CRLF 被破坏：{out:?}");
}

#[test]
fn strip_block_preserves_crlf_line_endings() {
    let crlf = "Set-Alias g git\r\n\
                    # === cc-monitor BEGIN v1 ===\r\n\
                    function cc {}\r\n\
                    # === cc-monitor END ===\r\n\
                    function prompt { 'PS> ' }\r\n";
    let out = strip_block(crlf, "C:/x/profile.ps1").unwrap();
    assert!(out.contains("Set-Alias g git\r\n"));
    assert!(out.contains("function prompt"));
    assert!(!out.contains("BEGIN"));
    let bare_lf_count = out.matches('\n').count() - out.matches("\r\n").count();
    assert_eq!(bare_lf_count, 0, "出现了裸 LF：{out:?}");
}

#[test]
fn lf_only_file_stays_lf() {
    let lf = "Set-Alias g git\n";
    let new_block = "# === cc-monitor BEGIN v1 ===\nfunction cc {}\n# === cc-monitor END ===";
    let out = replace_or_append_block(lf, new_block, "C:/x/profile.ps1").unwrap();
    // 已是 LF 的文件不强加 CRLF
    assert!(!out.contains("\r\n"), "纯 LF 文件被改成了 CRLF：{out:?}");
}

// === v1.7.10：install / uninstall end-to-end 落地保护测试 ===

fn tmp_profile() -> PathBuf {
    let mut p = std::env::temp_dir();
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("ccm-test-{ms}-{}.ps1", std::process::id()));
    p
}

#[test]
fn install_preserves_existing_user_content() {
    let p = tmp_profile();
    let user_content = "# my profile\nSet-Alias g git\nfunction prompt { 'PS> ' }\n";
    std::fs::write(&p, user_content).unwrap();

    let door = door_at(&p);
    run(install_to_profile(&door, &p)).unwrap();

    let after = std::fs::read_to_string(&p).unwrap();
    assert!(
        after.contains("Set-Alias g git"),
        "用户原有 alias 被冲掉了！after={after}"
    );
    assert!(after.contains("function prompt"));
    assert!(after.contains("# === cc-monitor BEGIN"));
    assert!(!after.is_empty());

    // 备份由后端做（`files-put` 的 `backup: true`，判据在 `files_write_tests.rs`）；
    //   这一侧钉的是「要了备份」—— 用户的 profile 不许在没有备份的情况下被改。
    // 门就是本进程的 `files-put` ⇒ 直接看盘：旁边真留了一份备份。
    let parent = p.parent().unwrap();
    let stem = p.file_name().unwrap().to_string_lossy().to_string();
    let backups = std::fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{stem}.ccm-backup-"))
        })
        .count();
    assert!(
        backups >= 1,
        "改用户的 profile 必须要后端留一份备份（旁边一份都没有）"
    );

    // 清理
    let _ = std::fs::remove_file(&p);
    for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
        let n = entry.file_name().to_string_lossy().to_string();
        if n.starts_with(&format!("{stem}.ccm-backup-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[test]
fn install_to_nonexistent_path_creates_file() {
    let p = tmp_profile();
    // 不预先创建
    assert!(!p.exists());
    run(install_to_profile(&door_at(&p), &p)).unwrap();
    let content = std::fs::read_to_string(&p).unwrap();
    // ★ **带边界**〔audit-0805 F24 存量清账〕：`contains("function cc")` 是**正向事实钉**，
    // 而 `function ccm` 也含有它 —— 安装器若改成生成 `function ccm`（同文件 `:727` 就有
    // 那个形态），本条**照样绿**。这是「匹配单位比事实小」里最贵的那种：假绿。
    // `cc` 住清单：块里只有握手。按词判，不按子串（`function ccm` 也含 `function cc`）。
    assert!(
        !guard_core::contains_word(&content, "function cc"),
        "{content}"
    );
    assert!(
        guard_core::contains_word(&content, "function __ccm_bind"),
        "{content}"
    );
    let _ = std::fs::remove_file(&p);
}

#[test]
fn reinstall_replaces_block_keeps_user_content() {
    let p = tmp_profile();
    std::fs::write(&p, "Set-Alias g git\n").unwrap();

    run(install_to_profile(&door_at(&p), &p)).unwrap();
    // 第二次装：之前的块应该被原地替换，用户内容仍在
    run(install_to_profile(&door_at(&p), &p)).unwrap();

    let after = std::fs::read_to_string(&p).unwrap();
    assert!(after.contains("Set-Alias g git"));
    assert!(after.contains("function __ccm_bind"));
    // 只应该有一个 BEGIN 块
    assert_eq!(after.matches("# === cc-monitor BEGIN").count(), 1);

    let _ = std::fs::remove_file(&p);
    let parent = p.parent().unwrap();
    let stem = p.file_name().unwrap().to_string_lossy().to_string();
    for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
        let n = entry.file_name().to_string_lossy().to_string();
        if n.starts_with(&format!("{stem}.ccm-backup-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// 这里原来是 `install_preserves_explicit_acl_entries`〔散文墓碑〕（Windows：v1.7.10
// `ReplaceFileW` 保住 explicit ACE）。写从本进程搬到了后端（只有后端的文件管理部分写文件，本机也算），
// 那条性质跟着搬：`tests/backend/control/files_write_tests.rs::put_keeps_explicit_acl_entries_on_windows`。

#[test]
fn uninstall_strips_block_keeps_user_content() {
    let p = tmp_profile();
    let user = "# mine\nSet-Alias g git\n";
    std::fs::write(&p, user).unwrap();
    run(install_to_profile(&door_at(&p), &p)).unwrap();
    run(uninstall_from_profile(&door_at(&p), &p)).unwrap();

    let after = std::fs::read_to_string(&p).unwrap();
    assert!(after.contains("Set-Alias g git"));
    assert!(!after.contains("# === cc-monitor"));
    assert!(!after.contains("__ccm_bind"));

    let _ = std::fs::remove_file(&p);
    let parent = p.parent().unwrap();
    let stem = p.file_name().unwrap().to_string_lossy().to_string();
    for entry in std::fs::read_dir(parent).unwrap().filter_map(|e| e.ok()) {
        let n = entry.file_name().to_string_lossy().to_string();
        if n.starts_with(&format!("{stem}.ccm-backup-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R62`：本机 POSIX 那一格
// ═══════════════════════════════════════════════════════════════════════

/// 「这份文本里**有整整一行**逐字等于它」。
///
/// ⚠ 刻意不是 `contains`：那种比法的**匹配单位（子串）比事实（一整行）小**——
/// 一行被截断、或被多贴了半句，子串比法照样绿（同 `account_aliases` 那条纪律）。
fn pinned(hay: &str, needle: &str) -> bool {
    hay.lines().any(|l| l == needle)
}

/// 一份**只有裸行、一个围栏都没有**的 rc —— `K-R57` 现打用户 `~/.bashrc` 的形状。
///
/// 那 14 行里 10 行是真使用者，`名字() { ccm <修饰...> "$@"; }`，另 4 行是注释。
/// 这里照抄那个形状（名字取同一批），**并且刻意一个 BEGIN/END 都不放**。
const BARE_RC: &str = "\
# 我自己的一些设置
export EDITOR=vim

# ccm 启动器
cc()   { ccm \"$@\"; }
cct()  { ccm --ccm-tmux \"$@\"; }
oo()   { ccm --ccm-agent codex \"$@\"; }
zcc()  { ccm --account z \"$@\"; }
alias  ccx='ccm --base'

# 与 ccm 无关的一行
gs() { git status; }
";

/// ★★ `KR62D1` 的正题：**本机 POSIX 那条路与远端那个口，产物逐字节相同。**
///
/// **死值验**：让本机这一臂改用一份**自己的**副本（或自己写一套 merge）⇒ 这一条当场红，
/// 因为它比的不是「像不像」，是**恒等于远端那份实现在同一份输入上的产物**。
#[test]
fn the_local_posix_port_is_byte_for_byte_the_remote_one() {
    let what = "/home/u/.bashrc";
    for existing in ["", "export A=1\n", BARE_RC] {
        let got = plan_install(Shell::Posix, existing, what, "/h").expect("本机 POSIX 装口");
        let want = super::merge_profile_block(existing, super::CCM_WRAPPER_SNIPPET, what)
            .expect("远端那个口");
        assert_eq!(
            got, want,
            "本机 POSIX 装进 rc 的东西与远端那个口不再是同一份 —— \
                 `KR62D1` 要买的一半正是「补这一格的时候没有变成第四套」。\
                 若这里改成了一份自己的 snippet / 一套自己的 merge，就把那三套变成了四套。"
        );
        // 卸那一半同样恒等（装了又卸回得去，是同一对围栏才可能成立）。
        let back = plan_uninstall(Shell::Posix, &got, what).expect("本机 POSIX 卸口");
        assert_eq!(
            back,
            super::strip_profile_block(&got, what).expect("远端卸口"),
            "卸那一半漂了 —— 装与卸必须是同一对围栏，否则装得进去卸不干净"
        );
    }
}

/// ★★ `KR62D1`：**全树只有一份 snippet**，而且它不住在本模块。
///
/// 上一条比的是「产物一样」，一份**逐字节相同的副本**能骗过它；这一条按源码数**住址**。
/// 两条一起才关得住「不许出现第二份 snippet」。
#[test]
fn the_alias_snippet_has_exactly_one_home_in_the_rust_tree() {
    // 人群 = 两棵 Rust 树（monitor `src/frontend/shell/src` ＋ 后端 `src/backend`）：住址从 monitor 搬进了后端。
    let root = crate::guard_support::repo_root();
    let mut files =
        guard_core::scan_tree_excluding(&root.join("src/frontend/shell/src"), &["rs"], &[]);
    files.extend(guard_core::scan_tree_excluding(
        &root.join("src/backend"),
        &["rs"],
        &[],
    ));
    // ⚠ needle **拼出来**，本行不留那个宏名加左括号的字面形 —— 留了，
    //   `cross_half_edge_registry` 会把本处当成一处「解析不出路径的 include」而红
    //   （它按「宏名 + `(`」数调用，路径解析不出来就逼人登记）。
    //   判据自己不许在别人的人群里留一个假身影。
    let needle = format!("{}!(\"../../../shared/ccm-aliases.sh\")", "include_str");
    let mut homes: Vec<String> = Vec::new();
    for (path, src) in &files {
        if guard_core::production_code(src).contains(&needle) {
            homes.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert_eq!(
        homes,
        // 住址从 `sftp.rs` 搬进了本模块（别名块的真相归别名域，B §2 第 12 条）。
        vec!["block.rs".to_string()],
        "`src/shared/ccm-aliases.sh` 在 Rust 侧的住址应当**恰好一处**（`assets/aliases/block.rs` 的 \
             `CCM_WRAPPER_SNIPPET`），实得 {homes:?}。多一处就是第二份 snippet —— \
             它们会各自漂，而漂了之后本机与远端装进去的东西就不是同一个了。"
    );
}

/// ★★ `KR62D1`：**没有第二套 merge/strip，也没有第二对 POSIX 围栏。**
///
/// 从前这条钉「本模块借 `sftp.rs` 那一份（跨文件调用恰好一次）」；
/// 那一份今天搬进了本模块（`sftp.rs` 已经不做 SFTP，B §2 第 12 条），性质不变、量法换成：
/// ① 合 / 剥两个函数在本模块的生产段里**各定义恰好一次**（多一个 `fn merge_…` 就是第二套）；
/// ② 本机 POSIX 那一臂（[`plan_install`] / [`plan_uninstall`] 的函数体）**调的就是它们**；
/// ③ POSIX 那一对围栏的字面量全模块**恰好一处**（那一行 `const`）。
///
/// 死值验：在本模块里另写一个 `fn merge_posix_block(...)` 并让 `plan_install` 调它 ⇒ ② 当场红。
#[test]
fn the_posix_arm_borrows_the_remote_implementation_instead_of_growing_a_second_one() {
    let src = guard_core::production_code(include_str!(
        "../../../../src/backend/assets/aliases/block.rs"
    ));
    for def in [
        "pub(crate) fn merge_profile_block(",
        "pub(crate) fn strip_profile_block(",
    ] {
        guard_core::find_pinned(&src, def).unwrap_or_else(|e| {
            panic!("`{def}` 在本模块的生产段里应当恰好定义一次（{e}）—— 多一个就是第二套合 / 剥")
        });
    }
    let body = |sig: &str| -> String {
        let i = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {sig} —— 判据够不着被测对象了"));
        let j = src[i..].find("\n}\n").map(|k| i + k).unwrap_or(src.len());
        src[i..j].to_string()
    };
    guard_core::find_pinned(
        &body("pub(crate) fn plan_install("),
        "merge_profile_block(existing, CCM_WRAPPER_SNIPPET, what)",
    )
    .unwrap_or_else(|e| panic!("本机 POSIX 装那一臂不再走远端那一份合块（{e}）—— 第四套长出来了"));
    guard_core::find_pinned(
        &body("pub(crate) fn plan_uninstall("),
        "strip_profile_block(existing, what)",
    )
    .unwrap_or_else(|e| panic!("本机 POSIX 卸那一臂不再走远端那一份剥块（{e}）"));
    // POSIX 那一对围栏的**字面量**只许一处：那一行 const。
    guard_core::find_pinned(&src, "remote ccm BEGIN").unwrap_or_else(|e| {
        panic!(
            "POSIX 那一对围栏的字面量不是恰好一处（{e}）—— 它只许有一个住址\
             （`CCM_PROFILE_BEGIN`），抄一份就是 `K13` 那族「一个性质两把尺子」"
        )
    });
}

/// ★ `KR62D1` 的落盘那一跳：**装完之后用户原有的每一行都还在，而且块是真的块。**
#[test]
fn installing_into_a_posix_rc_keeps_every_user_line() {
    let td = tmpdir("posix-install");
    let p = td.0.join(".bashrc");
    std::fs::write(&p, BARE_RC).expect("写夹具");
    run(install_to_profile(&door_at(&p), &p)).expect("装进 POSIX rc");
    let after = std::fs::read_to_string(&p).expect("读回");
    for line in BARE_RC.lines() {
        assert!(
            pinned(&after, line),
            "用户原来那一行没了：{line:?} —— `K31`：一个字节都不许删用户的行"
        );
    }
    // 装进去的确实是那份 snippet（按整行比，不按子串）。
    for line in super::CCM_WRAPPER_SNIPPET.trim().lines() {
        assert!(pinned(&after, line), "别名块少了一行：{line:?}");
    }
    // 幂等：再装一次一个字节都不变。
    run(install_to_profile(&door_at(&p), &p)).expect("再装一次");
    assert_eq!(
        std::fs::read_to_string(&p).expect("读回"),
        after,
        "第二次安装改了文件 —— 不幂等的安装器会在 rc 里堆出两份块"
    );
    // 扫得出「已装」，而且这一格此前在 POSIX 上恒 false。
    let scan = block_state(&p, &after, &[]);
    assert!(
        scan.present,
        "装完却扫不出块 —— 界面会说「未安装」且藏起卸载按钮"
    );
    // 卸得干净，用户的行还在。
    run(uninstall_from_profile(&door_at(&p), &p)).expect("卸");
    let stripped = std::fs::read_to_string(&p).expect("读回");
    assert!(
        !stripped.contains("cc-monitor remote ccm"),
        "卸完还剩围栏残骸"
    );
    for line in BARE_RC.lines() {
        assert!(pinned(&stripped, line), "卸载吃掉了用户那一行：{line:?}");
    }
    for entry in std::fs::read_dir(&td.0).unwrap().filter_map(|e| e.ok()) {
        let _ = std::fs::remove_file(entry.path());
    }
}

/// ★ 方言按**文件是什么**定，不按**机器是什么**定；而且路径仍然由人选。
#[test]
fn the_flavour_follows_the_file_not_the_machine() {
    use std::path::Path;
    for ps in [
        "/home/u/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1",
        "C:/x/profile.PS1",
    ] {
        assert_eq!(Shell::of_target(Path::new(ps)), Shell::PowerShell, "{ps}");
    }
    for rc in [
        "/home/u/.bashrc",
        "/home/u/.zshrc",
        "/home/u/.profile",
        "/home/u/.config/fish/config.fish",
    ] {
        assert_eq!(Shell::of_target(Path::new(rc)), Shell::Posix, "{rc}");
    }
}

/// ★★ `KR62D2` 的正题：**一份没有任何围栏的 rc，那些裸行逐行指得出来。**
///
/// **死值验**：把查法换回「只认围栏」⇒ 这一条当场红。
/// 本条自带那把反向尺子（下面第一段）：**围栏那条路在同一份输入上恒空** ——
/// 所以「只加两条路径给 `scan_legacy_profiles`〔散文墓碑〕」在这里不可能变绿。
#[test]
fn bare_lines_with_no_fence_at_all_are_named_line_by_line() {
    // ① 反向尺子：围栏那条路在这份输入上**什么都看不见**。
    assert!(
        !find_block_version(BARE_RC, BEGIN_MARKER).0,
        "这份夹具里居然有围栏 —— 那它就证不了「够不着裸行」这件事"
    );
    assert_eq!(
        super::super::fence::find_pair(
            BARE_RC,
            super::CCM_PROFILE_BEGIN,
            super::CCM_PROFILE_END,
            "夹具"
        ),
        Ok(None),
        "围栏配对在这份输入上必须是「没有」—— 这正是 `K-R57` 现打用户机器的形状"
    );

    // ② 正题：逐行指名，行号与原文都要对。
    let hits = scan_legacy_rc_lines(BARE_RC);
    let got: Vec<(usize, &str)> = hits.iter().map(|h| (h.line_no, h.text.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (4, "# ccm 启动器"),
            (5, "cc()   { ccm \"$@\"; }"),
            (6, "cct()  { ccm --ccm-tmux \"$@\"; }"),
            (7, "oo()   { ccm --ccm-agent codex \"$@\"; }"),
            (8, "zcc()  { ccm --account z \"$@\"; }"),
            (9, "alias  ccx='ccm --base'"),
            (11, "# 与 ccm 无关的一行"),
        ],
        "逐行指名对不上 —— 行号错一位，用户按着它去删就会删错行"
    );

    // ③ 分类：四条函数、两条注释、一条 alias。分类错了提示的措辞就会错。
    let kinds: Vec<LegacyRcKind> = hits.iter().map(|h| h.kind).collect();
    assert_eq!(
        kinds,
        vec![
            LegacyRcKind::Comment,
            LegacyRcKind::Function,
            LegacyRcKind::Function,
            LegacyRcKind::Function,
            LegacyRcKind::Function,
            LegacyRcKind::Other,
            LegacyRcKind::Comment,
        ]
    );
    assert_eq!(
        hits.iter()
            .filter_map(|h| h.name.clone())
            .collect::<Vec<_>>(),
        vec!["cc", "cct", "oo", "zcc"]
    );

    // ④ 与 ccm 无关的行一条都不许进来（`gs() { git status; }` 与 `export EDITOR=vim`）。
    assert!(
        !hits.iter().any(|h| h.text.contains("git status")),
        "把与 ccm 无关的行也指名了 —— 那会让用户去删他自己的东西"
    );
}

/// ★ `KR62D2`：**我们自己那一块不算「你的旧行」。**
///
/// 三对围栏都要认：`profile_installer` 的、`sftp` 的、`account_aliases` 的。
/// 认漏一对 ⇒ 装完之后界面立刻回头指着我们自己刚写的那几行说「这是旧的」。
#[test]
fn our_own_fenced_block_is_never_reported_as_the_users_old_lines() {
    let what = "/home/u/.bashrc";
    let installed = plan_install(Shell::Posix, "export A=1\n", what, "/h").expect("装一次");
    let hits = scan_legacy_rc_lines(&installed);
    assert!(
        hits.is_empty(),
        "刚装完就把自己那一块指名成「你的旧行」了：{:?}",
        hits.iter().map(|h| h.line_no).collect::<Vec<_>>()
    );
    // 另两对围栏同样要让开。
    let mixed = format!(
        "# === cc-monitor aliases BEGIN v1 ===\n\
             if [ -r \"$HOME/.cc-monitor/account-aliases.sh\" ]; then . \"$HOME/.cc-monitor/account-aliases.sh\"; fi\n\
             # === cc-monitor aliases END ===\n\
             cc() {{ ccm \"$@\"; }}\n"
    );
    let hits = scan_legacy_rc_lines(&mixed);
    assert_eq!(
        hits.iter().map(|h| h.line_no).collect::<Vec<_>>(),
        vec![4],
        "围栏内外分不开 —— 只有第 4 行那条裸的才是用户自己的"
    );
}

/// ★★ `KR62D2` 的产物：**一段让用户自己动手的提示，而产品一个字节都不删。**
#[test]
fn the_hint_names_every_line_and_the_product_deletes_nothing() {
    let hits = scan_legacy_rc_lines(BARE_RC);
    let hint = render_manual_cleanup_hint("/home/u/.bashrc", &hits);
    assert!(!hint.is_empty(), "有裸行却生成了一段空提示");
    for h in &hits {
        assert!(
            hint.contains(&format!("第 {} 行", h.line_no)),
            "提示里没点名第 {} 行",
            h.line_no
        );
        assert!(
            pinned(
                &hint,
                &format!("  第 {} 行  {}", h.line_no, h.text.trim_end())
            ) || hint.lines().any(|l| l.starts_with(&format!(
                "  第 {} 行  {}",
                h.line_no,
                h.text.trim_end()
            ))),
            "提示里那一行的原文被改写了 —— 用户要照着它去自己文件里认行"
        );
    }
    // 措辞：**不许**是「请删除」。产品指名，不替人做决定。
    assert!(
        hint.contains("由你自己定"),
        "措辞必须把决定权留给用户 —— 我们够不着边界，猜一个边界去删是最坏的那条路"
    );
    assert!(
        hint.contains("一个字节都不会碰"),
        "提示要明说产品不动手（`K31` + 用户逐字「原本的配置要手动删除」）"
    );
    // 没有裸行时是空串（界面靠它决定这一块出不出现）。
    assert!(render_manual_cleanup_hint("/home/u/.bashrc", &[]).is_empty());
}

/// ★★ `K31`：**「查」这一路一个字节都不写。**
///
/// 不是读注释读出来的：真落一份夹具、扫一遍、逐字节比 + 比 mtime，
/// 并且断言目录里没多出任何文件（备份文件也算「动了机器」）。
#[test]
fn scanning_a_rc_changes_not_a_single_byte_on_disk() {
    let td = tmpdir("posix-scan");
    let p = td.0.join(".bashrc");
    std::fs::write(&p, BARE_RC).expect("写夹具");
    let before = std::fs::read(&p).expect("读");
    let before_n = std::fs::read_dir(&td.0).unwrap().count();

    // 扫一份今天走读回口那一趟（`account_aliases::rc_candidates_in`：读一次、算块的现状），
    //   把这份 rc 当「其它文件」递进去 —— 那正是界面上扫它的那条路。
    // 那一趟改走门（`rc_candidates_via`，读经那台后端的 `files-peek`）；这里的门是落在临时目录上的替身。
    let door = super::super::tests::HomeDoor(td.0.clone());
    let names = ["cc".to_string(), "zz".to_string()];
    let cands = super::super::rc_candidates_via(
        &door,
        &td.0.display().to_string(),
        Shell::Posix,
        Some(&p.display().to_string()),
        &names,
    )
    .expect("门读候选");
    let scan = &cands
        .iter()
        .find(|c| c.path == p.display().to_string())
        .expect("候选里该有这份 rc")
        .block;
    assert!(!scan.manual_cleanup_hint.is_empty(), "扫出来的提示是空的");
    assert!(!scan.present, "这份夹具里没有块");
    assert_eq!(
        scan.conflicting_functions,
        vec![NameClash {
            name: "cc".to_string(),
            line: 5
        }],
        "清单里有 `cc`，rc 第 5 行自己定义了 `cc() {{` —— 只报这一个（`zz` 没有、`cct` 不在清单里）"
    );

    assert_eq!(std::fs::read(&p).expect("读回"), before, "扫描改了文件内容");
    assert_eq!(
        std::fs::read_dir(&td.0).unwrap().count(),
        before_n,
        "扫描在目录里留下了东西（备份 / 临时文件）—— 那也是「动机器」"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴 `K-R132`：**装上了、能跑、用户敲不到** —— 这四条判据盘的是哪一半
// ═══════════════════════════════════════════════════════════════════════
//
// ## 🔴 诚实边界，先写在这里，别读大
//
// `K-R129` 那一形（**真装完、真开一个终端、真敲一次**）**这四条一条都盘不住**，
// 而且不是「今天还没写」，是**在构造上盘不住**：它要一台 Windows、要真跑一遍
// NSIS `/S` / MSI `/qn`、要开一个新会话。本门禁的 npm / tsc / e2e / cargo
// 全跑在 Linux 沙箱里 —— `tests/scripts/gate.sh` 的 `GATE_BLIND` 里
// `windows-runner` 那一条早就逐字写着这件事。
//
// ⇒ **这四条盘的是「片段生成」那一半**：我们**要写进用户 profile 的那几行**
// 长不长得对、指不指得到真落点、有没有踩 Windows 改 PATH 的那两个经典地雷。
// **「写进去之后终端里敲得到吗」仍然只有真机答得了**，本件的真机读数住
// `tests/evidence/K-R132-摸底.md`。
//
// ⚠ 别把这段话读成「所以这几条没用」：`K-R129` 那条缺陷的**直接死因**就是
// 「这几行里根本没有 PATH 这回事」，而那一半恰好是这里盘得住的。

/// ★★ 〔`KR135D1b` / `R86` 09-15〕**这一条是上一轮那条判据翻过来的那一面。**
///
/// 上一轮那条判据（`K-R132` 立的）钉的是**反过来**的事：
/// 「块里**必须有**一段把我们那个 bin 目录放上 `$env:PATH` 的代码」。
/// `R86` 把那一段删了 ⇒ **那条判据失去了对象**。件文件逐字要求
/// **别留一条永远绿的空判据** ⇒ 这里不是删掉了事，是**翻正成棘轮**：那一段不许再回来。
///
/// # 它守的不是风格，是那个「半通」状态
///
/// 会话级 `$env:PATH` 只对 PowerShell 有效（`cmd` 不读任何 profile、
/// Git Bash 读 `~/.bashrc`）⇒ 一旦有人把它加回来，`K-R129`/`K-R132` 两轮真机
/// 3×3 表里那个「**PowerShell 里能、`cmd` 里不能**」的格子当场复活，
/// 而**「看起来装好了、换个终端就没了」比「哪儿都没有」更难查**。
///
/// # 地板先证（testing.md 判据规则 7：先证够得到，否则「零违例」是空真）
///
/// 两道地板，缺一不可：① 渲染出来不是空的、围栏在、placeholder 填掉了；
/// ② **正向**那一道 —— 块里真有 `__ccm_bind`。少了 ②，
/// 「把整个模板掏空」这一刀照样让下面每一条「不含」成立。
///
/// # 诚实边界
///
/// 它只看**我们生成的那一块**。用户自己在围栏外写的任何 `$env:PATH` 一概不管
/// （那是用户的文件，`K31`）；`v3.8.0` 已经写进某人 profile 里的那段**旧块**
/// 也不在射程内 —— 那一格靠「用户再走一次装或卸就整块重写」接住，
/// 逐字住本文件上面那段横幅，**不是自动消失**。
///
/// # 死值验（`KR135D1b` 刀①）
///
/// 往 `render_cc_code` 的拼装处加回一段 `$env:PATH = "<我们的 bin 目录>;$env:PATH"` ⇒ 本条红。
#[test]
fn the_powershell_block_never_touches_the_session_path_again() {
    let word = crate::control::ccm::SUBCOMMAND_WORD;
    {
        let out = render_cc_code(std::path::Path::new("/_"));
        // ── 地板①：这份东西本身得是真的 ──────────────────────────────
        assert!(
            !out.trim().is_empty(),
            "渲染出来是空的 —— 下面每一条「不含」都成了空真"
        );
        assert!(
            out.contains(BEGIN_MARKER) && out.contains(END_MARKER),
            "围栏没了：这一块装进去就卸不掉"
        );
        assert!(!out.contains("{{"), "placeholder 没被填掉");
        // ── 地板②：正向那一道 —— 模板没被掏空 ────────────────────────
        assert!(
            guard_core::contains_word(&out, "__ccm_bind"),
            "连 `__ccm_bind` 都不在了 —— 模板被掏空，下面的「不含」全是空真\n{out}"
        );
        // ── 棘轮：会话级 PATH 那一段不许再回来 ────────────────────────
        let touches: Vec<&str> = out.lines().filter(|l| l.contains("$env:PATH")).collect();
        assert!(
            touches.is_empty(),
            "这一块又在动会话级 `$env:PATH` 了。\n\
                 `R86` 逐字把这一段删了，理由是它造出「PowerShell 里能、`cmd` 里不能」\
                 那个半通状态 —— 要让 `{word}` 在**所有**终端里都找得到，\
                 走的是**用户级** PATH 那一条（`render_user_path_setup_command`，\
                 由用户点一下）。\n实得 {} 行：\n{:#?}",
            touches.len(),
            touches
        );
        // 持久化那一族更不许出现在 profile 里：这一块每开一个会话跑一次，
        // 把写注册表放在这里 = 每开一个终端就改一次用户的环境（`K33` 明禁）。
        assert!(
            !out.contains("SetEnvironmentVariable"),
            "profile 块里出现了 `SetEnvironmentVariable` —— 这一块每开一个会话跑一次，\
                 放在这里等于每开一个终端改一次用户环境。\n{out}"
        );
    }
}

/// ★★ **两臂的 `cc` 走同一条路**：`cc` 今天是清单里首建就带的一条（[`super::super::first_aliases`]），
/// 两种方言都由同一个渲染器写出 —— POSIX `cc() { ccm "$@"; }`、PowerShell `& ccm $RemainingArgs`
/// （`K33`「所有命令只许有一处」＋ `K28`「一切对外都经后端」）。PowerShell 的别名块里**一条调用行都没有**
/// （`cc` 不再住块里）；没有 tmux 的那一臂首建不带 `cct` / `cca`（「名字在、行为不在」的壳比没有更坏，`K-R129`）。
///
/// 🔴 诚实边界：证的是「生成的文本指向 `ccm`」，不是「敲下去真起得来」（要一台 Windows）。
#[test]
fn the_powershell_cc_goes_through_ccm_exactly_like_the_posix_one() {
    use super::super::{first_aliases, render_line};
    let word = crate::control::ccm::SUBCOMMAND_WORD;
    let cc_of = |sh: Shell| {
        first_aliases(sh)
            .into_iter()
            .find(|a| a.name == "cc")
            .map(|a| render_line(&a, sh))
            .expect("首建那几条里该有 `cc`")
    };
    assert_eq!(cc_of(Shell::Posix), format!("cc() {{ {word} \"$@\"; }}"));
    let ps = cc_of(Shell::PowerShell);
    let invokes: Vec<&str> = ps
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("& "))
        .collect();
    assert_eq!(invokes, vec![format!("& {word} $RemainingArgs")], "{ps}");
    let block = render_cc_code(std::path::Path::new("/_"));
    assert!(
        !block.lines().any(|l| l.trim().starts_with("& ")),
        "别名块里又有调用行了 —— `cc` 住清单\n{block}"
    );
    let names =
        |sh: Shell| -> Vec<String> { first_aliases(sh).into_iter().map(|a| a.name).collect() };
    assert_eq!(names(Shell::Posix), ["cc", "cct", "cca"]);
    assert_eq!(names(Shell::PowerShell), ["cc"]);
}

/// ★★ `KR132D2` 的第三半，**真机逮到的那一条**：
/// **PowerShell profile 落盘要带 BOM，POSIX rc 一个字节都不许带。**
///
/// # 这不是风格，是真机上一条「静默吞掉一行代码」的路
///
/// win11 真机现打（读数住 `tests/evidence/K-R132-摸底.md`）：
/// 本块写成不带 BOM 的 UTF-8 之后，Windows PowerShell 5.1 按系统 ANSI 代码页
/// （那台机器是 GBK）解它，一行 CJK 注释**把它下面那一行吃掉** ——
/// `$ccmBinDir = …` 落进了注释里，于是 `$env:PATH` 被赋成 `";" + 原值`，
/// 而 `parse-errors=0`、**一个报错都没有**。
/// 同一趟还现打到：发出去的 `v3.8.0` 模板里 `$ccmDir` / `$deadline` / `$oldTitle`
/// 三处赋值也是这么被吃掉的 ⇒ CJK 代码页上「终端集成」那套绑定一直是坏的。
///
/// # 诚实边界（别读大）
///
/// - **这条判据证不了「PS 5.1 从此解对了」** —— 那要一台 Windows，而本门禁
///   跑在 Linux 沙箱里（`GATE_BLIND` 的 `windows-runner`）。它证的是
///   **我们落盘的字节形状对**，而「形状对 ⇒ PS 解得对」那一步是真机量出来的。
/// - **分母是一台机器**（系统代码页 GBK）。英文代码页上那条机制不成立。
///
/// # 死值验（`KR132D2` 刀②）
///
/// 把 `shell_dialect.rs` 里 PowerShell 那一臂的 `encode_for_disk` 改成原样返回 ⇒ 本条红。
#[test]
fn the_powershell_profile_lands_with_a_bom_and_the_posix_rc_never_does() {
    let td = tmpdir("bom");
    // ── PowerShell 那一支：有 BOM，而且装两趟只有一个 ────────────────
    let ps = td.0.join("Microsoft.PowerShell_profile.ps1");
    std::fs::write(&ps, "# 我自己的一行\nWrite-Host hi\n").expect("写夹具");
    run(install_to_profile(&door_at(&ps), &ps)).expect("装第一趟");
    let b1 = std::fs::read(&ps).expect("读回");
    assert_eq!(
        &b1[..3],
        &[0xEF, 0xBB, 0xBF],
        "PowerShell profile 落盘没有 BOM —— PS 5.1 会按 ANSI 代码页解它，\
             而那条路在真机上**吃掉过一整行可执行代码**（见本判据头注）"
    );
    run(install_to_profile(&door_at(&ps), &ps)).expect("装第二趟");
    let b2 = std::fs::read(&ps).expect("读回");
    assert_eq!(
        b2, b1,
        "装两趟字节不一样 —— 幂等破了（多半是 BOM 叠了一层）"
    );
    assert_eq!(
        b2.windows(3).filter(|w| *w == [0xEF, 0xBB, 0xBF]).count(),
        1,
        "文件里有不止一个 BOM —— 剥 BOM 那一跳漏了某条路"
    );
    // 用户块外的内容一个字节都没丢（BOM 这一改最容易伤到的就是它）。
    let text = String::from_utf8(b2.clone()).expect("UTF-8");
    assert!(pinned(strip_bom(&text), "# 我自己的一行"), "用户内容丢了");
    assert!(pinned(&text, "Write-Host hi"), "用户内容丢了");
    // 卸干净之后 BOM 还在、块没了、用户内容还在。
    run(uninstall_from_profile(&door_at(&ps), &ps)).expect("卸");
    let after = std::fs::read_to_string(&ps).expect("读回");
    assert!(!after.contains(BEGIN_MARKER), "卸了之后围栏还在：\n{after}");
    assert!(pinned(&after, "Write-Host hi"), "卸载吃掉了用户内容");

    // ── POSIX 那一支：一个 BOM 都不许有 ──────────────────────────────
    let rc = td.0.join(".bashrc");
    std::fs::write(&rc, BARE_RC).expect("写夹具");
    run(install_to_profile(&door_at(&rc), &rc)).expect("装 rc");
    let rb = std::fs::read(&rc).expect("读回");
    assert_ne!(
        &rb[..3],
        &[0xEF, 0xBB, 0xBF],
        "往 POSIX rc 里写了 BOM —— `sh` 会把那三个字节当成一条命令，\
             这是把 PowerShell 那一侧的药灌给了另一个病人"
    );
    assert!(
        !rb.windows(3).any(|w| w == [0xEF, 0xBB, 0xBF]),
        "rc 里任何位置都不许出现 BOM"
    );
}

/// 🔴 **P4**：别名块的**预览就是写的那一份** —— `render_block` 与「往一份空文件里装一次」之后
/// 盘上那份（BOM 剥掉）逐字相等。两种方言 × 要不要连 `cc` 函数，四格都走。
///
/// 这不是两份拼法对拍：两边调的是同一个 `plan_install`。它钉的是「预览没有另起一条拼法」这件事 ——
/// 从前的预览（`render_cc_code`）只会 PowerShell 那一块、而且不带装那一跳会加的排版，POSIX 那一块根本没有预览。
#[test]
fn the_block_preview_is_byte_for_byte_what_an_install_writes() {
    for (shell, name) in [(Shell::Posix, ".bashrc"), (Shell::PowerShell, "p.ps1")] {
        {
            let td = tmpdir("block-preview");
            let p = td.0.join(name);
            // home 与装那一跳同一个（`__ccm_bind` 的数据目录按那台的 home 推）。
            let preview = render_block(shell, &td.0.display().to_string()).expect("渲染");
            run(install_to_profile(&door_at(&p), &p)).expect("装进空文件");
            let disk = std::fs::read_to_string(&p).expect("读回");
            assert_eq!(
                strip_bom(&disk),
                preview,
                "{shell:?}：预览与写下的不是同一份"
            );
            // 反空真：两边都是空串也「逐字相等」—— 预览必须真是一块围栏（首行就是那个方言的 BEGIN）。
            let fence = match shell {
                Shell::PowerShell => BEGIN_MARKER,
                Shell::Posix => super::CCM_PROFILE_BEGIN,
            };
            assert!(
                preview.lines().next().is_some_and(|l| l.starts_with(fence)),
                "{shell:?}：预览的第一行不是那个方言的 BEGIN：{preview:?}"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 别名块的内容与合 / 剥 —— 这几条随被测对象从 `sftp_tests.rs` 搬来（逐字，只改住址）
// ═══════════════════════════════════════════════════════════════════════════

/// 单一来源漂移守卫①：写进 rc 的**别名块**只管接入 —— `ccm` 进 PATH ＋ 接上别名文件（`cc` / `cct` / `cca` 住清单）。
#[test]
fn ccm_aliases_snippet_has_required_elements() {
    for needle in [
        // CLI 落点必须进 PATH，否则别名全指向不存在的命令。
        "export PATH=\"$HOME/.cc-monitor/bin:$PATH\"",
        // 接上别名文件（文件不在时什么都不做）。
        "if [ -r \"$HOME/.cc-monitor/aliases.sh\" ]; then . \"$HOME/.cc-monitor/aliases.sh\"; fi",
    ] {
        assert!(
            CCM_WRAPPER_SNIPPET.contains(needle),
            "别名块缺关键要素: {needle}"
        );
    }
    // 别名块**不得**再含实现（那是 CLI 的事；混回来就又变成两套实现）。
    for forbidden in [
        "__ccm_rbind()",
        "exec claude",
        "tmux new-session",
        "declare -f",
        "cc()",
    ] {
        assert!(
            !CCM_WRAPPER_SNIPPET.contains(forbidden),
            "别名块不该含实现细节 {forbidden}——实现属于 ~/.local/bin/ccm"
        );
    }
}

/// `KR58D2` —— `src/doc/IPC-PROTOCOL.md` §11 里描述别名块的那一句，**行数与名单同句**。
///
/// 本区最高频的那条病就是「数与名单同句、只改一半」⇒ 这里**两样一起对**，
/// 而且两样都**现算**自源头 [`CCM_WRAPPER_SNIPPET`]（= `src/shared/ccm-aliases.sh` 本身），
/// 判据里不抄第二份名单、不写死行数。
///
/// ⚠ **它买到的射程只有这一句**：§11 其余部分（`shared/ccm` · `CCM_CLI_SCRIPT`）
/// 在 `K-R48` 第二拍之后已经是**存量馊话**，本判据够不着，也不假装够得着。
///
/// ⚠ 判据够不着被测对象时必须**响亮地红**，不许变成空真 ⇒ 找不到那一句就 panic。
#[test]
fn the_protocol_doc_sentence_about_the_alias_block_matches_the_file() {
    const IPC_DOC: &str = include_str!("../../../../src/doc/IPC-PROTOCOL.md");
    let defined: Vec<String> = CCM_WRAPPER_SNIPPET
        .lines()
        .filter_map(|l| Shell::Posix.dialect().declared_function(l.trim_start()))
        .collect();
    let want_lines = CCM_WRAPPER_SNIPPET.lines().count();

    let sent = IPC_DOC
        .lines()
        .find(|l| l.contains("src/shared/ccm-aliases.sh`，**"))
        .expect(
            "src/doc/IPC-PROTOCOL.md 里描述别名块的那一句找不到了 —— \
                 要么它被改写了、要么被删了；无论哪种，这条对账现在是瞎的",
        );
    let bold = sent
        .split("**")
        .nth(1)
        .expect("那一句里的粗体段没了 —— 对账抓不到数与名单");

    assert!(
        bold.contains(&format!("{want_lines} 行")),
        "行数对不上：src/shared/ccm-aliases.sh 现在 {want_lines} 行，而文档那句写的是「{bold}」"
    );
    // 块今天一个别名都不定义（`cc` / `cct` / `cca` 住清单）—— 文档那句要照实说，块里长回函数就要改那句。
    assert_eq!(
        bold.contains("一个别名都不定义"),
        defined.is_empty(),
        "块里定义了 {defined:?}，而文档那句写的是「{bold}」"
    );
}

#[test]
fn merge_profile_block_append_replace_idempotent() {
    let snippet = "ccm() { :; }";
    // 空 existing → 仅块。
    let m1 = merge_profile_block("", snippet, "远端 ~/.bashrc").unwrap();
    assert!(m1.contains(CCM_PROFILE_BEGIN));
    assert!(m1.contains("ccm() { :; }"));
    assert!(m1.contains(CCM_PROFILE_END));

    // 无块 → 追加，原内容保留在前。
    let existing = "export PATH=/x\nalias ll='ls -l'\n";
    let m2 = merge_profile_block(existing, snippet, "远端 ~/.bashrc").unwrap();
    assert!(m2.starts_with(existing), "块外内容保留在前");
    assert!(m2.contains(CCM_PROFILE_BEGIN));

    // 幂等：同 snippet 再 merge 不变。
    assert_eq!(
        merge_profile_block(&m2, snippet, "远端 ~/.bashrc").unwrap(),
        m2,
        "merge∘merge == merge"
    );

    // 重装（换 snippet 内容）→ 整块替换，只有一个块，块外内容仍保留。
    let m3 = merge_profile_block(&m2, "ccm() { echo new; }", "远端 ~/.bashrc").unwrap();
    assert!(m3.starts_with(existing), "重装仍保留块外内容");
    assert!(
        m3.contains("echo new") && !m3.contains("{ :; }"),
        "块被整块替换"
    );
    assert_eq!(m3.matches(CCM_PROFILE_BEGIN).count(), 1, "重装不重复加块");
}

/// 审计 B1 回归：块外内容（含块**后**的用户内容）在替换时绝不丢。
#[test]
fn merge_profile_block_preserves_content_after_block() {
    let existing =
        format!("head_line\n{CCM_PROFILE_BEGIN}\nold()\n{CCM_PROFILE_END}\ntail_user_line\n");
    let m = merge_profile_block(&existing, "ccm() { echo new; }", "远端 ~/.bashrc").unwrap();
    assert!(m.contains("head_line"), "块前内容保留");
    assert!(
        m.contains("tail_user_line"),
        "块后用户内容保留（B1 不能吞掉）"
    );
    assert!(m.contains("echo new") && !m.contains("old()"), "块整块替换");
    assert_eq!(m.matches(CCM_PROFILE_BEGIN).count(), 1);
}

/// 审计 B1 核心：BEGIN 存在但其后无 END（损坏/截断）→ Err 中止，**绝不**误配前面的 END
/// 而吞掉用户内容。
#[test]
fn merge_profile_block_aborts_on_orphan_begin() {
    // END 在前、孤立 BEGIN 在后无配对 END：独立 find 会误配 → 旧实现吞内容。新实现报错。
    let corrupt = format!("{CCM_PROFILE_END}\nuser_a\n{CCM_PROFILE_BEGIN}\nuser_b\n");
    assert!(
        merge_profile_block(&corrupt, "ccm() { :; }", "远端 ~/.bashrc").is_err(),
        "孤立 BEGIN（其后无 END）必须中止而非吞内容"
    );
    // 纯孤立 BEGIN（截断的安装）→ Err。
    let truncated = format!("user_x\n{CCM_PROFILE_BEGIN}\nhalf");
    assert!(merge_profile_block(&truncated, "ccm() { :; }", "远端 ~/.bashrc").is_err());
}

#[test]
fn strip_removes_paired_block_keeps_surrounding() {
    let s = format!("head\n{CCM_PROFILE_BEGIN}\nccm() {{ :; }}\n{CCM_PROFILE_END}\ntail\n");
    let out = strip_profile_block(&s, "远端 ~/.bashrc").unwrap();
    assert_eq!(out, "head\ntail\n");
    assert!(!out.contains(CCM_PROFILE_BEGIN));
    // 幂等：再 strip 不变
    assert_eq!(strip_profile_block(&out, "远端 ~/.bashrc").unwrap(), out);
}

#[test]
fn strip_noop_when_no_block() {
    let s = "just user content\nno block here\n";
    assert_eq!(strip_profile_block(s, "远端 ~/.bashrc").unwrap(), s);
}

/// **T04 审计②：迁移后远端这三个边界的语义确实变了，逐条锁死。**
/// 我原话"判定没变"已被实测证伪——写在这里免得下次又当成"没变"。
#[test]
fn remote_merge_boundary_semantics_after_migration() {
    let snip = "ccm() { :; }";
    // ① 行内 marker 不再命中 → 追加，且**用户那两行 echo 一个字节都不动**
    //    （旧实现会切断第一行、吃掉第二行——远端一个未申报就修掉的数据丢失）
    let inline =
        format!("a\necho \"{CCM_PROFILE_BEGIN}\"\necho \"{CCM_PROFILE_END}\"\nuser code\n");
    let got = merge_profile_block(&inline, snip, "远端 ~/.bashrc").unwrap();
    assert!(got.starts_with(&inline), "块外内容必须逐字保留：{got}");
    assert!(got.contains(snip));
    // ② BEGIN 与 END 同一行 → 现在 Err（**退化，如实记**：旧实现能替换该行）
    let same_line = format!("a\n{CCM_PROFILE_BEGIN} {CCM_PROFILE_END}\nb\n");
    let e = merge_profile_block(&same_line, snip, "远端 ~/.bashrc").unwrap_err();
    assert!(e.contains("找不到配对的 END"), "{e}");
    // ③ 缩进 marker → 归一到列 0（旧实现保留 BEGIN 缩进、丢 END 缩进，不自洽）
    let indented = format!("a\n  {CCM_PROFILE_BEGIN}\nold\n\t{CCM_PROFILE_END}\nb\n");
    let got = merge_profile_block(&indented, snip, "远端 ~/.bashrc").unwrap();
    assert!(
        got.contains(&format!(
            "\n{CCM_PROFILE_BEGIN} {} ===\n",
            super::POSIX_BLOCK_VERSION
        )),
        "缩进应归一到列 0：{got}"
    );
    assert!(
        got.starts_with("a\n") && got.ends_with("b\n"),
        "块外保留：{got}"
    );
}

#[test]
fn strip_aborts_on_malformed_begin_without_end() {
    // **这条测试原先把 bug 编码进去了**（T04 审计阻塞）：它断言悬空 BEGIN 时
    // strip 是 no-op，而调用方据此打印「远端 … 没有 ccm 块，无需卸载」——
    // 那正是同一个 commit 里被定义为 bug 的形态，只是发生在「卸」这半边。
    // 现在两侧的装与卸四条路全走 `find_pair`，此处必须 Err 中止。
    let corrupt = format!("a\n{CCM_PROFILE_BEGIN}\nccm() {{ :; }}\nuser code\n");
    let e = strip_profile_block(&corrupt, "远端 ~/.bashrc").unwrap_err();
    assert!(e.contains("找不到配对的 END"), "{e}");
    assert!(e.contains("已中止"), "要让用户知道我们没动文件：{e}");
    assert!(e.contains("远端 ~/.bashrc"), "要说清是哪个文件：{e}");
    // 而**没有** BEGIN 时仍是正常的 no-op（别把这条也变成错误）
    assert_eq!(
        strip_profile_block("just user code\n", "远端 ~/.bashrc").unwrap(),
        "just user code\n"
    );
}

/// 🔴 **别名块只有一个写口**：`user_files::edit`（→ 那台机器后端的 `files-put`）。
///
/// 要求：「写入 —— **那台机器后端的文件管理那一面**（带围栏，本机和远端同一条路）」·
/// `§4.5`「围栏管理 · 备份 · 原子写 · 写后校验 · 回滚 —— 这些是『判定的规则』，只许一份」。
///
/// 人群（从源码现打，monitor 生产段全树）：函数体里碰「别名块内容」的函数 —— 调合 / 剥 / 计划装卸 / 装卸入口
/// （`merge_profile_block(` · `strip_profile_block(` · `plan_install(` · `plan_uninstall(` · `install_to_profile(` ·
/// `uninstall_from_profile(`）的那几个。两向相等于下面这张手写表（异源：表是人按角色写的，右边是源码现扫）：
/// - **写的**（两个：远端那两条命令并进了转交那两条）函数体里必须恰好一处 `crate::user_files::edit(`；
/// - **纯规划 / 预览**（三个）与**转交**（两条命令，交给写的那两个）一处写原语都不许有；
/// - 全体都不许碰别的写原语（`.put(` · `std::fs::write` · `fenced_block::apply`〔散文墓碑〕）。
///
/// 同波别的路长出新成员时：再加一个碰别名块的函数 ⇒ 集合不等 ⇒ 红，去表里登记它的角色（写的得经 `edit`）。
#[test]
fn the_alias_block_is_written_through_exactly_one_door() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Role {
        Writes,
        Pure,
        Delegates,
    }
    const TABLE: &[(&str, &str, Role)] = &[
        ("assets/aliases/block.rs", "plan_install", Role::Pure),
        ("assets/aliases/block.rs", "plan_uninstall", Role::Pure),
        ("assets/aliases/block.rs", "render_block", Role::Pure),
        (
            "assets/aliases/block.rs",
            "install_to_profile",
            Role::Writes,
        ),
        (
            "assets/aliases/block.rs",
            "uninstall_from_profile",
            Role::Writes,
        ),
        // 转交那几条是线上那几口（`aliases-block-*`，住 `assets/aliases/mod.rs`）。
        (
            "assets/aliases/mod.rs",
            "answer_block_install",
            Role::Delegates,
        ),
        (
            "assets/aliases/mod.rs",
            "answer_block_remove",
            Role::Delegates,
        ),
    ];
    const TOUCHES: &[&str] = &[
        "merge_profile_block(",
        "strip_profile_block(",
        "plan_install(",
        "plan_uninstall(",
        "install_to_profile(",
        "uninstall_from_profile(",
    ];
    const OTHER_WRITES: &[&str] = &[".put(", "std::fs::write", "fenced_block::apply"];
    // 人群 = 后端生产段全树（别名块整族搬进来了）。
    let root = crate::guard_support::repo_root().join("src/backend");
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    let mut seen: Vec<(String, String, String)> = Vec::new(); // (文件, 函数, 函数体)
    let mut fns_scanned = 0usize;
    for (path, raw) in &files {
        let rel = guard_core::module_address(&root, path);
        let prod = guard_core::production_code(raw);
        // 顶层函数：从 `fn 名(` / `fn 名<` 到下一个列 0 的 `}`。
        let mut at = 0usize;
        while let Some(k) = prod[at..].find("fn ") {
            let s = at + k;
            at = s + 3;
            let head_ok = s == 0
                || prod[..s].ends_with('\n')
                || prod[..s].ends_with("pub ")
                || prod[..s].ends_with("async ")
                || prod[..s].ends_with("pub(crate) ");
            if !head_ok {
                continue;
            }
            let name: String = prod[s + 3..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let Some(open) = prod[s..].find('{') else {
                continue;
            };
            let end = prod[s..].find("\n}\n").map_or(prod.len(), |e| s + e);
            let body = &prod[s + open..end.max(s + open)];
            fns_scanned += 1;
            if TOUCHES.iter().any(|t| body.contains(t)) {
                seen.push((rel.clone(), name, body.to_string()));
            }
        }
    }
    // 反空真：扫描器真的走过一大片函数，而且认得出表里的每一个。
    assert!(
        fns_scanned > 500,
        "只扫到 {fns_scanned} 个函数 —— 扫描器坏了"
    );
    let got: std::collections::BTreeSet<(String, String)> = seen
        .iter()
        .map(|(f, n, _)| (f.clone(), n.clone()))
        .collect();
    let want: std::collections::BTreeSet<(String, String)> = TABLE
        .iter()
        .map(|(f, n, _)| (f.to_string(), n.to_string()))
        .collect();
    assert_eq!(
        got, want,
        "碰别名块内容的函数 != 登记表（两向）—— 新长出来的那个先说清它的角色（写的必须经 `user_files::edit`）"
    );
    for (f, n, body) in &seen {
        let role = TABLE
            .iter()
            .find(|(tf, tn, _)| tf == f && tn == n)
            .unwrap()
            .2;
        match role {
            Role::Writes => {
                guard_core::find_pinned(body, "door::edit(").unwrap_or_else(|e| {
                    panic!("{f}::{n} 写别名块却不是恰好一处经 `door::edit`（{e}）")
                });
            }
            Role::Pure | Role::Delegates => assert!(
                !guard_core::contains_word(body, "edit"),
                "{f}::{n} 登记成不写，函数体里却有 `edit`（读 · 算 · 交那一跳）"
            ),
        }
        for w in OTHER_WRITES {
            assert!(
                !body.contains(w),
                "{f}::{n} 碰别名块，又用了另一个写原语 `{w}` —— 写口只许一个"
            );
        }
    }
}

/// 🔴 C1：**`__ccm_bind` 找 monitor 数据目录只有一个住址**（`paths::resolve_monitor_data_dir`，跟 `CCM_DATA_DIR`）。
///
/// 要求住址：`INVARIANTS §2`「monitor 自己的 data dir 永远是 `~/.cc-monitor/`」那一节的出口
/// （`paths.rs` 头注：`CCM_DATA_DIR` 只为「把这个进程整体挪到别处跑」而存在）；（模板写死一份 ⇒ 数据目录的第二个住址）。
/// ① 渲染出来的块里 `$ccmDir =` 恰一行、值 == 喂进去的那个目录（带单引号的路径逐字转义成 PowerShell 字面量）；
/// ② 模板源码里零处数据目录旧住址的字面量（正控：渲染产物里喂一个带它的目录时数得到）；
/// ③ 装那一跳交的就是唯一出口算出来的那个（源码：`plan_install` 的 PowerShell 臂恰一处 `resolve_monitor_data_dir()`）。
#[test]
fn hx2_the_bind_helper_finds_the_monitor_data_dir_through_the_one_exit() {
    let dir = std::path::Path::new("C:\\Users\\o'brien\\iso data");
    {
        let out = render_cc_code(dir);
        let lines: Vec<&str> = out
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("$ccmDir ="))
            .collect();
        assert_eq!(
            lines,
            vec!["$ccmDir = 'C:\\Users\\o''brien\\iso data'"],
            "{out}"
        );
        assert!(!out.contains("{{"), "有占位符没填：{out}");
    }
    let tpl = include_str!("../../../../src/shared/cc.ps1.tpl");
    assert_eq!(
        tpl.matches("claudecode-frontend").count(),
        0,
        "模板里又写死了一份数据目录"
    );
    assert_eq!(tpl.matches("{{MONITOR_DATA_DIR}}").count(), 1);
    let default_dir = std::path::Path::new("/home/u/.claude/claudecode-frontend");
    assert_eq!(
        render_cc_code(default_dir)
            .matches("claudecode-frontend")
            .count(),
        1,
        "正控：喂一个带它的目录时产物里数得到那一个"
    );
    let prod = guard_core::production_code(include_str!(
        "../../../../src/backend/assets/aliases/block.rs"
    ));
    let at = guard_core::find_pinned(&prod, "pub(crate) fn plan_install(").expect("装那一跳不在了");
    let body = &prod[at..at + prod[at..].find("\n}\n").expect("没收尾")];
    assert_eq!(body.matches("monitor_data_dir(home)").count(), 1, "{body}");
}
