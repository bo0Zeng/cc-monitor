use super::*;

struct TmpDir(PathBuf);
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tmpdir(tag: &str) -> TmpDir {
    let p = std::env::temp_dir().join(format!(
        "ps1-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    TmpDir(p)
}

/// 内嵌清单必须与仓内那份**逐文件对得上**。
///
/// 少一个 ⇒ 装出去的是**缺件的 skill**，而那比不装更糟：用户以为装好了，
/// 敲某条命令时才发现没有。
#[test]
fn the_embedded_file_list_matches_the_repo() {
    let root = crate::guard_support::repo_root().join("src/shared/cc-bus");
    // ⚠ 走 `guard_core::scan_tree!(&root, &[])` —— **空列表 = 不筛扩展名**，
    //   那个能力是本件 08-13 补进原语的：cc-bus 的脚本（`cc-send` / …）**没有扩展名**，
    //   而原语此前按扩展名筛 ⇒ 想扫这棵树只能自己 `read_dir`，
    //   那又撞 `scanning_guard_registry` 的递减棘轮（「不许把上限调上去让今天好过」）。
    //   ⇒ **缺的是原语的能力，不是纪律的例外。**
    // ⚠ 本条扫的是 `src/shared/cc-bus/`（另一棵树，不含 `.rs`）⇒ 按构造读不到自己。
    let mut on_disk: Vec<String> = guard_core::scan_tree!(&root, &[])
        .into_iter()
        .map(|(p, _)| {
            p.strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    on_disk.sort();
    let mut embedded: Vec<String> = FILES.iter().map(|(r, _)| r.to_string()).collect();
    embedded.sort();
    assert_eq!(
        embedded, on_disk,
        "内嵌清单与 `src/shared/cc-bus/` 对不上 —— 加了文件就要加到 `FILES` 里，\
             否则装出去的是个**缺件的** skill"
    );
}

/// `PS2`：**三态互相分得开**（这正是本件的正题）。
#[test]
fn the_three_install_states_are_distinguishable() {
    let t = tmpdir("state");
    // ① 没装
    assert_eq!(
        install_state_in(&t.0).unwrap(),
        CcBusInstallState::NotInstalled
    );
    // ② 装了、且是这一版
    deploy_into(&t.0).expect("部署");
    assert_eq!(install_state_in(&t.0).unwrap(), CcBusInstallState::UpToDate);
    // ③ 装了、但不是这一版 —— **带着差了几个**，不是一句「不一致」
    let dest = t.0.join("skills/cc-bus");
    std::fs::write(dest.join("SKILL.md"), b"old").unwrap();
    std::fs::remove_file(dest.join("scripts/cc-send")).unwrap();
    assert_eq!(
        install_state_in(&t.0).unwrap(),
        CcBusInstallState::Drifted {
            differing: 1,
            missing: 1
        },
        "「装了旧版」必须带上差异规模 —— 只说「不一致」用户不知道该不该在意"
    );
}

/// `PS2`：目录在、但一个内嵌文件都没有 ⇒ 那是**没装**，不是「装了个旧版」。
///
/// ⚠ 这一格是分界：把它判成 `Drifted` 会让界面说「有更新」，
/// 而用户点下去发现是第一次装 —— 两件事的心理预期完全不同。
#[test]
fn an_empty_dir_counts_as_not_installed() {
    let t = tmpdir("empty");
    std::fs::create_dir_all(t.0.join("skills/cc-bus")).unwrap();
    assert_eq!(
        install_state_in(&t.0).unwrap(),
        CcBusInstallState::NotInstalled
    );
}

/// ★ 幂等：装两次，第二次**一个字节都不写**、也不留备份。
/// ★★ **内嵌清单不许漏掉仓里的脚本**〔08-13〕。
///
/// `FILES` 是手写的 `include_bytes!` 清单，而 `src/shared/cc-bus/scripts/` 是真相源。
/// 08-13 往那个目录**新建过一个脚本**（`cc-spawned-record`）—— 漏进清单的后果是：
/// 编译照过、测试照绿，而**装出去的 cc-bus 少一个文件**，
/// 在用户机器上表现成「新版 cc-spawn 调一个不存在的命令」。
///
/// ⚠ 只钉 `scripts/`：`examples/` 与 `SKILL.md` 是另一族（那边多一个示例文件
/// 不影响能不能跑），要钉得单独论证。**判据的人群等于它真正证明的那件事。**
#[test]
fn every_script_in_the_repo_is_embedded_for_deployment() {
    let dir = crate::guard_support::repo_root().join("src/shared/cc-bus/scripts");
    let mut on_disk: Vec<String> = guard_core::shell_scripts(&crate::guard_support::repo_root())
        .into_iter()
        .filter(|p| p.replace('\\', "/").contains("src/shared/cc-bus/scripts/"))
        .filter_map(|p| {
            std::path::Path::new(&p)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect();
    on_disk.sort();
    assert!(
        on_disk.len() >= 10,
        "只列到 {} 个脚本 —— 取法坏了，本断言在空转：{on_disk:?}",
        on_disk.len()
    );
    let embedded: Vec<String> = FILES
        .iter()
        .filter(|(rel, _)| rel.starts_with("scripts/"))
        .map(|(rel, _)| rel.trim_start_matches("scripts/").to_string())
        .collect();
    let missing: Vec<&String> = on_disk.iter().filter(|f| !embedded.contains(f)).collect();
    assert!(
        missing.is_empty(),
        "这些脚本在 `{}` 里，却没进内嵌清单：{missing:?}\n\
             ⇒ 装出去的 cc-bus 会**少这几个文件**，而编译与测试都不会红。\n\
             在 `FILES` 里加一行 `include_bytes!`。",
        dir.display()
    );
    // 反向：清单里不许有仓里已经没有的（幽灵条目会让人以为还装着它）
    let ghosts: Vec<&String> = embedded.iter().filter(|f| !on_disk.contains(f)).collect();
    assert!(ghosts.is_empty(), "内嵌清单里有仓里没有的脚本：{ghosts:?}");
}

#[test]
fn deploying_twice_writes_nothing_the_second_time() {
    let t = tmpdir("idem");
    let first = deploy_into(&t.0).expect("首次部署");
    assert_eq!(first.written as usize, FILES.len());
    assert_eq!(first.unchanged, 0);
    assert_eq!(first.backup, None, "之前没装过，不该有备份");

    let second = deploy_into(&t.0).expect("再次部署");
    assert_eq!(second.written, 0, "幂等：内容一致就不该再写");
    assert_eq!(second.unchanged as usize, FILES.len());
    assert_eq!(
        second.backup, None,
        "一致时**不许**备份 —— 每点一次多一份垃圾备份，那是把「可撤销」变成「攒垃圾」"
    );
}

/// ★ 可撤销：内容变了 ⇒ 覆盖前把旧的整个改名留着。
#[test]
fn an_overwrite_leaves_a_restorable_backup() {
    let t = tmpdir("bak");
    deploy_into(&t.0).expect("首次");
    let dest = t.0.join("skills/cc-bus");
    // 弄脏一个文件，模拟「已装的是旧版」。
    std::fs::write(dest.join("SKILL.md"), b"old version").unwrap();

    let r = deploy_into(&t.0).expect("覆盖");
    assert!(r.written > 0);
    let bak = r.backup.expect("覆盖必须留备份");
    let bak = Path::new(&bak);
    assert!(bak.is_dir(), "备份目录不在：{}", bak.display());
    assert_eq!(
        std::fs::read(bak.join("SKILL.md")).unwrap(),
        b"old version",
        "备份里必须是**被覆盖前**那份，否则撤不回去"
    );
}

/// ★ 围栏：`skills` 是个指向别处的软链 ⇒ **拒收**。
#[cfg(unix)]
#[test]
fn a_symlinked_skills_dir_is_refused() {
    let t = tmpdir("fence");
    let outside = tmpdir("outside");
    std::os::unix::fs::symlink(&outside.0, t.0.join("skills")).unwrap();
    let err = deploy_into(&t.0).expect_err("软链出去必须拒收");
    assert!(err.contains("拒绝"), "{err}");
    assert!(
        !outside.0.join("cc-bus").exists(),
        "已经往围栏外写了 —— 那正是这道围栏要挡的"
    );
}

/// ★★ 装前那道能力预检**列的东西必须与 `cc-spawn` 真正协商的一致**〔08-13〕。
///
/// 两边是**同一个事实的两份表达**（Rust 的 `CC_SPAWN_NEEDS` 与 shell 里那行 `for _c in …`）。
/// 抽不成一份（一个是编译进 monitor 的常量、一个是要部署出去的 shell）⇒ 只能钉一致。
/// ⚠ 不一致的后果是**预检说没事、装完就坏**：漏列一条，缺那条能力的机器上
/// 装完 `cc-spawn` 直接 `exit 2`，而部署那步一声不吭地成功了。
#[test]
fn the_deploy_precheck_lists_what_cc_spawn_negotiates() {
    let spawn = include_str!("../../src/shared/cc-bus/scripts/cc-spawn");
    let line = spawn
        .lines()
        .find(|l| l.trim_start().starts_with("for _c in "))
        .expect("`cc-spawn` 里那行能力协商不见了 —— 它是这条判据的另一半");
    // 形如：`for _c in detach tmux-size tmux-base bus-register; do`
    let listed: Vec<&str> = line
        .trim()
        .trim_start_matches("for _c in ")
        .split(';')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    assert!(
        !listed.is_empty(),
        "解析出空清单 —— 抽取器坏了（本条此刻是空转的）"
    );
    assert_eq!(
        listed, CC_SPAWN_NEEDS,
        "装前预检的清单与 `cc-spawn` 真正协商的对不上。\n             \
             左=cc-spawn 实际要的，右=Rust 侧 `CC_SPAWN_NEEDS`。\n             \
             漏列一条 ⇒ 预检说没事、装完 `cc-spawn` 直接 exit 2，而部署那步一声不吭地成功了。"
    );
}

/// ★★ **Windows 上「这一格没做预检」必须说出口，不许静默**〔win-compile 09-09〕。
///
/// `local_ccm_too_old_warning` 的 `None` 有一个**确定**含义：**探过了、够新**
/// （调用方据此不给用户任何提示）。而 Windows 上根本探不了（探测那一族整族
/// 带 `#[cfg(not(windows))]`，它跑的是 `bash -lic`）⇒ 返回 `None` 就是把
/// 「查不了」读成「没问题」，正是 `sftp.rs` 那个四值枚举 `TargetBinary` 治的病。
///
/// ⚠ **它跑在本仓唯一跑 `cargo test` 的平台上**（云端 windows-latest）——
///   与同一批补门的那四条判据恰好相反：那四条从此在那台机器上一次都不跑。
/// ⚠ 射程：只买「话说没说、说没说全」。**买不到**「用户真在界面上看见了」——
///   那一跳在 `settings/cc-bus-section.ts`，由前端那侧的判据管。
#[cfg(windows)]
#[test]
fn windows_says_the_precheck_did_not_happen_instead_of_staying_silent() {
    let w = local_ccm_too_old_warning().unwrap_or_default();
    assert!(
        !w.is_empty(),
        "Windows 上回了 `None` —— 那是把「查不了」读成「没问题」：\
             用户读到的是一个纯成功，而没有任何人告诉他这一格根本没检查"
    );
    assert!(
        w.contains("没做预检"),
        "这句话没把「**没做**」说出来 ⇒ 用户分不开它与「查过了、有问题」：{w}"
    );
    // 清单从常量现取（不许在那句话里抄一份字面量）⇒ 四条能力必须真的出现在话里。
    for c in CC_SPAWN_NEEDS.iter().copied() {
        assert!(w.contains(c), "警告里没提能力 {c:?}：{w}");
    }
}

/// ★ 三态的**计数**要精确，且「清单外的文件」**故意不算**〔08-13 复核〕。
///
/// # 四态逐个钉
///
/// | 盘上的样子 | 该说 |
/// |---|---|
/// | 刚装完 | `UpToDate` |
/// | 改了 2 个文件 | `Drifted{differing:2, missing:0}` |
/// | **多一个清单外的文件** | `UpToDate` —— 见下方「为什么故意不算」 |
/// | 删了 1 个 | `Drifted{differing:0, missing:1}` |
///
/// ⚠ 钉的是**数字**不是「是不是 Drifted」：按钮上写的是「更新（差 N 个文件）」，
/// N 错了跟状态错了一样骗人。
///
/// # 为什么「多出来的文件」故意不算
///
/// **我们自己的备份就是清单外的文件**（`cc-bus.bak-<ts>` / 08-13 实测用户那份里还有
/// 四份 07-18 留下的 `scripts.bak-*`）。把它们算成漂移 ⇒ 那颗按钮**永远**写着「更新」，
/// 而点了也不会变 —— 比不报还坏。
///
/// ⚠ **代价如实写**：哪天有脚本从清单里**删掉**，盘上那份会一直留着而状态仍说
/// `UpToDate`。这属于 `P2t` 记过的「旧文件永不回收」那一族，它的处置逐字是
/// 「**真但未发生**」——今天清单只增不减，零实例 ⇒ 不为它造回收机制
/// （造了也验不出修对没修对）。**这条注释就是那个前提的住址**：清单第一次删东西时，
/// 来这儿把它改掉。
#[test]
fn the_three_states_count_precisely_and_ignore_extra_files() {
    let d = tmpdir("ps2-counts");
    deploy_into(&d.0).unwrap();
    let dest = d.0.join("skills/cc-bus");
    assert_eq!(install_state_in(&d.0).unwrap(), CcBusInstallState::UpToDate);

    std::fs::write(dest.join("SKILL.md"), b"tampered").unwrap();
    std::fs::write(dest.join("scripts/cc-spawn"), b"tampered").unwrap();
    assert_eq!(
        install_state_in(&d.0).unwrap(),
        CcBusInstallState::Drifted {
            differing: 2,
            missing: 0
        },
        "改了两个文件就该报 2 —— 按钮上写的是「更新（差 N 个）」，N 错了跟状态错了一样骗人"
    );

    deploy_into(&d.0).unwrap();
    std::fs::write(dest.join("scripts/cc-legacy-thing"), b"old script").unwrap();
    assert_eq!(
        install_state_in(&d.0).unwrap(),
        CcBusInstallState::UpToDate,
        "清单外的文件**故意不算** —— 我们自己的 .bak 就是清单外的，算了那颗按钮就永远写着「更新」"
    );

    std::fs::remove_file(dest.join("scripts/cc-kill")).unwrap();
    assert_eq!(
        install_state_in(&d.0).unwrap(),
        CcBusInstallState::Drifted {
            differing: 0,
            missing: 1
        },
        "少一个就该报 missing:1，且不该把它算进 differing"
    );
}

/// ★ 落点被一个**普通文件**占着（用户手滑 / 旧版留下的残骸）〔08-13 复核〕。
///
/// 两条性质一起钉，因为它们**互相制约**：
/// · 状态必须说 `NotInstalled` —— 说 `UpToDate` 就是骗人，那颗按钮会写着「已是最新」
///   而盘上根本没有 cc-bus（`PS2` 头注逐字：把「装了旧版」说成「已装」⇒ 没人会去点）；
/// · 部署必须**先备份再替换** —— 直接删掉用户那个文件是不可逆的。
#[test]
fn a_regular_file_at_the_destination_is_not_installed_and_gets_backed_up() {
    let d = tmpdir("file-at-dest");
    std::fs::create_dir_all(d.0.join("skills")).unwrap();
    std::fs::write(d.0.join("skills/cc-bus"), b"i am a file, not a dir").unwrap();
    assert_eq!(
        install_state_in(&d.0).expect("查状态"),
        CcBusInstallState::NotInstalled,
        "落点是**文件**时说成「已装/最新」就是骗人 —— 那颗按钮会写着已是最新，而盘上没有 cc-bus"
    );
    let r = deploy_into(&d.0).expect("装");
    assert!(r.written > 0, "该装的一个都没装");
    let bak = r
        .backup
        .expect("覆盖用户那个文件之前**必须**留备份（不可逆动作的底线）");
    assert_eq!(
        std::fs::read_to_string(&bak).expect("备份读得回来"),
        "i am a file, not a dir",
        "备份里不是原来那个文件的内容 —— 那等于没备份"
    );
}

/// ★ `skills/` **不可写**（只读目录 / 磁盘满的可控替身）〔08-13 复核〕。
///
/// ⚠ 必须是 `Err` 而不是「装了 0 个文件的 Ok」：后者会让 UI 报「已装到 …（写了 0 个）」，
/// 又一次「假成功比失败更坏」。
///
/// ⚠⚠ **补门的代价：本条从此在 Windows 上 0 次执行**〔win-compile 09-09〕。
/// 它靠 `std::os::unix::fs::PermissionsExt` 造「只读目录」这个可控替身，而先前
/// **漏了门** ⇒ 云端（windows-latest，本仓**唯一**跑 `cargo test` 的平台）上
/// 编译失败（E0433 ×1 + E0599 ×2）。门照本文件既有口径写成 `#[cfg(unix)]`
/// （同 `a_symlinked_skills_dir_is_refused` / `deployed_scripts_are_executable`
/// 那两条 —— 它们用的是同一个平台原语）。
/// ⚠ 于是「`skills/` 不可写时必须 `Err`、不许返回写了 0 个的 `Ok`」这条性质，
///   在 Windows 上**没有任何东西守着**；替身要换成 Windows 的 ACL 才买得回来。
#[cfg(unix)]
#[test]
fn an_unwritable_skills_dir_fails_loudly() {
    use std::os::unix::fs::PermissionsExt;
    let d = tmpdir("ro-skills");
    let skills = d.0.join("skills");
    std::fs::create_dir_all(&skills).unwrap();
    std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o555)).unwrap();
    let r = deploy_into(&d.0);
    // 先恢复权限再断言 —— 否则失败时 `TmpDir::drop` 删不掉，留一地垃圾。
    std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o755)).unwrap();
    let e = r.expect_err("`skills/` 不可写时必须报错，不许返回「写了 0 个」的 Ok");
    assert!(
        e.contains("Permission denied") || e.contains("失败"),
        "错误里要留下**原因**，别只说一句「装不了」。实得：{e}"
    );
}

/// ★ 装完就能跑：`scripts/` 下的必须可执行。
#[cfg(unix)]
#[test]
fn deployed_scripts_are_executable() {
    use std::os::unix::fs::PermissionsExt;
    let t = tmpdir("exec");
    deploy_into(&t.0).expect("部署");
    let p = t.0.join("skills/cc-bus/scripts/cc-send");
    let mode = std::fs::metadata(&p).unwrap().permissions().mode();
    assert!(mode & 0o111 != 0, "装完不可执行等于没装（mode={mode:o}）");
}
