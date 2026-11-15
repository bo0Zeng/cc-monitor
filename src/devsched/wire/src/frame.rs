//! 帧：一条连接上一行一个 JSON 对象，`t` 是帧名。各帧谁发、各键什么意思写在 `shape.rs` 的说明表里（FRAMES.md 由它生成）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Frame {
    Hello(Hello),
    Facts(Facts),
    Job(Job),
    Out(Out),
    Exit(Exit),
    Stop(Stop),
    Beat,
    Clean(Clean),
    Error(ErrorFrame),
}

impl Frame {
    /// 认得的帧名（与 `t` 的值逐字一样）。
    pub const NAMES: &'static [&'static str] = &[
        "hello", "facts", "job", "out", "exit", "stop", "beat", "clean", "error",
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Frame::Hello(_) => "hello",
            Frame::Facts(_) => "facts",
            Frame::Job(_) => "job",
            Frame::Out(_) => "out",
            Frame::Exit(_) => "exit",
            Frame::Stop(_) => "stop",
            Frame::Beat => "beat",
            Frame::Clean(_) => "clean",
            Frame::Error(_) => "error",
        }
    }
}

/// 节点起来先发的一帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub proto: String,
    pub version: String,
    pub caps: Vec<Cap>,
    pub programs: Vec<String>,
    pub root: String,
    pub heartbeat_timeout_s: u64,
    /// 从 `<根>/projects` 往上各层会被 cargo 读到的 `.cargo/config.toml` · `.cargo/config`（根在家目录下时那台家目录的配置会生效）。
    pub cargo_config: Vec<String>,
}

/// 一项能力：名字固定（cargo · tmux · tmuxnew · node · gui · jdk · android · rsync），版本照探到的原样。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cap {
    pub name: String,
    pub version: String,
}

/// 资源事实；量不到的那一格是 `null`（调度端判不了 ⇒ 不放行）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    pub mem: Option<u64>,
    pub psi: Option<f64>,
    pub disk: Option<u64>,
    pub tmp: Option<u64>,
    pub threads: Option<u64>,
    pub nproc: Option<u64>,
    pub ours: Option<u64>,
    pub jobs: u64,
}

/// 交作业。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub job: u64,
    pub project: String,
    pub tree: String,
    pub rel: String,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub stop: StopLines,
    pub nice: Option<i32>,
}

/// 跑着的停线；键与 `Breach` 的值逐字一样。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StopLines {
    pub mem: Option<u64>,
    pub psi: Option<f64>,
    pub disk: Option<u64>,
}

/// 作业的一截输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Out {
    pub job: u64,
    pub stream: Stream,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    pub const ALL: &'static [Stream] = &[Stream::Stdout, Stream::Stderr];
}

/// 作业收尾。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    pub job: u64,
    pub code: Option<i32>,
    pub stop: StopReason,
    pub breach: Option<Breach>,
    pub skipped: Vec<String>,
}

/// 停因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// 跑完（退出码是命令自己的）
    Done,
    /// 调度端发了停作业
    Stopped,
    /// 撞停线（`breach` 写哪一条）
    Breach,
    /// 账号线程到线
    Threads,
    /// 起不了进程
    Spawn,
    /// 心跳断
    Heartbeat,
    /// 临时目录路径超长
    TmpPath,
}

impl StopReason {
    pub const ALL: &'static [StopReason] = &[
        StopReason::Done,
        StopReason::Stopped,
        StopReason::Breach,
        StopReason::Threads,
        StopReason::Spawn,
        StopReason::Heartbeat,
        StopReason::TmpPath,
    ];

    /// 穷举：加一种停因而不进 `ALL`，测试里序号对不上。
    pub fn ordinal(self) -> usize {
        match self {
            StopReason::Done => 0,
            StopReason::Stopped => 1,
            StopReason::Breach => 2,
            StopReason::Threads => 3,
            StopReason::Spawn => 4,
            StopReason::Heartbeat => 5,
            StopReason::TmpPath => 6,
        }
    }
}

/// 撞的哪条停线。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Breach {
    Mem,
    Psi,
    Disk,
}

impl Breach {
    pub const ALL: &'static [Breach] = &[Breach::Mem, Breach::Psi, Breach::Disk];

