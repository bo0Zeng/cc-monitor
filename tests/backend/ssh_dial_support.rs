//! 拨号核心（`src/backend/ssh-dial/`）判据共用的测试档那一套。整个模块只在 `cfg(test)` 下存在。
//! 源码人群的根是**后端那个包**（`guard_core::addr::backend_src_root`）：本 crate 住它底下，从那里扫就连本 crate 一起扫到。

/// 测试档那一套建目录给的权限位（部署那种规矩；与后端交的 `own_dir::PRIVATE_DIR_MODE` 同值，后端那边另有一条核它真交了那一个）。
pub(crate) const TEST_DIR_MODE: u32 = 0o700;

/// 装上测试档那一套：同 [`crate::env::Env::bare`]，只是远端写按部署那两处（判据要写 `staging` / `bin`）、
/// 钥匙那一格先查判据登记的几样（[`register`]：密码 · 外部签名器），不在表里才按路径读文件。一个进程一次。
pub(crate) fn test_host() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let env = crate::env::Env {
            keys: Box::new(TestKeys),
            files: crate::sftp::WriteRules::Deploy {
                dir_mode: TEST_DIR_MODE,
            },
            ..crate::env::Env::bare()
        };
        assert!(crate::env::install(env).is_ok(), "测试档那一套装过了");
    });
}

/// 判据登记的一样钥匙（`key_path` 写它的编号）。
#[derive(Clone)]
pub(crate) enum TestCred {
    Password(String),
    Signer(std::sync::Arc<dyn crate::env::ExternalSigner>),
}

fn creds() -> &'static std::sync::Mutex<std::collections::BTreeMap<String, TestCred>> {
    static C: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, TestCred>>> =
        std::sync::OnceLock::new();
    C.get_or_init(Default::default)
}

/// 登记一样钥匙：拨号请求里 `key_path = id` 时交它（同手机那一形：编号 ⇒ 库里那一样）。
pub(crate) fn register(id: &str, cred: TestCred) {
    test_host();
    creds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.to_string(), cred);
}

struct TestKeys;

impl crate::env::KeySource for TestKeys {
    fn credential(&self, key: &str) -> Result<crate::env::Credential, crate::Said> {
        let found = creds()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned();
        match found {
            Some(TestCred::Password(p)) => Ok(crate::env::Credential::Password(p)),
            Some(TestCred::Signer(s)) => Ok(crate::env::Credential::Signer(s)),
            None => crate::env::KeyFiles.credential(key),
        }
    }
}
