//! `24e` 的语料：**采结构不采内容**。
//!
//! 参数取自／那趟现打（`$HOME` 下 640 413 条真路径的
//! 分布），**一个真路径都没用** —— 真路径带个人信息，而判据要的是结构。
//!
//! # 🔴 采样法：以**路径长**为主，再把长度预算分配到各段
//!
//! 记过第一版为什么错：**先独立采深度与段长、让路径长自然落下来**
//! ⇒ 路径长均值 145.6 / 目标 125.1，**偏 16%**，自检当场红。
//! 根因是真数据里**深度与段长负相关**（深路径段短，末尾常有一个长文件名，
//! 如 `node_modules/<长包名>/dist/index.js`），独立采样凑不齐。
//! ⇒ 本模块照订正后的办法来：**先采 `L`（路径长）与 `D`（深度），再把 `L` 分配到 `D` 段**。
//!
//! # ⚠ 自检的判法
//!
//! 「相对偏差 ≤ 15% **或** 绝对差 ≤ 1 个单位」，两者取宽。
//! 那一条**不是放宽容差凑绿**：一个整数值分布的中位，精度本来就不可能优于 ±1，
//! 而在「目标 = 6」上，**一个量化单位 ＝ 16.7% > 闸** ⇒ 比分辨率还细的闸必然误报。
//!
//! # 🔴 那条点名豁免，**本实现不需要，于是不留**
//!
//! `99` 给 `段长 p90` 开了唯一一项点名豁免（它那版合成 22 / 目标 27，偏 18.5%）。
//! 本模块的采样把余量摊到**1～2 段**（`synth_paths` 的 `heavy`）而不是死摊到末段，
//! 尾部因此够厚 —— 现打 `段长 p90 = 27.0`，**偏差 0.0%**。
//!
//! ⇒ **豁免删掉，这一项按正常项判。** 留着一个用不上的豁免，
//! 等于把一个真判得了的维度**永久挖瞎** —— 那正是本仓最贵那族病的形状。
//! ⚠ 哪天有人改了采样把这一项弄偏，它**应该**红；到那时要么修采样，
//! 要么拿出「为什么这一项不影响被测量」的论证再来谈豁免，**不许直接加回去**。
//!
//! 〔`99` 当初那条豁免的论证原样留档，因为它仍然正确：egui 把每一行当
//! **一整个字符串**排版、**不解析路径分段** ⇒ 段结构对「布局＋三角化的 CPU 成本」
//! 不可见；驱动成本的是**路径总长与字形数**。〕

use super::source::Row;

/// 确定性 PRNG（xorshift64*）。**刻意不引第三方 rand** —— 语料要可复算，
/// 而且这条路上多一个依赖就多一分 `winchk`/`muslbuild` 的账。
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// `[0, 1)` 的均匀数。
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// `[lo, hi]` 的均匀整数。
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        debug_assert!(hi >= lo);
        lo + (self.unit() * ((hi - lo + 1) as f64)) as usize % (hi - lo + 1)
    }

    /// 标准正态（Box–Muller，只取一支）。
    pub fn normal(&mut self) -> f64 {
        let u1 = self.unit().max(1e-12);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// 对数正态：`median * exp(sigma * N(0,1))`。
    pub fn lognormal(&mut self, median: f64, sigma: f64) -> f64 {
        median * (sigma * self.normal()).exp()
    }
}

/// 🔴 分布参数 —— 全部来自那张表的「现打目标」列。
pub mod target {
    /// 路径长：中位 / 均值 / p90
    pub const PATH_LEN: (f64, f64, f64) = (121.0, 125.1, 179.0);
    /// 深度：中位 / 均值 / p90
    pub const DEPTH: (f64, f64, f64) = (10.0, 10.3, 13.0);
    /// 段长：中位 / 均值 / p90
    pub const SEG_LEN: (f64, f64, f64) = (6.0, 11.1, 27.0);
}

/// 由 `median` 与 `mean` 反解对数正态的 sigma：`mean = median * exp(sigma²/2)`。
fn sigma_from(median: f64, mean: f64) -> f64 {
    (2.0 * (mean / median).ln()).sqrt()
}

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789-_";

