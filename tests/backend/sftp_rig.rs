//! **合成 SFTP 服务端**（台架）：`ssh-dial/sftp.rs` 与 `control/transfer.rs` 两族判据共用。
//!
//! 来历：monitor 那一侧 `tests/frontend/shell/sftp_staging_tests.rs`（F7c）那台「逐条记改动路径」的服务端，
//! SFTP 客户端搬进本机后端时一起搬过来，再补三样本仓那台没有的：
//! - **`realpath`**：远端写围栏靠它（`"."` ⇒ home；逐段解链接；不在 ⇒ `NoSuchFile`，同 OpenSSH 的 `realpath(3)`）；
//! - **目录链接**（`links`）：「`bin/x` 是一条指向 `~/.ssh` 的链接」那一形要一台真会解链接的服务端才造得出来；
//! - **`lstat` 认得链接**：开写之前那一问。
//!
//! 🔴 **判据自己看改动表（`mutated`），不信被测那一侧的自述** —— 同原台架那条纪律。
//! 路径一律按**起始目录相对**记（客户端发绝对路径时，服务端把 `HOME/` 前缀剥掉再记）。
//!
//! 买不到：真 sshd 的 sftp-server 行为（那一格在 `tests/evidence/SR1b-sftp-loopback.py`）。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};

/// 台架的 home（`realpath(".")` 的答案）。
pub const HOME: &str = "/home/rig";

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub bytes: Vec<u8>,
}

#[derive(Default, Debug)]
pub struct Fs {
    pub files: BTreeMap<String, Entry>,
    pub dirs: BTreeSet<String>,
    /// 目录链接：`链接路径 → 它指向的（起始目录相对）路径`。
    pub links: BTreeMap<String, String>,
    handles: HashMap<String, String>,
    /// 开着的目录句柄：`句柄 → (目录, 已经交过一趟了没有)`。
    dir_handles: HashMap<String, (String, bool)>,
    next_handle: u32,
    /// 🔴 **本台架的针**：每一次会改东西的请求碰的路径，按到达顺序 `(动词, 路径)`。
    pub mutated: Vec<(String, String)>,
    /// 第 N 次（0 起）`WRITE` 之后开始回 `FAILURE`（「失败留」那一格要一个中途真坏的形状）。
    pub fail_write_after: Option<usize>,
    write_calls: usize,
    /// 第 N 次（0 起）`READ` 之后开始回 `FAILURE`（下载「中途坏了」那一形）。
    pub fail_read_after: Option<usize>,
    read_calls: usize,
    /// 每一次 `WRITE` 的偏移（续传那一格判「从哪儿接上的」）。
    pub write_offsets: Vec<u64>,
    /// 服务端**看到过**的写（含回坏的那几条）盖到的最远字节 —— 「客户端发出去的写，走之前是不是都有了回话」。
    pub seen_write_end: u64,
    /// 每条 `WRITE` 处理之前让出几次（`yield_now`）：把「写还在路上、客户端已经拿到第一条坏回话」那一形
    /// 从调度的运气变成台架的设定（负载高时才碰得上的那个竞态，在这里每次都碰上）。
    pub yield_per_write: usize,
    /// 每一次 `READ` 的偏移（下载续传那一格判「前缀没有被重新读一遍」）。
    pub read_offsets: Vec<u64>,
    /// 服务端**交出去**的读字节数（逐条 `READ` 应答里 `data` 的长度之和）—— 「同一段字节有没有被要了两遍」。
    pub served_bytes: u64,
    /// 目录上被 `SETSTAT` 设过的权限位（`目录 → permissions`）—— 部署建出来的那几层收没收窄。
    pub dir_modes: BTreeMap<String, u32>,
    /// 列目录时报的修改时间（`路径 → 秒`；没登记 ⇒ 不报）—— 残件新旧那一格要一个「早就放在那儿」的形状。
    pub mtimes: BTreeMap<String, u32>,
    /// 读回来的字节第一个翻一位（「传坏了、长度还对」那一形：读回比对要抓得住）。
    pub corrupt_reads: bool,
    /// `REMOVE` 一律回 `FAILURE`（「传坏的那一份删不掉」那一形）。
    pub refuse_remove: bool,
}

