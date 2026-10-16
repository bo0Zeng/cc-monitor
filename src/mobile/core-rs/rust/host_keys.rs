//! **主机钥匙那份库在手机上**：App 私有目录下一份 OpenSSH known_hosts（路径由 Kotlin 装核心时交）。
//! 读 · 记 · 忘的规矩在拨号核心 `ssh_dial::known_hosts`（后端同一份）；这里只有这份文件的读写（同一个进程只有核心写它，锁是进程内的一把）
//! ＋ Room 旧表迁进来那一次（[`PhoneHostKeys::migrate`]，只活这一版）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;

use ssh_dial::env::HostKeys;
use ssh_dial::{known_hosts, Endpoint, Said};

pub struct PhoneHostKeys {
    path: PathBuf,
    /// 改写那一段的锁（读 → 改 → 写回整份）。
    lock: Mutex<()>,
    /// 旧表没迁完那一句：在 ⇒ 读库就拒（不当首见，否则没迁过来的那几台下次连就是一次首见）。
    unmigrated: Mutex<Option<String>>,
}

/// Room 旧表的一行（Kotlin 原样交来）。
pub struct OldRow {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    /// 公钥 blob 的 base64。
    pub key: String,
}

impl OldRow {
    /// Kotlin 交来的平铺数组：「主机 · 口 · 类型 · base64」四个一行。缺格 · 口读不动的那一行照样算一行（读不回来 ⇒ 对不上，不悄悄少数）。
    pub fn flat(cells: &[String]) -> Vec<OldRow> {
        cells
            .chunks(4)
            .map(|c| {
                let at = |i: usize| c.get(i).cloned().unwrap_or_default();
                OldRow {
                    host: at(0),
                    port: at(1).parse().unwrap_or(0),
                    key_type: at(2),
                    key: at(3),
                }
            })
            .collect()
    }
}

