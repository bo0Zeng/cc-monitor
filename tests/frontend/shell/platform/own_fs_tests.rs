//! `platform::own_fs`：自己的状态落在数据目录（默认 `~/.cc-monitor/`）底下、只给本人；部署物先写 `.partial` 再上位；
//! 相对名越不出根。家是测试档的沙箱（`test-home`），每条判据在它底下另建一层。

use super::*;
use std::path::PathBuf;

/// 这一条判据自己的家：沙箱临时目录底下按名字建一层，用完删。
fn fake_home(tag: &str) -> PathBuf {
    let h = std::env::temp_dir().join(format!("own-fs-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&h).ok();
    std::fs::create_dir_all(&h).expect("建判据自己的家");
    h
}

/// 照真规矩从家推出数据目录（没设 `CCM_DATA_DIR` ⇒ `<家>/.cc-monitor`）。
fn state_in(home: &Path) -> OwnState {
    OwnState::at(
        crate::config::monitor_data_dir_from(None, Some(home.to_path_buf())).expect("家是绝对路径"),
    )
}

#[test]
fn own_state_lands_under_the_data_dir_and_only_for_me() {
    let home = fake_home("state");
    let own = state_in(&home);
    own.write("sub/state.json", b"{}").expect("写自己的状态");
    let at = home.join(".cc-monitor").join("sub").join("state.json");
    assert_eq!(std::fs::read(&at).expect("落在数据目录底下"), b"{}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&at).expect("元数据").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "自己的状态只给本人");
        let dir = std::fs::metadata(at.parent().expect("上一层"))
            .expect("元数据")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir, 0o700, "缺的那几层建出来就只给本人");
    }
    // 覆盖写：旁名不留。
    own.write("sub/state.json", b"[1]").expect("再写一次");
    assert_eq!(std::fs::read(&at).expect("读回"), b"[1]");
    let left: Vec<_> = std::fs::read_dir(at.parent().expect("上一层"))
        .expect("列")
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();
    assert_eq!(left.len(), 1, "原子写的旁名换名后不留：{left:?}");
    own.remove("sub/state.json").expect("删");
    assert!(!at.exists());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_name_that_climbs_out_of_the_root_is_refused() {
    let home = fake_home("climb");
    let own = state_in(&home);
    for bad in ["../escape.json", "/etc/escape.json", "", "a/../../b"] {
        assert!(
            own.write(bad, b"x").is_err(),
            "{bad:?} 不是根底下的相对名，该拒"
        );
    }
    assert!(!home.join("escape.json").exists());
    let bin = OwnBin::at(home.join(".cc-monitor").join("bin"));
    for bad in ["../ccm", "sub/ccm", ""] {
        assert!(
            bin.stage(bad, b"x").is_err(),
            "{bad:?} 不是落点目录里的一个名字，该拒"
        );
    }
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_deployment_is_staged_as_partial_then_put_in_place() {
    let home = fake_home("bin");
    let bin = OwnBin::at(home.join(".cc-monitor").join("bin"));
    let staged = bin.stage("ccm", b"v1").expect("暂存");
    let name = staged
        .file_name()
        .and_then(|n| n.to_str())
        .expect("名字")
        .to_string();
    assert!(
        name.starts_with(".ccm.") && name.ends_with(".partial"),
        "暂存件叫 .<名>.<pid>.partial：{name}"
    );
    assert!(!bin.dir().join("ccm").exists(), "上位之前落点不在");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&staged)
            .expect("元数据")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "暂存件已置可执行位、只给本人");
    }
    let dest = bin.put("ccm", &staged).expect("上位");
    assert_eq!(dest, bin.dir().join("ccm"));
    assert_eq!(std::fs::read(&dest).expect("读回"), b"v1");
    assert!(!staged.exists(), "上位之后暂存件不在");
    // 再放一版：旧的被换掉。
    let dest = bin.place("ccm", b"v2").expect("再放一版");
    assert_eq!(std::fs::read(&dest).expect("读回"), b"v2");
    bin.remove("ccm").expect("删落点那一份");
    assert!(!dest.exists());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_log_dir_is_created_where_the_caller_says() {
    let home = fake_home("logs");
    let at = home.join("logs").join("backend");
    log_dir(&at).expect("建日志目录");
    assert!(at.is_dir());
    log_dir(&at).expect("已在的不动");
    std::fs::remove_dir_all(&home).ok();
}