impl Fs {
    fn alloc(&mut self) -> String {
        let id = self.next_handle;
        self.next_handle += 1;
        format!("h{id}")
    }
    fn touch(&mut self, verb: &str, path: &str) {
        self.mutated.push((verb.to_string(), path.to_string()));
    }
    /// 改动表里碰过的路径（去重、排序）。
    pub fn touched(&self) -> BTreeSet<String> {
        self.mutated.iter().map(|(_, p)| p.clone()).collect()
    }
    /// 一份文件此刻的字节。
    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).map(|e| e.bytes.clone())
    }
}

/// 客户端发来的路径 ⇒ 起始目录相对（绝对路径剥掉 `HOME/`；`"."` ⇒ `""`）。
fn rel(path: &str) -> String {
    let p = path.strip_prefix(&format!("{HOME}/")).unwrap_or(path);
    let p = if path == HOME { "" } else { p };
    if p == "." {
        String::new()
    } else {
        p.trim_end_matches('/').to_string()
    }
}

struct Server {
    fs: Arc<Mutex<Fs>>,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Success".to_string(),
        language_tag: "en-US".to_string(),
    }
}

fn attrs_of(e: &Entry) -> FileAttributes {
    FileAttributes {
        size: Some(e.bytes.len() as u64),
        permissions: Some(0o100_644),
        ..Default::default()
    }
}

fn dir_attrs() -> FileAttributes {
    FileAttributes {
        permissions: Some(0o040_755),
        ..Default::default()
    }
}

/// 逐段解链接（只认目录链接那张表）。回解完的起始目录相对路径；某一段不在 ⇒ `None`。
fn resolve(fs: &Fs, p: &str) -> Option<String> {
    if p.is_empty() {
        return Some(String::new());
    }
    let mut cur = String::new();
    for comp in p.split('/') {
        let next = if cur.is_empty() {
            comp.to_string()
        } else {
            format!("{cur}/{comp}")
        };
        cur = match fs.links.get(&next) {
            Some(t) => t.clone(),
            None => next,
        };
        if !fs.dirs.contains(&cur) && !fs.files.contains_key(&cur) {
            return None;
        }
    }
    Some(cur)
}

impl russh_sftp::server::Handler for Server {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> impl std::future::Future<Output = Result<Version, Self::Error>> + Send {
        async move {
            Ok(Version {
                version: 3,
                extensions: HashMap::new(),
            })
        }
    }

