//! 帧形：从 `samples()` 序列化出来的键与 JSON 类型；协议指纹是它的摘要，FRAMES.md 是它的文字版。

use crate::frame::{samples, Frame};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// 值是一组固定名字的键：帧形里列出全部取值。
const ENUM_PATHS: &[&str] = &["out.stream", "exit.stop", "exit.breach"];
/// 键不固定的对象（名 → 值）。
const MAP_PATHS: &[&str] = &["job.env"];

/// 各帧谁发、什么意思。
const FRAME_DOC: &[(&str, &str, &str)] = &[
    ("hello", "节点 → 调度端", "连上先发的一帧；`proto` 与调度端的指纹不同 ⇒ 调度端不交作业"),
    ("facts", "节点 → 调度端", "资源事实：hello 之后发一次，此后每收到一次 beat 回一次；量不到的格是 null"),
    ("job", "调度端 → 节点", "交作业：节点在 `<根>/projects/<project>/<tree>/<rel>` 起 `argv`，环境 ＝ 节点自己设的三类 ＋ `env`"),
    ("out", "节点 → 调度端", "作业的一截输出（不是 UTF-8 的字节按替换字符写）"),
    ("exit", "节点 → 调度端", "作业收尾：只有 `stop` = done 时 `code` 是命令自己的退出码"),
    ("stop", "调度端 → 节点", "停作业：整组停掉，回 exit（stop = stopped）"),
    ("beat", "调度端 → 节点", "心跳，调度端每 5 秒一次；节点超过 hello 报的时限收不到 ⇒ 停掉这条连接上的作业"),
    ("clean", "调度端 → 节点", "删 `<根>/projects/<project>/<tree>` 与这棵树的产物目录"),
    ("error", "两头都发", "说不成一帧回话的错；说得上是哪一趟的带 `job`"),
];

/// 各键的说明。
const FIELD_DOC: &[(&str, &str)] = &[
    ("hello.proto", "协议指纹（本文件头上那个）"),
    ("hello.version", "节点版本"),
    ("hello.caps", "能力：名字固定（cargo · tmux · tmuxnew · node · gui · jdk · android · rsync）"),
    ("hello.caps[].name", "能力名"),
    ("hello.caps[].version", "探到的版本，原样"),
    ("hello.programs", "PATH 里查到的程序（sccache · cc 等），供调度端核指向程序的变量"),
    ("hello.root", "节点的登记根（`--root`）"),
    ("hello.heartbeat_timeout_s", "心跳时限，秒"),
    ("hello.cargo_config", "从 `<根>/projects` 往上各层会被 cargo 读到的 `.cargo/config.toml` · `.cargo/config`（根在家目录下时那台家目录的配置会生效）"),
    ("facts.mem", "可用内存，MiB"),
    ("facts.psi", "内存压力 some avg10，百分数"),
    ("facts.disk", "根目录所在盘剩余，MiB"),
    ("facts.tmp", "/tmp 剩余，MiB"),
    ("facts.threads", "本账号此刻的线程数"),
    ("facts.nproc", "RLIMIT_NPROC"),
    ("facts.ours", "节点名下（连节点自己）的线程数"),
    ("facts.jobs", "这台此刻在跑几趟"),
    ("job.job", "号（一条连接上不重）"),
    ("job.project", "项目名"),
    ("job.tree", "树名"),
    ("job.rel", "树里的相对目录（空 ＝ 树根）"),
    ("job.argv", "命令数组"),
    ("job.env", "带过去的环境（名 → 值，空值照设）"),
    ("job.stop", "停线；没写的那条不看"),
    ("job.stop.mem", "可用内存低于它（MiB）"),
    ("job.stop.psi", "内存压力不低于它（百分数）"),
    ("job.stop.disk", "根目录所在盘剩余低于它（MiB）"),
    ("job.nice", "nice 值"),
    ("out.job", "号"),
    ("out.stream", "哪一路"),
    ("out.data", "这一截"),
    ("exit.job", "号"),
    ("exit.code", "退出码；命令被信号打死的折成 128 ＋ 信号；stop ≠ done 的是 null"),
    ("exit.stop", "停因：done 跑完 · stopped 停作业 · breach 撞停线 · threads 线程上限 · spawn 起不了进程 · heartbeat 心跳断 · tmp_path 临时目录路径超长"),
    ("exit.breach", "stop = breach 时撞的哪条，与 job.stop 的键逐字一样"),
    ("exit.skipped", "作业自己报的没跑的段"),
    ("stop.job", "号"),
    ("clean.project", "项目名"),
    ("clean.tree", "树名"),
    ("error.job", "号；说不上的是 null"),
    ("error.msg", "说明"),
];

/// 一个键：出现过的 JSON 类型，与固定取值（只 ENUM_PATHS 记）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Field {
    pub types: BTreeSet<String>,
    pub values: BTreeSet<String>,
}

/// 帧名 → 键路径 → 键。
pub type Shape = BTreeMap<String, BTreeMap<String, Field>>;

pub fn shape_of(frames: &[Frame]) -> Shape {
    let values: Vec<Value> = frames
        .iter()
        .map(|f| serde_json::to_value(f).expect("帧总序列化得了"))
        .collect();
    shape_of_values(&values)
}

