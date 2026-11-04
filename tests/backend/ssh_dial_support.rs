//! 拨号核心（`src/comms/ssh-dial/`）判据共用的测试档那一套。整个模块只在 `cfg(test)` 下存在。
//! 源码人群的根是**后端那个包**（`guard_core::addr::backend_src_root`）：本 crate 在它清单写的人群里（`population`），从那里扫就连本 crate 一起扫到。

/// 测试档那一套建目录给的权限位（部署那种规矩；与后端交的 `own_dir::PRIVATE_DIR_MODE` 同值，后端那边另有一条核它真交了那一个）。
pub(crate) const TEST_DIR_MODE: u32 = 0o700;

/// 装上测试档那一套：同 [`crate::env::Env::bare`]，只是远端写按部署那两处（判据要写 `staging` / `bin`）、
/// 钥匙那一格先查判据登记的几样（[`register`]：密码 · 外部签名器），不在表里才按路径读文件（读成文本交 [`crate::env::key_from_pem`]）、
/// 主机钥匙库是进程里一份文本（[`remembered`] · [`seed_library`]）。一个进程一次。
pub(crate) fn test_host() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let env = crate::env::Env {
            keys: Box::new(TestKeys),
            host_keys: Box::new(TestHostKeys),
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
            // 不在表里 ⇒ 按路径读那份钥匙文件（宿主那一侧的事，测试档替它读），再交拨号核心解。
            None => match std::fs::read_to_string(key) {
                Ok(pem) => crate::env::key_from_pem(key, &pem, None),
                Err(e) => Err(crate::Said::with_raw(crate::env::key_unreadable(key), &e)),
            },
        }
    }
}

/// 没装宿主那一套时（[`crate::env::Env::bare`]）钥匙那一格不碰盘：盘上真有一把 OpenSSH 私钥、路径照写，也回读不出那一句。
#[test]
fn the_bare_env_never_reads_a_key_file() {
    let dir = std::env::temp_dir().join(format!("ccm-dial-bare-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let at = dir.join("id_t");
    std::fs::write(&at, test_pem().as_bytes()).unwrap();
    let shown = at.to_string_lossy().into_owned();
    let said = match crate::env::Env::bare().keys.credential(&shown) {
        Err(s) => s.said,
        Ok(_) => panic!("没装宿主那一套也读盘拿到了钥匙"),
    };
    assert_eq!(
        said,
        copy_core::copy_text("beConnect.auth.keyLoad", &[("path", &shown)])
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 宿主读好的 PEM 文本由拨号核心那一处解：同一份文本解得出钥匙；坏文本回读不出那一句（路径照交来的那一形）、解码原话进 `raw`。
#[test]
fn a_pem_handed_in_by_the_host_decodes_in_the_core() {
    let pem = test_pem();
    assert!(matches!(
        crate::env::key_from_pem("~/.ssh/id_t", &pem, None),
        Ok(crate::env::Credential::Key(_))
    ));
    let bad = match crate::env::key_from_pem("~/.ssh/id_t", "not a key", None) {
        Err(s) => s,
        Ok(_) => panic!("坏文本解成了钥匙"),
    };
    assert_eq!(bad.said, crate::env::key_unreadable("~/.ssh/id_t"));
    assert!(bad.raw.is_some(), "解码原话没带上");
}

/// 一把 OpenSSH 格式的私钥文本（固定种子，判据现造）。
fn test_pem() -> String {
    russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(
        &[7; 32],
    ))
    .to_openssh(russh::keys::ssh_key::LineEnding::LF)
    .unwrap()
    .to_string()
}

/// 测试档那份主机钥匙库（整个进程一份文本；各判据的服务端各占一个口，按口认自己那几行）。
fn library() -> &'static std::sync::Mutex<String> {
    static L: std::sync::OnceLock<std::sync::Mutex<String>> = std::sync::OnceLock::new();
    L.get_or_init(Default::default)
}

/// 库里主机名段含 `[127.0.0.1]:port` 的那几行（`主机名段 算法 base64`）。
pub(crate) fn remembered(port: u16) -> Vec<String> {
    let me = crate::known_hosts::host_pattern("127.0.0.1", port);
    library()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .lines()
        .filter(|l| {
            l.split_whitespace()
                .next()
                .is_some_and(|n| n.split(',').any(|x| x == me))
        })
        .map(str::to_string)
        .collect()
}

/// 判据往库里预先写一行（`[127.0.0.1]:port 算法 base64`，或待核的那一形）。
pub(crate) fn seed_library(line: &str) {
    test_host();
    let mut l = library().lock().unwrap_or_else(|e| e.into_inner());
    l.push_str(line);
    l.push('\n');
}

struct TestHostKeys;

impl crate::env::HostKeys for TestHostKeys {
    fn read(&self) -> Result<String, crate::Said> {
        Ok(library().lock().unwrap_or_else(|e| e.into_inner()).clone())
    }
    fn rewrite(&self, edit: &mut dyn FnMut(&str) -> Option<String>) -> Result<(), crate::Said> {
        let mut l = library().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(next) = edit(&l) {
            *l = next;
        }
        Ok(())
    }
}