/// 生成 `n` 条合成路径。
pub fn synth_paths(n: usize, seed: u64) -> Vec<String> {
    let mut rng = Rng::new(seed);
    let len_sigma = sigma_from(target::PATH_LEN.0, target::PATH_LEN.1);
    let depth_sigma = sigma_from(target::DEPTH.0, target::DEPTH.1);
    let mut out = Vec::with_capacity(n);
    let mut buf = String::with_capacity(256);

    for _ in 0..n {
        // ① 路径长 L 与深度 D —— 先定这两个，别让它们自己落下来。
        let l = rng
            .lognormal(target::PATH_LEN.0, len_sigma)
            .round()
            .clamp(8.0, 4096.0) as usize;
        let d = rng
            .lognormal(target::DEPTH.0, depth_sigma)
            .round()
            .clamp(1.0, 64.0) as usize;

        // ② 长度预算：去掉 D 个分隔符。
        let budget = l.saturating_sub(d).max(d); // 每段至少 1 个字符

        // ③ 每段先拿一个「短基数」—— 中位就是它定的。
        let mut segs: Vec<usize> = (0..d).map(|_| rng.range(2, 9)).collect();
        let base: usize = segs.iter().sum();

        if budget > base {
            // ④ 余量堆到**少数几段**上 —— 这就是真数据里那个「末尾一个长文件名」
            //    与「node_modules/<长包名>」的形状。堆几段决定了段长的尾部厚度。
            let mut rest = budget - base;
            let heavy = if rng.unit() < 0.55 { 2 } else { 1 };
            for k in 0..heavy {
                let take = if k + 1 == heavy {
                    rest
                } else {
                    (rest as f64 * (0.35 + 0.30 * rng.unit())) as usize
                };
                // 最后一段（文件名）优先，其次随机一段中间目录。
                let idx = if k == 0 { d - 1 } else { rng.range(0, d - 1) };
                segs[idx] += take;
                rest -= take;
                if rest == 0 {
                    break;
                }
            }
        } else {
            // 预算比基数还小：按比例压回去，最短 1。
            let scale = budget as f64 / base as f64;
            for s in segs.iter_mut() {
                *s = ((*s as f64) * scale).round().max(1.0) as usize;
            }
        }

        // ⑤ 铺字节。内容全合成。
        buf.clear();
        for s in &segs {
            buf.push('/');
            for _ in 0..*s {
                let c = ALPHABET[(rng.next_u64() % ALPHABET.len() as u64) as usize];
                buf.push(c as char);
            }
        }
        out.push(buf.clone());
    }
    out
}

/// 合成 `n` 行列表数据。约 1/8 是目录 —— 只影响画什么，不影响长度分布。
///
/// 🔴〔补齐五项 2026-09-23〕**链接与时间两格也合成，理由是「尺子要量今天的靶子」。**
/// 行上多了两样要画的东西（`🔗` 那个标记与那一列修改时间）⇒ 若这份语料
/// 两格全空，`scale` 那一族的帧时读数量的就是一个**已经不在盘上的界面**。
///
/// ⚠ 两格**都从路径算，一格 rng 都不多抽** —— 那是刻意的：`rng` 的序一动，
/// 合成出来的路径就全变了，而 `corpus_tests` 那条分布判据与
/// `source_tests` 那条「默认序与基线逐字节相同」的指纹都钉在这份语料上。
pub fn synth_rows(n: usize, seed: u64) -> Vec<super::source::Listed> {
    let paths = synth_paths(n, seed);
    let mut rng = Rng::new(seed ^ 0x5DEE_CE66);
    paths
        .into_iter()
        .map(|p| {
            let name = p.rsplit('/').next().unwrap_or("").to_string();
            let is_dir = rng.next_u64() % 8 == 0;
            // 从路径本身派生（不抽 rng）：约 1/17 是链接，时间摊在约 30 年的跨度上。
            let h = p
                .bytes()
                .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(u64::from(b)));
            super::source::Listed {
                link: h % 17 == 0,
                link_dir: false,
                mtime_secs: Some(1_000_000_000 + h % 900_000_000),
                mtime_text: Some("10-02".to_string()),
                mtime_full: Some("2026-10-02 15:01:23".to_string()),
                raw_name: None,
                link_broken: false,
                row: Row {
                    name,
                    path: p,
                    is_dir,
                    size: if is_dir {
                        0
                    } else {
                        rng.next_u64() % (1 << 26)
                    },
                    lossy_name: false,
                },
            }
        })
        .collect()
}

/// 一项的三个数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stat {
    pub median: f64,
    pub mean: f64,
    pub p90: f64,
}

fn stat(v: &mut [f64]) -> Stat {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    Stat {
        median: v[n / 2],
        mean: v.iter().sum::<f64>() / n as f64,
        p90: v[(n as f64 * 0.9) as usize % n],
    }
}

/// 一趟语料的分布读数。
#[derive(Clone, Copy, Debug)]
pub struct Report {
    pub path_len: Stat,
    pub depth: Stat,
    pub seg_len: Stat,
}

pub fn measure(paths: &[String]) -> Report {
    let mut lens: Vec<f64> = paths.iter().map(|p| p.len() as f64).collect();
    let mut depths: Vec<f64> = paths
        .iter()
        .map(|p| p.matches('/').count() as f64)
        .collect();
    let mut segs: Vec<f64> = paths
        .iter()
        .flat_map(|p| {
            p.split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.len() as f64)
        })
        .collect();
    Report {
        path_len: stat(&mut lens),
        depth: stat(&mut depths),
        seg_len: stat(&mut segs),
    }
}

/// 判法：相对偏差 ≤ 15% **或** 绝对差 ≤ 1 个单位，取宽。
pub fn within_tolerance(got: f64, want: f64) -> bool {
    let rel = ((got - want) / want).abs();
    rel <= 0.15 || (got - want).abs() <= 1.0
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/corpus_tests.rs"]
mod tests;