pub fn shape_of_values(values: &[Value]) -> Shape {
    let mut s = Shape::new();
    for v in values {
        let Some(o) = v.as_object() else { continue };
        let name = o
            .get("t")
            .and_then(|t| t.as_str())
            .unwrap_or("?")
            .to_string();
        let fields = s.entry(name.clone()).or_default();
        for (k, x) in o {
            if k == "t" {
                continue;
            }
            walk(fields, &format!("{name}.{k}"), x);
        }
    }
    s
}

fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn walk(fields: &mut BTreeMap<String, Field>, path: &str, v: &Value) {
    let f = fields.entry(path.to_string()).or_default();
    if MAP_PATHS.contains(&path) {
        f.types.insert("map<str,str>".into());
        return;
    }
    f.types.insert(json_type(v).into());
    if ENUM_PATHS.contains(&path) {
        if let Value::String(s) = v {
            f.values.insert(s.clone());
        }
    }
    match v {
        Value::Array(xs) => {
            for x in xs {
                walk(fields, &format!("{path}[]"), x);
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                walk(fields, &format!("{path}.{k}"), x);
            }
        }
        _ => {}
    }
}

/// 帧形的规范文字（指纹就是它的摘要）。
pub fn canonical(s: &Shape) -> String {
    let mut out = String::new();
    for (name, fields) in s {
        out.push_str(name);
        out.push('\n');
        for (path, f) in fields {
            out.push_str(&format!(
                "  {path} {}{}\n",
                f.types.iter().cloned().collect::<Vec<_>>().join("|"),
                if f.values.is_empty() {
                    String::new()
                } else {
                    format!(
                        " {{{}}}",
                        f.values.iter().cloned().collect::<Vec<_>>().join(",")
                    )
                }
            ));
        }
    }
    out
}

/// FNV-1a 64 位摘要，十六进制 16 位。
pub fn digest(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn fingerprint_of(s: &Shape) -> String {
    digest(canonical(s).as_bytes())
}

/// 这一版的协议指纹。
pub fn proto() -> &'static str {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| fingerprint_of(&shape_of(&samples())))
}

/// FRAMES.md 的全文。
pub fn frames_md() -> String {
    let s = shape_of(&samples());
    let mut out = String::new();
    out.push_str("# 帧形一览\n\n");
    out.push_str("由 `devsched-wire` 的类型生成，别手改；改了帧之后重写：`DEVSCHED_WIRE_BLESS=1 cargo test -p devsched-wire --test frames_md`。\n\n");
    out.push_str(&format!("协议指纹：`{}`\n\n", proto()));
    out.push_str("一条连接（标准输入 / 输出）上一行一个 JSON 对象，`t` 是帧名。类型里 `T|null` 是可以不填的格。\n");
    for (name, dir, what) in FRAME_DOC {
        out.push_str(&format!("\n## {name}\n\n{dir}。{what}。\n\n"));
        let fields = s.get(*name);
        match fields {
            Some(fields) if !fields.is_empty() => {
                out.push_str("| 键 | 类型 | 说明 |\n|---|---|---|\n");
                for (path, f) in fields {
                    let key = path.strip_prefix(&format!("{name}.")).unwrap_or(path);
                    let mut ty = f.types.iter().cloned().collect::<Vec<_>>().join("\\|");
                    if !f.values.is_empty() {
                        ty.push_str(&format!(
                            "：{}",
                            f.values.iter().cloned().collect::<Vec<_>>().join(" · ")
                        ));
                    }
                    let note = if path.ends_with("[]") {
                        "每一项"
                    } else {
                        FIELD_DOC
                            .iter()
                            .find(|(p, _)| p == path)
                            .map(|(_, n)| *n)
                            .unwrap_or("")
                    };
                    out.push_str(&format!("| `{key}` | {ty} | {note} |\n"));
                }
            }
            _ => out.push_str("没有别的键。\n"),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_cover_exactly_the_shape() {
        let s = shape_of(&samples());
        let names: BTreeSet<&str> = s.keys().map(|k| k.as_str()).chain(["beat"]).collect();
        let documented: BTreeSet<&str> = FRAME_DOC.iter().map(|(n, _, _)| *n).collect();
        assert_eq!(names, documented);
        assert_eq!(documented, Frame::NAMES.iter().copied().collect());
        let paths: BTreeSet<&str> = s
            .values()
            .flat_map(|f| f.keys().map(|k| k.as_str()))
            .filter(|p| !p.ends_with("[]"))
            .collect();
        let doc_paths: BTreeSet<&str> = FIELD_DOC.iter().map(|(p, _)| *p).collect();
        assert_eq!(paths, doc_paths, "FIELD_DOC 跟帧形对不上");
    }

    #[test]
    fn enums_list_every_value() {
        let s = shape_of(&samples());
        assert_eq!(
            s["exit"]["exit.stop"].values.len(),
            crate::frame::StopReason::ALL.len()
        );
        assert_eq!(
            s["exit"]["exit.breach"].values.len(),
            crate::frame::Breach::ALL.len()
        );
        assert_eq!(s["out"]["out.stream"].values.len(), 2);
    }
}