    fn realpath(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Name, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let fs = fs.lock().unwrap();
            let r = resolve(&fs, &rel(&path)).ok_or(StatusCode::NoSuchFile)?;
            let abs = if r.is_empty() {
                HOME.to_string()
            } else {
                format!("{HOME}/{r}")
            };
            Ok(Name {
                id,
                files: vec![File::dummy(abs)],
            })
        }
    }

    fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> impl std::future::Future<Output = Result<Handle, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let p = rel(&filename);
            let writes =
                pflags.intersects(OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE);
            if writes {
                fs.touch("open-w", &p);
            }
            if pflags.contains(OpenFlags::CREATE) {
                if pflags.contains(OpenFlags::EXCLUDE) && fs.files.contains_key(&p) {
                    return Err(StatusCode::Failure);
                }
                let parent_ok = match p.rsplit_once('/') {
                    Some((parent, _)) => fs.dirs.contains(parent),
                    None => true,
                };
                if !parent_ok {
                    return Err(StatusCode::NoSuchFile);
                }
                if pflags.contains(OpenFlags::TRUNCATE) || !fs.files.contains_key(&p) {
                    fs.files.insert(p.clone(), Entry::default());
                }
            } else if !fs.files.contains_key(&p) {
                return Err(StatusCode::NoSuchFile);
            }
            let h = fs.alloc();
            fs.handles.insert(h.clone(), p);
            Ok(Handle { id, handle: h })
        }
    }

    fn close(
        &mut self,
        id: u32,
        handle: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            fs.lock().unwrap().handles.remove(&handle);
            Ok(ok(id))
        }
    }

    fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> impl std::future::Future<Output = Result<Data, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            fs.read_offsets.push(offset);
            fs.read_calls += 1;
            if fs.fail_read_after.is_some_and(|n| fs.read_calls > n) {
                return Err(StatusCode::Failure);
            }
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?;
            let e = fs.files.get(path).ok_or(StatusCode::NoSuchFile)?;
            let start = offset as usize;
            if start >= e.bytes.len() {
                return Err(StatusCode::Eof);
            }
            let end = (start + len as usize).min(e.bytes.len());
            let mut data = e.bytes[start..end].to_vec();
            if fs.corrupt_reads && start == 0 {
                if let Some(b) = data.first_mut() {
                    *b ^= 1;
                }
            }
            fs.served_bytes += data.len() as u64;
            Ok(Data { id, data })
        }
    }

    fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let yields = fs.lock().unwrap().yield_per_write;
            for _ in 0..yields {
                tokio::task::yield_now().await;
            }
            let mut fs = fs.lock().unwrap();
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?.clone();
            fs.touch("write", &path);
            let end = offset + data.len() as u64;
            fs.seen_write_end = fs.seen_write_end.max(end);
            fs.write_calls += 1;
            if fs.fail_write_after.is_some_and(|n| fs.write_calls > n) {
                return Err(StatusCode::Failure);
            }
            fs.write_offsets.push(offset);
            let e = fs.files.get_mut(&path).ok_or(StatusCode::NoSuchFile)?;
            let at = offset as usize;
            if e.bytes.len() < at + data.len() {
                e.bytes.resize(at + data.len(), 0);
            }
            e.bytes[at..at + data.len()].copy_from_slice(&data);
            Ok(ok(id))
        }
    }

    fn stat(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Attrs, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let fs = fs.lock().unwrap();
            let p = resolve(&fs, &rel(&path)).ok_or(StatusCode::NoSuchFile)?;
            if p.is_empty() || fs.dirs.contains(&p) {
                return Ok(Attrs {
                    id,
                    attrs: dir_attrs(),
                });
            }
            let e = fs.files.get(&p).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: attrs_of(e),
            })
        }
    }

    fn lstat(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Attrs, Self::Error>> + Send {
        let fs = self.fs.clone();
        let p = rel(&path);
        let is_link = fs.lock().unwrap().links.contains_key(&p);
        let plain = self.stat(id, path);
        async move {
            if is_link {
                return Ok(Attrs {
                    id,
                    attrs: FileAttributes {
                        permissions: Some(0o120_777),
                        ..Default::default()
                    },
                });
            }
            drop(fs);
            plain.await
        }
    }

    fn fstat(
        &mut self,
        id: u32,
        handle: String,
    ) -> impl std::future::Future<Output = Result<Attrs, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let fs = fs.lock().unwrap();
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?;
            let e = fs.files.get(path).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: attrs_of(e),
            })
        }
    }

    fn remove(
        &mut self,
        id: u32,
        filename: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let p = rel(&filename);
            fs.touch("remove", &p);
            if fs.refuse_remove {
                return Err(StatusCode::Failure);
            }
            match fs.files.remove(&p) {
                Some(_) => Ok(ok(id)),
                None => Err(StatusCode::NoSuchFile),
            }
        }
    }

    fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let p = rel(&path);
            fs.touch("mkdir", &p);
            let parent_ok = p.rsplit_once('/').is_none_or(|(q, _)| fs.dirs.contains(q));
            if !parent_ok || fs.dirs.contains(&p) {
                return Err(StatusCode::Failure);
            }
            fs.dirs.insert(p);
            Ok(ok(id))
        }
    }

    fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let (o, n) = (rel(&oldpath), rel(&newpath));
            fs.touch("rename-from", &o);
            fs.touch("rename-to", &n);
            // 标准 SFTP v3 的改名不覆盖（OpenSSH 的 sftp-server 在目标已在时回 FAILURE）。
            if fs.files.contains_key(&n) {
                return Err(StatusCode::Failure);
            }
            let e = fs.files.remove(&o).ok_or(StatusCode::NoSuchFile)?;
            fs.files.insert(n, e);
            Ok(ok(id))
        }
    }

    fn opendir(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Handle, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let p = resolve(&fs, &rel(&path)).ok_or(StatusCode::NoSuchFile)?;
            if !(p.is_empty() || fs.dirs.contains(&p)) {
                return Err(StatusCode::NoSuchFile);
            }
            let h = fs.alloc();
            fs.dir_handles.insert(h.clone(), (p, false));
            Ok(Handle { id, handle: h })
        }
    }

    fn readdir(
        &mut self,
        id: u32,
        handle: String,
    ) -> impl std::future::Future<Output = Result<Name, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let (dir, done) = fs
                .dir_handles
                .get(&handle)
                .cloned()
                .ok_or(StatusCode::Failure)?;
            if done {
                return Err(StatusCode::Eof);
            }
            fs.dir_handles.insert(handle, (dir.clone(), true));
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            let direct = |k: &String| {
                k.strip_prefix(&prefix)
                    .filter(|rest| !rest.is_empty() && !rest.contains('/'))
                    .map(str::to_string)
            };
            let mut files: Vec<File> = fs
                .dirs
                .iter()
                .filter_map(|d| direct(d).map(|n| File::new(n, dir_attrs())))
                .collect();
            files.extend(fs.files.iter().filter_map(|(k, e)| {
                direct(k).map(|n| {
                    let mut a = attrs_of(e);
                    a.mtime = fs.mtimes.get(k).copied();
                    File::new(n, a)
                })
            }));
            Ok(Name { id, files })
        }
    }

    fn rmdir(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let p = rel(&path);
            fs.touch("rmdir", &p);
            let busy = fs
                .files
                .keys()
                .chain(fs.dirs.iter())
                .any(|k| k.starts_with(&format!("{p}/")));
            if busy || !fs.dirs.remove(&p) {
                return Err(StatusCode::Failure);
            }
            Ok(ok(id))
        }
    }

    fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let p = rel(&path);
            fs.touch("setstat", &p);
            if let (true, Some(m)) = (fs.dirs.contains(&p), attrs.permissions) {
                fs.dir_modes.insert(p, m);
            }
            Ok(ok(id))
        }
    }
}

