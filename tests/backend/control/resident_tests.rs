//! 〔HOST · V139〕远端常驻后端的起 · 找 · 停（`control/resident.rs`）。
//! 守的要求：`99 §1` V139「远端常驻、本机远端同形」；`INVARIANTS §48.1`「监听口要钥匙」（钥匙不进 env / argv）。

use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-resident-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// H5：钥匙出生即只给本人、第二次读回同一把；子进程的环境里只有钥匙文件的**路径**，没有钥匙本身。
#[test]
fn the_token_is_private_stable_and_never_handed_through_the_environment() {
    let home = scratch("tok");
    let path = home.join(relay_route_core::LISTEN_TOKEN_FILE_REL);
    let t1 = ensure_token(&path).expect("铸不出钥匙");
    assert_eq!(t1.len(), 2 * TOKEN_BYTES);
    assert_eq!(
        ensure_token(&path).unwrap(),
        t1,
        "第二次没读回同一把 —— 已在跑的常驻后端会接不上"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "钥匙文件不是只给本人");
    }
    let env = child_env(
        49999,
        &path,
        &home,
        &[
            ("CCM_RELAY_PORT", "8788".into()),
            ("CCM_BACKEND_STDERR_LOG", format!("~/{STDERR_LOG_REL}")),
        ],
    );
    assert!(
        env.iter().all(|(_, v)| !v.contains(&t1)),
        "钥匙进了子进程的环境：{env:?}"
    );
    let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        names,
        [
            crate::listen::ENV_PORT,
            crate::listen::ENV_TOKEN_FILE,
            "CCM_RELAY_PORT",
            "CCM_BACKEND_STDERR_LOG",
        ],
        "子进程环境那几格对不上（V139：中转口要交给它）"
    );
    assert_eq!(
        env[3].1,
        home.join(STDERR_LOG_REL).display().to_string(),
        "`~/` 没换成家目录"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 停之前核身份：部署换过文件之后旧进程的 exe 带 ` (deleted)` 照样认；别的程序不认。
#[test]
fn the_owner_record_parses_and_the_identity_check_tolerates_a_replaced_file() {
    assert_eq!(
        parse_owner("123\n/h/.cc-monitor/bin/cc-monitor-backend\n"),
        Some((123, PathBuf::from("/h/.cc-monitor/bin/cc-monitor-backend")))
    );
    assert_eq!(parse_owner("0\n/x\n"), None);
    assert_eq!(parse_owner("123\n"), None);
    let rec = Path::new("/h/.cc-monitor/bin/cc-monitor-backend");
    assert!(exe_matches("/h/.cc-monitor/bin/cc-monitor-backend", rec));
    assert!(exe_matches(
        "/h/.cc-monitor/bin/cc-monitor-backend (deleted)",
        rec
    ));
    assert!(!exe_matches("/usr/bin/python3", rec));
}

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_string())
    }
}

/// 〔HOST〕远端 `--resident-ensure` 那一形：钥匙在文件里（env 只有路径）⇒ 读出来就是常驻；文件空 / 读不动 ⇒ 拒（fail closed）。
#[test]
fn a_token_file_gives_listen_mode_and_an_empty_one_is_refused() {
    let dir = std::env::temp_dir().join(format!("ccm-listen-tokfile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("listen-token");
    std::fs::write(&f, "abc123\n").unwrap();
    let path = f.to_string_lossy().into_owned();
    let m = crate::listen::mode_from(&env_of(&[
        (crate::listen::ENV_PORT, "51000"),
        (crate::listen::ENV_TOKEN_FILE, &path),
    ]))
    .unwrap();
    assert_eq!(
        crate::listen::resolve(m).unwrap(),
        Some((51000, "abc123".to_string()))
    );
    std::fs::write(&f, " \n").unwrap();
    let m = crate::listen::mode_from(&env_of(&[
        (crate::listen::ENV_PORT, "51000"),
        (crate::listen::ENV_TOKEN_FILE, &path),
    ]))
    .unwrap();
    assert!(crate::listen::resolve(m).is_err(), "空钥匙文件也起了一个口");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 〔TAIL · HOST 余项〕远端起的常驻后端也带上数据目录那两格（与那台自己的 monitor 交的同一对值）⇒ 那台 monitor 收养它、HX2 不拒。
/// 守的要求：主会话裁 HOST 待拍 1「后端自己按默认推出这两格路径（谁起都一样）」（`_施工/paused-state.md`）。期望路径手写。
#[test]
fn the_resident_derives_the_same_data_dir_pair_whoever_starts_it() {
    let names = ("CCM_APIKEY_CREDENTIALS", "CCM_HISTORY_METADATA");
    let got = data_dir_envs(&env_of(&[]), Path::new("/h"), names.0, names.1);
    assert_eq!(
        got,
        vec![
            (
                names.0,
                "/h/.claude/claudecode-frontend/apikey-credentials.json".to_string()
            ),
            (
                names.1,
                "/h/.claude/claudecode-frontend/history-metadata.json".to_string()
            ),
        ]
    );
    // `CCM_DATA_DIR` 跟着走；本进程已有的那一格不覆盖；相对路径 ⇒ 两格都缺席（不退回真 profile）。
    let iso = data_dir_envs(
        &env_of(&[("CCM_DATA_DIR", "/iso"), (names.0, "/x/c.json")]),
        Path::new("/h"),
        names.0,
        names.1,
    );
    assert_eq!(
        iso,
        vec![(names.1, "/iso/history-metadata.json".to_string())]
    );
    assert!(data_dir_envs(
        &env_of(&[("CCM_DATA_DIR", "rel")]),
        Path::new("/h"),
        names.0,
        names.1
    )
    .is_empty());
}
