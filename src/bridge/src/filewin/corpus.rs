//! `24e` 的语料：**采结构不采内容**。
//!
//! 参数取自 `真相源/98 §3.3` ／ `真相源/99 §四` 那趟现打（`$HOME` 下 640 413 条真路径的
//! 分布），**一个真路径都没用** —— 真路径带个人信息，而判据要的是结构。
//!
//! # 🔴 采样法：以**路径长**为主，再把长度预算分配到各段
//!
//! `真相源/99 §4.1` 逐字记过第一版为什么错：**先独立采深度与段长、让路径长自然落下来**
//! ⇒ 路径长均值 145.6 / 目标 125.1，**偏 16%**，自检当场红。
//! 根因是真数据里**深度与段长负相关**（深路径段短，末尾常有一个长文件名，
//! 如 `node_modules/<长包名>/dist/index.js`），独立采样凑不齐。
//! ⇒ 本模块照订正后的办法来：**先采 `L`（路径长）与 `D`（深度），再把 `L` 分配到 `D` 段**。
//!
//! # ⚠ 自检的判法（`真相源/99 §4.2`）
//!
//! 「相对偏差 ≤ 15% **或** 绝对差 ≤ 1 个单位」，两者取宽。
//! 那一条**不是放宽容差凑绿**：一个整数值分布的中位，精度本来就不可能优于 ±1，
//! 而在「目标 = 6」上，**一个量化单位 ＝ 16.7% > 闸** ⇒ 比分辨率还细的闸必然误报。
//!
//! # 🔴 `真相源/99 §4.3` 那条点名豁免，**本实现不需要，于是不留**
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

/// 🔴 分布参数 —— 全部来自 `真相源/99 §四` 那张表的「现打目标」列。
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
pub fn synth_rows(n: usize, seed: u64) -> Vec<Row> {
    let paths = synth_paths(n, seed);
    let mut rng = Rng::new(seed ^ 0x5DEE_CE66);
    paths
        .into_iter()
        .map(|p| {
            let name = p.rsplit('/').next().unwrap_or("").to_string();
            let is_dir = rng.next_u64() % 8 == 0;
            Row {
                name,
                path: p,
                is_dir,
                size: if is_dir {
                    0
                } else {
                    rng.next_u64() % (1 << 26)
                },
                lossy_name: false,
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

/// `真相源/99 §4.2` 的判法：相对偏差 ≤ 15% **或** 绝对差 ≤ 1 个单位，取宽。
pub fn within_tolerance(got: f64, want: f64) -> bool {
    let rel = ((got - want) / want).abs();
    rel <= 0.15 || (got - want).abs() <= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 **语料分布自检 —— 这条会红，而且它红过就要停下来看。**
    ///
    /// `真相源/99 §4.1`／`§4.2` 记过它两次真红（一次逮住生成器偏 16%，
    /// 一次逮住闸自己比量具的分辨率还细）。**两次都红对了。**
    ///
    /// **九项全判，一项都不豁免**（`99` 那条 `段长 p90` 的豁免本实现用不上，
    /// 理由见模块头注）。九个数每趟都印出来。
    #[test]
    fn the_synthetic_corpus_matches_the_measured_distribution() {
        let paths = synth_paths(200_000, 0xC0FFEE);
        let r = measure(&paths);

        let checks: [(&str, f64, f64); 9] = [
            ("路径长 中位", r.path_len.median, target::PATH_LEN.0),
            ("路径长 均值", r.path_len.mean, target::PATH_LEN.1),
            ("路径长 p90", r.path_len.p90, target::PATH_LEN.2),
            ("深度 中位", r.depth.median, target::DEPTH.0),
            ("深度 均值", r.depth.mean, target::DEPTH.1),
            ("深度 p90", r.depth.p90, target::DEPTH.2),
            ("段长 中位", r.seg_len.median, target::SEG_LEN.0),
            ("段长 均值", r.seg_len.mean, target::SEG_LEN.1),
            ("段长 p90", r.seg_len.p90, target::SEG_LEN.2),
        ];

        let mut bad: Vec<String> = Vec::new();
        for (name, got, want) in checks {
            let rel = ((got - want) / want).abs() * 100.0;
            let ok = within_tolerance(got, want);
            println!(
                "  {name:<12} 合成 {got:>7.1}  目标 {want:>7.1}  偏差 {rel:>5.1}%  {}",
                if ok { "ok" } else { "🔴 RED" }
            );
            if !ok {
                bad.push(format!(
                    "{name}: 合成 {got:.1} / 目标 {want:.1}（偏 {rel:.1}%）"
                ));
            }
        }
        assert_eq!(
            checks.len(),
            9,
            "九项一项都不许少 —— 少一项就是挖瞎一个维度"
        );
        assert!(
            bad.is_empty(),
            "语料分布对不上现打目标 —— 本次读数作废：\n  {}",
            bad.join("\n  ")
        );
    }

    /// 反空真：自检那把尺子**认得出坏语料**。
    /// 把生成器换成「所有路径一样长」，上面九项里必须有东西红。
    #[test]
    fn the_distribution_self_check_rejects_a_degenerate_corpus() {
        let degenerate: Vec<String> = (0..10_000).map(|i| format!("/a/b/{i:03}")).collect();
        let r = measure(&degenerate);
        let any_red = !within_tolerance(r.path_len.median, target::PATH_LEN.0)
            || !within_tolerance(r.depth.median, target::DEPTH.0)
            || !within_tolerance(r.seg_len.mean, target::SEG_LEN.1);
        assert!(any_red, "一份明显不像真分布的语料竟然全过了 —— 自检是空的");
    }

    #[test]
    fn the_generator_is_deterministic_for_a_given_seed() {
        let a = synth_paths(500, 42);
        let b = synth_paths(500, 42);
        let c = synth_paths(500, 43);
        assert_eq!(a, b, "同一个 seed 必须复算出同一份语料");
        assert_ne!(a, c, "不同 seed 不该产出同一份语料");
    }

    #[test]
    fn tolerance_takes_the_wider_of_the_two_rules() {
        // 差一个量化单位，相对偏差 16.7% > 15% —— 按绝对差那一支放行（§4.2）。
        assert!(within_tolerance(5.0, 6.0));
        // 差 5 个单位、18.5% —— 两支都不放行（§4.3 那一项靠点名豁免，不靠这条）。
        assert!(!within_tolerance(22.0, 27.0));
        // 大数上的 10% 照旧放行。
        assert!(within_tolerance(110.0, 121.0));
        // 大数上的 30% 不放行。
        assert!(!within_tolerance(85.0, 121.0));
    }
}
