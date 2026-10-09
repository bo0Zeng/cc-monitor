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
fn install(p: &Path) -> Result<Value, crate::stream::inbound::spec::Fail> {
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

/// 内容会变的那几个（缺的 ＋ 不一样的），按内嵌清单的顺序。
fn writes(skills: &Path) -> Vec<String> {
    state_at(skills)["writes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}
fn all_files() -> Vec<String> {
    FILES.iter().map(|(r, _)| r.to_string()).collect()
}

/// 确认卡要的那一份：没装 ⇒ 全部要写 · 装好了 ⇒ 一个不写 · 改了 / 少了 ⇒ 正好是那几个；目录在不在另报（在 ⇒ 装时整个改名留作备份）。
#[test]
fn the_state_names_exactly_the_files_that_would_change() {
    let t = tmpdir("state");
    let skills = t.0.join("skills");
    assert_eq!(writes(&skills), all_files());
    assert_eq!(state_at(&skills)["existing"], json!(false));
    install(&t.0).expect("部署");
    assert_eq!(writes(&skills), Vec::<String>::new());
    assert_eq!(state_at(&skills)["existing"], json!(true));
    assert_eq!(
        state_at(&skills)["dest"],
        json!(skills.join("cc-bus").display().to_string())
    );
    let dest = t.0.join("skills/cc-bus");
    std::fs::write(dest.join("SKILL.md"), b"old").unwrap();
    std::fs::remove_file(dest.join("scripts/cc-send")).unwrap();
    assert_eq!(writes(&skills), vec!["SKILL.md", "scripts/cc-send"]);
}

/// 目录在、但一个内嵌文件都没有 ⇒ 全部要写，且目录在（装时先改名留作备份）。
#[test]
fn an_empty_dir_needs_every_file() {
    let t = tmpdir("empty");
    std::fs::create_dir_all(t.0.join("skills/cc-bus")).unwrap();
    assert_eq!(writes(&t.0.join("skills")), all_files());
    assert_eq!(state_at(&t.0.join("skills"))["existing"], json!(true));
}

/// 扩展页拿内嵌那一份的摘要当 cc-bus 的「这一版」：装好之后，资产目录对那个目录算出来的摘要与它相同；改一个字节就不同。
#[test]
fn the_embedded_digest_is_what_the_catalog_sees_after_an_install() {
    let t = tmpdir("digest");
    install(&t.0).expect("部署");
    let dir = t.0.join("skills/cc-bus");
    let seen = crate::assets::asset_catalog::skill_asset(None, NAME, &dir, None).digest;
    assert_eq!(seen, embedded_digest());
    std::fs::write(dir.join("SKILL.md"), b"old").unwrap();
    let seen = crate::assets::asset_catalog::skill_asset(None, NAME, &dir, None).digest;
    assert_ne!(seen, embedded_digest(), "正控：改了一个字节摘要没变");
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
    assert_eq!(first["written"], json!(all_files()));
    assert!(first["backup"].is_null(), "之前没装过，不该有备份");

    let second = install(&t.0).expect("再次部署");
    assert_eq!(second["written"], json!([]), "幂等：内容一致就不该再写");
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
    assert_eq!(r["written"], json!(all_files()), "改名留作备份之后整份重写");
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
    let crate::stream::inbound::spec::Fail { message: err, .. } =
        install(&t.0).expect_err("软链出去必须拒收");
    assert!(
        copy_core::copy_matches("beCcBusInstall.fence.escapes", &err),
        "{err}"
    );
    assert!(
        !outside.0.join("cc-bus").exists(),
        "已经往围栏外写了 —— 那正是这道围栏要挡的"
    );
}

/// ★ 要写的那几个要精确，且「清单外的文件」**故意不算**：我们自己的备份（`cc-bus.bak-<秒>`）就是清单外的，
/// 算了它们那一格就永远说「要写」。代价：清单哪天删掉一个脚本，盘上那份会一直留着而这里说「不用写」（今天清单只增不减）。
#[test]
fn the_writes_are_precise_and_ignore_extra_files() {
    let d = tmpdir("ps2-counts");
    let skills = d.0.join("skills");
    install(&d.0).unwrap();
    let dest = d.0.join("skills/cc-bus");
    assert_eq!(writes(&skills), Vec::<String>::new());
    std::fs::write(dest.join("SKILL.md"), b"tampered").unwrap();
    std::fs::write(dest.join("scripts/cc-spawn"), b"tampered").unwrap();
    assert_eq!(writes(&skills), vec!["SKILL.md", "scripts/cc-spawn"]);
    install(&d.0).unwrap();
    std::fs::write(dest.join("scripts/cc-legacy-thing"), b"old script").unwrap();
    assert_eq!(writes(&skills), Vec::<String>::new(), "清单外的文件不算");
    std::fs::remove_file(dest.join("scripts/cc-kill")).unwrap();
    assert_eq!(writes(&skills), vec!["scripts/cc-kill"]);
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
        writes(&d.0.join("skills")),
        all_files(),
        "落点是文件 ⇒ 全部要写"
    );
    assert_eq!(
        state_at(&d.0.join("skills"))["existing"],
        json!(true),
        "那个文件会先改名留作备份，卡上要说"
    );
    let r = install(&d.0).expect("装");
    assert_eq!(r["written"], json!(all_files()), "该装的一个都没装");
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
    let crate::stream::inbound::spec::Fail { message: e, .. } =
        r.expect_err("`skills/` 不可写时必须报错，不许返回「写了 0 个」的 Ok");
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