    pub fn ordinal(self) -> usize {
        match self {
            Breach::Mem => 0,
            Breach::Psi => 1,
            Breach::Disk => 2,
        }
    }
}

/// 停作业。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stop {
    pub job: u64,
}

/// 清理：删节点那台上这棵树的工作树与它的产物目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clean {
    pub project: String,
    pub tree: String,
}

/// 错误：说不上是哪一趟的 `job` 为 `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorFrame {
    pub job: Option<u64>,
    pub msg: String,
}

/// 每种帧一份取全的样本（可选的格都填上，停因 · 撞线 · 流各种都出现一次）：帧形、指纹、往返测试都从这里取。
/// 结构体一律写全字段（加一个字段不补这里就编不过）。
pub fn samples() -> Vec<Frame> {
    let mut v = vec![
        Frame::Hello(Hello {
            proto: "0000000000000000".into(),
            version: "0.1.0".into(),
            caps: vec![Cap {
                name: "cargo".into(),
                version: "1.98.1".into(),
            }],
            programs: vec!["sccache".into()],
            root: "/srv/devsched".into(),
            heartbeat_timeout_s: 30,
            cargo_config: vec!["/home/u/.cargo/config.toml".into()],
        }),
        Frame::Facts(Facts {
            mem: Some(1024),
            psi: Some(1.5),
            disk: Some(2048),
            tmp: Some(512),
            threads: Some(100),
            nproc: Some(250),
            ours: Some(3),
            jobs: 1,
        }),
        Frame::Job(Job {
            job: 7,
            project: "alpha".into(),
            tree: "w1-0123abcd".into(),
            rel: "src".into(),
            argv: vec!["cargo".into(), "test".into()],
            env: [("CARGO_INCREMENTAL".to_string(), "0".to_string())]
                .into_iter()
                .collect(),
            stop: StopLines {
                mem: Some(2048),
                psi: Some(60.0),
                disk: Some(4096),
            },
            nice: Some(19),
        }),
        Frame::Stop(Stop { job: 7 }),
        Frame::Beat,
        Frame::Clean(Clean {
            project: "alpha".into(),
            tree: "w1-0123abcd".into(),
        }),
        Frame::Error(ErrorFrame {
            job: Some(7),
            msg: "说明".into(),
        }),
    ];
    for s in Stream::ALL {
        v.push(Frame::Out(Out {
            job: 7,
            stream: *s,
            data: "一截\n".into(),
        }));
    }
    for (i, r) in StopReason::ALL.iter().enumerate() {
        let breach = if *r == StopReason::Breach {
            Some(Breach::ALL[i % Breach::ALL.len()])
        } else {
            None
        };
        v.push(Frame::Exit(Exit {
            job: 7,
            code: Some(0),
            stop: *r,
            breach,
            skipped: vec!["mobile".into()],
        }));
    }
    // 可选的格空着的那一份：帧形里才看得出哪些格可以是 null
    v.push(Frame::Facts(Facts {
        mem: None,
        psi: None,
        disk: None,
        tmp: None,
        threads: None,
        nproc: None,
        ours: None,
        jobs: 0,
    }));
    v.push(Frame::Job(Job {
        job: 9,
        project: "alpha".into(),
        tree: "w1-0123abcd".into(),
        rel: String::new(),
        argv: vec!["true".into()],
        env: BTreeMap::new(),
        stop: StopLines {
            mem: None,
            psi: None,
            disk: None,
        },
        nice: None,
    }));
    v.push(Frame::Error(ErrorFrame {
        job: None,
        msg: "说明".into(),
    }));
    for b in Breach::ALL {
        v.push(Frame::Exit(Exit {
            job: 8,
            code: None,
            stop: StopReason::Breach,
            breach: Some(*b),
            skipped: vec![],
        }));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_lists_are_complete() {
        for (i, r) in StopReason::ALL.iter().enumerate() {
            assert_eq!(r.ordinal(), i);
        }
        for (i, b) in Breach::ALL.iter().enumerate() {
            assert_eq!(b.ordinal(), i);
        }
        for f in samples() {
            assert!(Frame::NAMES.contains(&f.name()));
        }
    }
}
