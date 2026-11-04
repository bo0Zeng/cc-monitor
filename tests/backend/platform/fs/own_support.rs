//! 判据用的几种 [`Own`] 造法：本包的判据与上层的测试档（经 `test-support`）共用；发布构建里没有。

use super::{Own, HOME_DIR_NAME};
use std::path::Path;

impl Own {
    /// 判据用：家目录指到一个临时目录（数据目录随之是它底下的 `.cc-monitor`，同没设 `CCM_DATA_DIR` 那一形）。
    pub fn issue_at(home: &Path) -> Own {
        Own {
            home: Some(home.to_path_buf()),
            data: Some(home.join(HOME_DIR_NAME)),
        }
    }

    /// 判据用：只给数据目录（指到一个临时目录；家目录那一根没有）。
    pub fn issue_data_at(data: &Path) -> Own {
        Own {
            home: None,
            data: Some(data.to_path_buf()),
        }
    }

    /// 判据用：两个根各指一处（家目录 · 数据目录，同设了 `CCM_DATA_DIR` 那一形）。
    pub fn issue_roots(home: &Path, data: &Path) -> Own {
        Own {
            home: Some(home.to_path_buf()),
            data: Some(data.to_path_buf()),
        }
    }

    /// 判据用：根指到这个测试进程自己的一个临时目录（不碰真家目录；只是要递一份句柄、并不关心写到哪的那些判据用它）。
    pub fn for_tests() -> Own {
        Own::issue_at(
            &std::env::temp_dir().join(format!("ccm-own-for-tests-{}", std::process::id())),
        )
    }
}

/// 判据的「fd 2」替身要的两样（生产里 fd 2 那一份由 `platform::stderr_fd` 量、指，不经它们）。
impl super::DiagFile {
    /// 此刻多长（问不到 ⇒ `None`）。
    #[expect(
        clippy::len_without_is_empty,
        reason = "判据只量长度涨没涨，「是不是空」没人问"
    )]
    pub fn len(&self) -> Option<u64> {
        self.0.metadata().ok().map(|m| m.len())
    }

    /// 同一份再开一个把手（同一个打开文件描述）。
    pub fn try_clone(&self) -> std::io::Result<super::DiagFile> {
        self.0.try_clone().map(super::DiagFile)
    }
}
