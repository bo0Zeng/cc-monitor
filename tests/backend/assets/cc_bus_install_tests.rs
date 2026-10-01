//! cc-bus 装到这台的判据：从 monitor `cc_bus_deploy_tests.rs` 搬来（判 · 写 · 记进了后端）。
//! 写经本进程的 `files-*`（[`crate::stream::inbound::LocalFiles`]，根由调用方给 ⇒ 临时目录）；装记录的写口换成替身（记下交了什么）。
use super::*;
use crate::stream::inbound::LocalFiles;

struct TmpDir(PathBuf);
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tmpdir(tag: &str) -> TmpDir {
    let p = std::env::temp_dir().join(format!(
        "mig3a-ccbus-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    TmpDir(p)
}
/// 装到 `<p>/skills/cc-bus`（装记录的写口是个不记事的替身）。
fn install(p: &Path) -> Result<Value, (&'static str, String)> {
    install_at(&LocalFiles, &p.join("skills"), &|_| Ok(json!({})))
}

/// ★ 装卸账**复用 skill 装记录那一份**：装完交给记录写口的是 `{op:"add", name:"cc-bus", files:{路径:{digest, created}}}`
/// —— 与 skill「装到这台」同形，每个内嵌文件一行、摘要就是那份原文的摘要；一致那一趟（什么都没写）不记。
#[test]
fn an_install_is_recorded_in_the_skill_install_ledger() {
    let t = tmpdir("ledger");
    let seen = std::sync::Mutex::new(Vec::<Value>::new());
    let rec = |v: &Value| {
        seen.lock().unwrap().push(v.clone());
        Ok(json!({}))
    };
    install_at(&LocalFiles, &t.0.join("skills"), &rec).expect("装");
    let got = seen.lock().unwrap().clone();
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["op"], "add");
    assert_eq!(got[0]["name"], NAME);
    let files = got[0]["files"].as_object().unwrap();
    assert_eq!(files.len(), FILES.len());
    let skill = FILES.iter().find(|(r, _)| *r == "SKILL.md").unwrap();
    assert_eq!(
        files["SKILL.md"]["digest"],
        json!(crate::assets::skill_ledger::digest_of(
            std::str::from_utf8(skill.1).unwrap()
        ))
    );
    install_at(&LocalFiles, &t.0.join("skills"), &rec).expect("再装");
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "一致那一趟什么都没写，不该再记"
    );
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
        state_at(&t.0.join("skills")),
        json!({ "state": "not_installed" })
    );
    // ② 装了、且是这一版
    install(&t.0).expect("部署");
    assert_eq!(
        state_at(&t.0.join("skills")),
        json!({ "state": "up_to_date" })
    );
    // ③ 装了、但不是这一版 —— **带着差了几个**，不是一句「不一致」
    let dest = t.0.join("skills/cc-bus");
    std::fs::write(dest.join("SKILL.md"), b"old").unwrap();
    std::fs::remove_file(dest.join("scripts/cc-send")).unwrap();
    assert_eq!(
        state_at(&t.0.join("skills")),
        json!({ "state": "drifted", "differing": 1, "missing": 1 }),
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
        state_at(&t.0.join("skills")),
        json!({ "state": "not_installed" })
    );
}

/// ★ 幂等：装两次，第二次**一个字节都不写**、也不留备份。
/// ★★ **内嵌清单不许漏掉仓里的脚本**。
///
/// `FILES` 是手写的 `include_bytes!` 清单，而 `src/shared/cc-bus/scripts/` 是源头。
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
    let first = install(&t.0).expect("首次部署");
    assert_eq!(first["written"], json!(FILES.len()));
    assert_eq!(first["unchanged"], json!(0));
    assert!(first["backup"].is_null(), "之前没装过，不该有备份");

    let second = install(&t.0).expect("再次部署");
    assert_eq!(second["written"], json!(0), "幂等：内容一致就不该再写");
    assert_eq!(second["unchanged"], json!(FILES.len()));
    assert!(
        second["backup"].is_null(),
        "一致时**不许**备份 —— 每点一次多一份垃圾备份，那是把「可撤销」变成「攒垃圾」"
    );
}