impl PhoneHostKeys {
    pub fn new(path: PathBuf) -> PhoneHostKeys {
        PhoneHostKeys {
            path,
            lock: Mutex::new(()),
            unmigrated: Mutex::new(None),
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

    /// Room 旧表逐行写进库、按同样的「主机 ＋ 口」读回比：每台读回的钥匙集合与旧表那几行相等 ⇒ `None`（Kotlin 删旧表）；
    /// 对不上 ⇒ 那一句，且此后读库就拒（这一道闸只活这一版，下次启动再迁）。
    pub fn migrate(&self, rows: &[OldRow]) -> Option<String> {
        let mut want: BTreeMap<(String, u16), BTreeSet<String>> = BTreeMap::new();
        for r in rows {
            want.entry((r.host.clone(), r.port))
                .or_default()
                .insert(format!("{} {}", r.key_type, r.key));
        }
        // 写不成也照读回比（比出来对不上就是那一句）。
        let _ = self.rewrite(&mut |text| {
            let mut out = text.to_string();
            for ((host, port), keys) in &want {
                let at = [Endpoint {
                    host: host.clone(),
                    port: *port,
                }];
                let have = lines_of(&known_hosts::known(&out, &at));
                for k in keys.iter().filter(|k| !have.contains(*k)) {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&format!("{} {k}\n", known_hosts::names(&at)));
                }
            }
            (out != text).then_some(out)
        });
        let back = self.text().unwrap_or_default();
        let mut read = 0;
        let mut same = true;
        for ((host, port), keys) in &want {
            let got = lines_of(&known_hosts::known(
                &back,
                &[Endpoint {
                    host: host.clone(),
                    port: *port,
                }],
            ));
            read += got.len();
            same &= got == *keys;
        }
        if same {
            return None;
        }
        let said = copy_core::copy_text(
            "mobSsh.hostKey.unmigrated",
            &[
                ("n", &rows.len().to_string()),
                ("m", &read.to_string()),
            ],
        );
        *self.unmigrated.lock().unwrap_or_else(|e| e.into_inner()) = Some(said.clone());
        Some(said)
    }
}

/// 钥匙 ⇒ OpenSSH 那一行的前两段（算法 ＋ base64），与旧表那一行同形好比。
fn lines_of(keys: &[russh::keys::PublicKey]) -> BTreeSet<String> {
    keys.iter()
        .filter_map(|k| k.to_openssh().ok())
        .map(|l| l.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
        .collect()
}

impl HostKeys for PhoneHostKeys {
    fn read(&self) -> Result<String, Said> {
        if let Some(said) = self
            .unmigrated
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Err(Said::from(said));
        }
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

    const ED: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH6Bv4yRS04VSkWcl9S10D9LjpurtRpINBdZiZpLhXHd";
    const ED2: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOANKshIlRrz0AhOUZAikn+rxmdOeexg0tdpEwkB4mBf";
    const EC: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBJm7kzon0dT8iSRRKFOAo/KRt5r1yFo3HvtUQroP/W+xCqxtDGJszr9PKxtduCWA8L9Sc8XDBtsyrNOrTtJE/9A=";

    /// 一份只给这一条判据的库文件（沙箱：系统临时目录下按进程号与名字分开）。
    fn book(name: &str) -> PhoneHostKeys {
        let dir = std::env::temp_dir().join(format!("mobcore-hk-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        PhoneHostKeys::new(dir.join("known_hosts"))
    }

    fn row(host: &str, port: u16, key: &str) -> OldRow {
        let (t, k) = key.split_once(' ').unwrap();
        OldRow {
            host: host.to_string(),
            port,
            key_type: t.to_string(),
            key: k.to_string(),
        }
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
        assert_eq!(lines_of(&known_hosts::known(&b.read().unwrap(), &at("box", 2222))).len(), 1);
    }

    #[test]
    fn forgetting_a_machine_drops_its_lines_and_keeps_the_others() {
        let b = book("forget");
        std::fs::write(&b.path, format!("a,b {ED}\nc {ED2}\n")).unwrap();
        b.forget("b", 22).unwrap();
        assert_eq!(b.read().unwrap(), format!("c {ED2}\n"));
    }

    #[test]
    fn old_rows_migrate_once_and_read_back_equal() {
        let b = book("migrate");
        // 同一台两种类型 · 另一台非 22 口；库里已有的那一把不重写。
        std::fs::write(&b.path, format!("box {ED}\n")).unwrap();
        let rows = [
            row("box", 22, ED),
            row("box", 22, EC),
            row("other", 2200, ED2),
        ];
        assert_eq!(b.migrate(&rows), None);
        let text = b.read().unwrap();
        assert_eq!(text.lines().count(), 3, "{text}");
        assert_eq!(lines_of(&known_hosts::known(&text, &at("box", 22))).len(), 2);
        assert!(text.contains(&format!("[other]:2200 {ED2}")));
        // 再迁一次（旧表删之前又起来了）：不重写。
        assert_eq!(b.migrate(&rows), None);
        assert_eq!(b.read().unwrap(), text);
    }

    #[test]
    fn rows_that_do_not_read_back_keep_the_gate_shut_and_say_how_many() {
        let b = book("short");
        // 认不出的那一行写进去也读不回来 ⇒ 对不上。
        let rows = [row("box", 22, ED), row("bad", 22, "ssh-ed25519 not-base64")];
        let said = b.migrate(&rows).expect("对不上要说一句");
        assert_eq!(said, copy_core::copy_text("mobSsh.hostKey.unmigrated", &[("n", "2"), ("m", "1")]));
        // 闸关着：读库就拒（这台记过的也拒，没迁完不认首见）。
        assert_eq!(b.read().unwrap_err().said, said);
    }

    #[test]
    fn the_rows_kotlin_hands_over_parse_and_a_broken_one_still_counts() {
        let cells: Vec<String> = ["box", "2222", "ssh-ed25519", "AAAA", "x"].map(String::from).to_vec();
        let rows = OldRow::flat(&cells);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].host.as_str(), rows[0].port, rows[0].key_type.as_str(), rows[0].key.as_str()), ("box", 2222, "ssh-ed25519", "AAAA"));
        assert_eq!(rows[1].port, 0);
    }

    #[test]
    fn an_empty_old_table_leaves_the_book_untouched() {
        let b = book("empty");
        assert_eq!(b.migrate(&[]), None);
        assert!(!b.path.exists());
        assert_eq!(b.read().unwrap(), "");
    }
}
