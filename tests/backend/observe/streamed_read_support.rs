//! d（整份读按块）几份判据共用的两样：合成大记录 · 读的时候本线程手里最多能有多少（各判据自己 `reset_peak` / `peak_since`，量具的单线程自检按调用点找测试属性）。

use std::io::Write as _;
use std::path::Path;

/// 读一份大记录时本线程「已分配未释放」的上界：两块（读进来的那块 ＋ 交出去还没放的那块）＋ 一行（夹具每行 ≤ 2 KiB，
/// 解析成 `Value` 的开销按百倍余量算）。今天整份读进一个 `Vec` ⇒ 至少是文件大小。
pub(crate) const HELD_BOUND: usize = 2 * (1 << 20) + (256 << 10);

/// 夹具大小：比上界大三倍多，整份读一定越线。
pub(crate) const BIG: usize = 8 << 20;

/// 合成一份约 `size` 字节的大记录（全是占位字）。填充行大多不进任何成品：`blank_only` ⇒ 只有纯空白行（不占行号、不出帧）；
/// 否则空白行与进度行轮着来（进度行要解析，但不进索引 / 清单 / 运行簿）。每 1000 行夹一句用户话（`needle <i>`）。回夹了几句。
pub(crate) fn big_record(path: &Path, size: usize, blank_only: bool) -> usize {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("建夹具"));
    let pad = "x".repeat(1800);
    let (mut n, mut users, mut i) = (0usize, 0usize, 0usize);
    while n < size {
        let line = if i % 1000 == 999 {
            users += 1;
            serde_json::json!({"type":"user","uuid":format!("u-{i}"),"timestamp":"2026-04-01T00:00:00Z",
                "message":{"role":"user","content":format!("needle {i}")}})
            .to_string()
        } else if blank_only || i % 2 == 0 {
            " ".repeat(1800)
        } else {
            serde_json::json!({"type":"progress","uuid":format!("p-{i}"),"data":pad}).to_string()
        };
        f.write_all(line.as_bytes())
            .and_then(|_| f.write_all(b"\n"))
            .expect("写夹具");
        n += line.len() + 1;
        i += 1;
    }
    f.flush().expect("写夹具");
    users
}