/// ★ 可撤销：内容变了 ⇒ 覆盖前把旧的整个改名留着。
#[test]
fn an_overwrite_leaves_a_restorable_backup() {
    let t = tmpdir("bak");
    install(&t.0).expect("首次");
    let dest = t.0.join("skills/cc-bus");
    // 弄脏一个文件，模拟「已装的是旧版」。
    std::fs::write(dest.join("SKILL.md"), b"old version").unwrap();

    let r = install(&t.0).expect("覆盖");
    assert!(r["written"].as_u64() > Some(0));
    let bak = r["backup"].as_str().expect("覆盖必须留备份").to_string();
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
    let (_, err) = install(&t.0).expect_err("软链出去必须拒收");
    assert!(err.contains("拒绝"), "{err}");
    assert!(
        !outside.0.join("cc-bus").exists(),
        "已经往围栏外写了 —— 那正是这道围栏要挡的"
    );
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
    install(&d.0).unwrap();
    let dest = d.0.join("skills/cc-bus");
    assert_eq!(
        state_at(&d.0.join("skills")),
        json!({ "state": "up_to_date" })
    );

    std::fs::write(dest.join("SKILL.md"), b"tampered").unwrap();
    std::fs::write(dest.join("scripts/cc-spawn"), b"tampered").unwrap();
    assert_eq!(
        state_at(&d.0.join("skills")),
        json!({ "state": "drifted", "differing": 2, "missing": 0 }),
        "改了两个文件就该报 2 —— 按钮上写的是「更新（差 N 个）」，N 错了跟状态错了一样骗人"
    );

    install(&d.0).unwrap();
    std::fs::write(dest.join("scripts/cc-legacy-thing"), b"old script").unwrap();
    assert_eq!(
        state_at(&d.0.join("skills")),
        json!({ "state": "up_to_date" }),
        "清单外的文件**故意不算** —— 我们自己的 .bak 就是清单外的，算了那颗按钮就永远写着「更新」"
    );

    std::fs::remove_file(dest.join("scripts/cc-kill")).unwrap();
    assert_eq!(
        state_at(&d.0.join("skills")),
        json!({ "state": "drifted", "differing": 0, "missing": 1 }),
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
        state_at(&d.0.join("skills")),
        json!({ "state": "not_installed" }),
        "落点是**文件**时说成「已装/最新」就是骗人 —— 那颗按钮会写着已是最新，而盘上没有 cc-bus"
    );
    let r = install(&d.0).expect("装");
    assert!(r["written"].as_u64() > Some(0), "该装的一个都没装");
    let bak = r["backup"]
        .as_str()
        .expect("覆盖用户那个文件之前**必须**留备份（不可逆动作的底线）")
        .to_string();
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
/// ⚠⚠ **补门的代价：本条从此在 Windows 上 0 次执行**。
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
    let r = install(&d.0);
    // 先恢复权限再断言 —— 否则失败时 `TmpDir::drop` 删不掉，留一地垃圾。
    std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_, e) = r.expect_err("`skills/` 不可写时必须报错，不许返回「写了 0 个」的 Ok");
    assert!(
        e.contains("Permission denied")
            || e.contains("失败")
            || e.contains("拒绝")
            || e.contains("denied"),
        "错误里要留下**原因**，别只说一句「装不了」。实得：{e}"
    );
}

/// ★ 装完就能跑：`scripts/` 下的必须可执行。
///
/// 〔保活 09-24〕`examples/cc-keepalive` 同理 —— 它是给 cron 直接调的，
/// 装出去不可执行，那条 cron 行就会在用户机器上每十分钟静默失败一次。
/// ⚠ 反面也钉：不带 shebang 的示例（`kinds.tsv`）**不许**被顺手标成可执行。
#[cfg(unix)]
#[test]
fn deployed_scripts_are_executable() {
    use std::os::unix::fs::PermissionsExt;
    let t = tmpdir("exec");
    install(&t.0).expect("部署");
    for rel in ["scripts/cc-send", "examples/cc-keepalive"] {
        let p = t.0.join("skills/cc-bus").join(rel);
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "{rel} 装完不可执行等于没装（mode={mode:o}）"
        );
    }
    let kinds = t.0.join("skills/cc-bus/examples/kinds.tsv");
    let mode = std::fs::metadata(&kinds).unwrap().permissions().mode();
    assert!(
        mode & 0o111 == 0,
        "kinds.tsv 不是脚本，却被标成了可执行（mode={mode:o}）"
    );
}
