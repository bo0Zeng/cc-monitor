//! **主机钥匙那份库在手机上**：App 私有目录下一份 OpenSSH known_hosts（路径由 Kotlin 装核心时交）。
//! 读 · 记 · 忘的规矩在拨号核心 `ssh_dial::known_hosts`（后端同一份）；这里只有这份文件的读写（同一个进程只有核心写它，锁是进程内的一把）。

use std::path::PathBuf;
use std::sync::Mutex;

use ssh_dial::env::HostKeys;
use ssh_dial::{known_hosts, Endpoint, Said};

pub struct PhoneHostKeys {
    path: PathBuf,
    /// 改写那一段的锁（读 → 改 → 写回整份）。
    lock: Mutex<()>,
}

impl PhoneHostKeys {
    pub fn new(path: PathBuf) -> PhoneHostKeys {
        PhoneHostKeys {
            path,
            lock: Mutex::new(()),
        }
    }

    /// 那份文件（没有 ⇒ 空串）。读不出来的不当空的（当空的写回会抹掉别的几台）。
    fn text(&self) -> Result<String, Said> {
        match std::fs::read_to_string(&self.path) {
            Ok(t) => Ok(t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(Said::from(e.to_string())),
        }
    }

    /// 写整份：先写旁边一份再换名（写到一半断电不留半份）。
    fn write(&self, text: &str) -> Result<(), Said> {
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, text)
            .and_then(|()| std::fs::rename(&tmp, &self.path))
            .map_err(|e| Said::from(e.to_string()))
    }

    /// 忘掉这台（「钥匙变了」页上点了才调）：删这台所有行。
    pub fn forget(&self, host: &str, port: u16) -> Result<(), Said> {
        let at = [Endpoint {
            host: host.to_string(),
            port,
        }];
        self.rewrite(&mut |t| known_hosts::forgotten(t, &at))
    }
}

impl HostKeys for PhoneHostKeys {
    fn read(&self) -> Result<String, Said> {
        self.text()
    }

    fn rewrite(&self, edit: &mut dyn FnMut(&str) -> Option<String>) -> Result<(), Said> {
        let _held = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let old = self.text()?;
        match edit(&old) {
            Some(new) => self.write(&new),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ED: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH6Bv4yRS04VSkWcl9S10D9LjpurtRpINBdZiZpLhXHd";
    const ED2: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOANKshIlRrz0AhOUZAikn+rxmdOeexg0tdpEwkB4mBf";

    /// 一份只给这一条判据的库文件（沙箱：系统临时目录下按进程号与名字分开）。
    fn book(name: &str) -> PhoneHostKeys {
        let dir = std::env::temp_dir().join(format!("mobcore-hk-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        PhoneHostKeys::new(dir.join("known_hosts"))
    }

    fn at(host: &str, port: u16) -> Vec<Endpoint> {
        vec![Endpoint {
            host: host.to_string(),
            port,
        }]
    }

    #[test]
    fn no_file_reads_as_an_empty_book_and_a_first_use_lands_in_the_file() {
        let b = book("first");
        assert_eq!(b.read().unwrap(), "");
        b.rewrite(&mut |t| known_hosts::remembered(t, &at("box", 2222), ED))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&b.path).unwrap(),
            format!("[box]:2222 {ED}\n")
        );
        assert_eq!(
            known_hosts::known(&b.read().unwrap(), &at("box", 2222)).len(),
            1
        );
    }

    #[test]
    fn forgetting_a_machine_drops_its_lines_and_keeps_the_others() {
        let b = book("forget");
        std::fs::write(&b.path, format!("a,b {ED}\nc {ED2}\n")).unwrap();
        b.forget("b", 22).unwrap();
        assert_eq!(b.read().unwrap(), format!("c {ED2}\n"));
    }
}