/// 起一台：`~/.cc-monitor` 在（后端的家）；`bin` / `staging` 按参数建。
pub fn home(with_bin: bool, with_staging: bool) -> Arc<Mutex<Fs>> {
    let mut fs = Fs::default();
    fs.dirs.insert(".cc-monitor".to_string());
    if with_bin {
        fs.dirs.insert(".cc-monitor/bin".to_string());
    }
    if with_staging {
        fs.dirs.insert(".cc-monitor/staging".to_string());
    }
    Arc::new(Mutex::new(fs))
}

/// 在台架上起一条后端的 SFTP 会话（生产 `Session::over` 那一个口 —— 被判的是同一份实现）。
pub async fn session_on(fs: Arc<Mutex<Fs>>) -> crate::sftp::Session {
    // 本 crate 自己的判据里装测试档那一套（写按部署那两处）；后端的测试档由后端自己装它那一套（`dial::host`）。
    #[cfg(test)]
    crate::guard_support::test_host();
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    russh_sftp::server::run(server_io, Server { fs }).await;
    crate::sftp::Session::over(client_io, Box::new(()))
        .await
        .expect("合成服务端上的 sftp 会话应当开得起来")
}

/// 合成语料：编译期拼、含不可打印字节（一个真会话正文的字节都没有）。
pub fn corpus(len: usize) -> Vec<u8> {
    let unit: [u8; 16] = *b"SR1b-rig\x00\x01\x02\x03\x04\x05\x06\x07";
    unit.iter().copied().cycle().take(len).collect()
}
