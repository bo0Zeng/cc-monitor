/**
 * S27（步 `23a`）：**断网门禁的 crate 缓存齐不齐** —— 装进 `npm test` 的那一半。
 *
 * # 它治的病
 *
 * 门禁断网跑（`.claude/devbox/gate` 默认 `--network none`）。缓存里缺一份 `.crate`，
 * `cargo` 退 **101**，终端上只剩一行 `GATE: FAIL —— cargo（退出码 101）`，
 * 而真正的话是「下不到包」—— **与代码对不对毫无关系的诊断**。
 * 本仓刚踩过同形的坑（一把尺子崩在 `substring not found`，而病在搬树）。
 * ⇒ 这条判据**先于** cargo 那一格说话，并且**点名**：谁缺了。
 *
 * # 真正的量具不在这个文件里
 *
 * 逻辑住 `tests/evidence/S27-offline-cache-check.py`（两个人群、地板、反空真都在那边），
 * 本文件只是把它挂进 `npm test` 并把「哪种环境该判、哪种判不了」这一格写死。
 * **刻意不在 TS 里重写一遍** —— 一个性质两把尺子是本仓最贵那族病
 * （`gate.sh` 头注逐字：「一个性质一把尺子」）。
 *
 * # 🔴 「判得了 / 判不了」这一格 —— 它是本文件最要紧的一段，别读快了
 *
 * 这条判据量的是**这台机器的 cargo 缓存**，不是仓库里的文本。
 * 于是它在三种环境里含义不同，**必须显式分开**（含糊就是下一条「安静地扫错东西」）：
 *
 * | 环境 | `cache_dirs` | 本仓 672 条命中 | 判 |
 * |---|---|---|---|
 * | 宿主 / 门禁沙箱 | 有 | 全部或几乎全部 | **判**：缺一条就红并点名 |
 * | 云端 CI（`ci.yml` 的 frontend job 跑在 windows-latest） | 可能有 | **0 条** | **判不了**：这台机器从没服务过本仓的断网构建 |
 * | 没装 cargo / 没有 python3 | 无 | — | **判不了** |
 *
 * 「判不了」那两格**不是绿**：它们会把理由原样打出来（`console.warn`），
 * 且**只认上面这两条具名理由** —— 出现第三种形状照样红。
 *
 * ## 诚实段：它挡不住什么
 *
 * - **整个缓存被清空** ⇒ 命中变 0 ⇒ 落进「判不了」⇒ **本条不红**。
 *   那一格由门禁的 `cargo` 格自己红（红得难看，但会红），
 *   以及 `tests/evidence/S27-prove-offline.py` 的 stage0 兜着。
 *   **这是这条判据已知的洞，写在这里而不是藏着。**
 * - **只证 `.crate` 文件在不在**，不证编得过、不证 checksum 对。
 *   「断网真的跑得动」那一格由 `S27-prove-offline.py`（`bwrap --unshare-net` 跑真 `cargo fetch`）买。
 * - **看的是宿主的种子缓存**，不是沙箱那个具名卷 `ccmon-cargo-registry`。
 *   卷是派生物（`.claude/devbox/gate` 从 `~/.cargo/registry` 播种），卷若比种子旧，本条看不见。
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const HERE = dirname(fileURLToPath(import.meta.url));
const METER = resolve(HERE, "../../evidence", "S27-offline-cache-check.py");

type Report = {
  cache_dirs: string[];
  p1_total: number;
  p1_missing: string[];
  p2_total: number;
  p2_missing: string[];
  structural_failures: string[];
  ok: boolean;
};

/** 跑量具；它在「缺包」时退 1（照样吐 JSON），所以非零退出码要接住而不是炸掉。 */
function runMeter(): { report: Report | null; why: string } {
  let raw: string;
  try {
    raw = execFileSync("python3", [METER, "--json"], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch (e) {
    const err = e as { stdout?: string; code?: string };
    if (typeof err.stdout === "string" && err.stdout.trim().startsWith("{")) raw = err.stdout;
    else return { report: null, why: `量具跑不起来（python3 缺席或脚本炸了）：${String(err.code ?? e)}` };
  }
  return { report: JSON.parse(raw) as Report, why: "" };
}

describe("S27 · 断网门禁的 crate 缓存", () => {
  it("量具本身在盘上（它是本条判据的全部内容，丢了就等于判据没了）", () => {
    expect(existsSync(METER)).toBe(true);
    expect(existsSync(join(HERE, "../../evidence", "S27-cache-manifest.md"))).toBe(true);
  });

  it("本机缓存里，今天那几份 lock ＋ `23a` 那 26 条，一条都不缺", () => {
    const { report, why } = runMeter();

    // ── 判不了 · 理由一：这台机器上没有 python3 / 量具跑不起来 ──────────
    if (report === null) {
      console.warn(`S27: 判不了 —— ${why}。**这不是绿**，只是这台机器上量不了。`);
      expect(why.length).toBeGreaterThan(0);
      return;
    }

    // ── 判不了 · 理由二：这台机器从没服务过本仓的断网构建 ────────────────
    const noCache = report.cache_dirs.length === 0;
    const foreign = report.p1_total > 0 && report.p1_missing.length === report.p1_total;
    if (noCache || foreign) {
      console.warn(
        `S27: 判不了 —— ${noCache ? "本机没有 cargo registry 缓存目录" : `本仓 ${report.p1_total} 条一条都不在本机缓存里（云端 CI 就是这一形）`}。` +
          "**这不是绿**；门禁沙箱与宿主上这一格是要判的。",
      );
      return;
    }

    // ── 判得了：从这里起，缺一条就红，并且点名 ────────────────────────────
    //
    // 🔴 反空真：人群自己先立住。量具那边有地板（P1 ≥ 500 · P2 == 26），
    //    立不住会落进 `structural_failures`，这里原样炸出来。
    expect(report.structural_failures, report.structural_failures.join(" / ")).toEqual([]);
    expect(report.p1_total).toBeGreaterThan(500);
    expect(report.p2_total).toBe(26);

    expect(
      report.p1_missing,
      `断网门禁今天就喂不饱：本机缓存缺这几份 .crate ⇒ ${report.p1_missing.join(" · ")}`,
    ).toEqual([]);
    expect(
      report.p2_missing,
      `\`russh-sftp 3.0.0\` 一落地门禁当场断网失败：缺 ⇒ ${report.p2_missing.join(" · ")}\n` +
        "补法：python3 tests/evidence/S27-prove-offline.py --prime（那一步要联网，它就是「喂缓存」本身）",
    ).toEqual([]);
  });
});
