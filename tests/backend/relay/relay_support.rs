//! [`crate::relay::test_support`] 的体 —— 判据专用的那几个口（本包的测试与上层的测试共用，只用本包 `[dependencies]` 里有的 crate）：
//! 判据用的那一把钥匙（与它派生出来的只许直通那一把）· 夹具家目录里预放钥匙 · 只记不读的 tee 落点 · 起监听那几样（绑口 · 接受循环 · 期限与上界）。

use crate::relay::key::{door_check, key_from_text, pass_of};
use comms_outward::{Keys, TapEvent, TapPort};
use creds_core::SecretKey;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use crate::relay::listen::{
    listen, serve, DOWNSTREAM_DEADLINE, INFLIGHT_BODY_CAP, INFLIGHT_CONNECTIONS, UPSTREAM_DEADLINE,
};

/// 判据用的那一把（形状合法、一眼认得出是测试值）。
pub const TEST_KEY: &str = "7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57";

/// 判据用：[`TEST_KEY`] 那一把。
pub fn test_key() -> SecretKey {
    key_from_text(TEST_KEY).expect("夹具钥匙过得了形状闸")
}

/// 判据用的那把只许直通的钥匙：从 [`TEST_KEY`] 派生（生产同一个派生）。
pub fn test_pass_key() -> String {
    pass_of(&test_key()).expose_for_relay_key().to_string()
}

/// 判据用：[`TEST_KEY`] ＋ 它派生出来的只许直通那一把（中转门上那两把，与生产同一个派生、同一个比对口）。
pub fn test_keys() -> Keys {
    Keys {
        full: door_check(test_key()),
        pass: door_check(pass_of(&test_key())),
    }
}

/// 判据用：在一个**夹具家目录**里预先放好 [`TEST_KEY`]（`0600`），回这个家目录。
/// 起真中转的判据都要它 —— 不给的话中转会去**用户真实的家目录**里铸钥匙。
pub fn seed_test_home(home: &Path) -> PathBuf {
    let path = home.join(relay_route_core::KEY_FILE_REL);
    // 夹具直接落盘（只给本人）；那一层目录这里自己建。
    std::fs::create_dir_all(path.parent().expect("钥匙路径有父目录")).expect("建夹具目录");
    std::fs::write(&path, TEST_KEY).expect("写夹具钥匙");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("只给本人");
    }
    home.to_path_buf()
}

/// tap 口收到的每一件（只记下来，判据要看就读 `.0`）。
#[derive(Default)]
pub struct Taps(pub Mutex<Vec<TapEvent>>);
impl TapPort for Taps {
    fn offer(&self, ev: TapEvent) -> bool {
        self.0.lock().expect("lock").push(ev);
        true
    }
}
