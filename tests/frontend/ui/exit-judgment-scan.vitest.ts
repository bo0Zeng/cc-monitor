/**
 * **出口零判定零格式化**的扫描判据（`src/doc/ARCHITECTURE.md` §2.9）：按意思判的两句（[`exit-rules.ts`]）——
 * 出口取的文案键只许骨架那几档且不带插值（`key`）；条件读了线上值的分支里不许取文（`branch`）。
 *
 * 扫哪几棵树：欠账名单 `tests/frontend/ui/exit-debt.json` 的 `roots`，照分层表（§2.9 的「输出适配层」那一行：
 * 界面 · 壳 · 文件窗口 · 手机端）各登一棵。三门语言共用这一份名单，各判各的格：
 * - TS（`src/frontend/ui`）在这里判；
 * - Rust（壳 · 文件窗口）在 `tests/frontend/filewin/exit_scan_tests.rs`；
 * - Kotlin（`src/mobile`）是 `src/mobile/detekt-rules` 里的 detekt 规则（`ExitCopyKey` · `ExitCopyBranch`）。
 * 名单自身与合计在这里判。存量只许删、不许加：
 * 1. TS 每一格（文件 × 句）实得 == 名单登的数：多一处 ⇒ 红；少了 ⇒ 也红（删了就把那一格改小或删掉）；名单外的文件命中一处 ⇒ 红。
 * 2. 合计 == `ceiling`；`ceiling` 只许往下改（本文件钉着 [`CEILING_AT_MOST`]，比它大 ⇒ 红）。
 * 3. 名单自身：每格 `n > 0`、`why` 非空、`kind` 在闭集里、句名认得、文件在盘上、没有重复格；`roots` 里每棵树都扫到了文件（反空真）。
 * 4. 对照样例：每一段两句各命中的数恰好是登的数（探测器坏了 ⇒ 先红）。
 *
 * 换成这两句那一趟（替掉三门语言各一套的写法正则）是唯一一次「加」：第一次跑看见的全登进名单（`kind: "首登"`），之后同样只许删。
 * 什么时候删：名单数到 0 ⇒ 删名单与只判名单的几条，主断言改成零命中。
 */
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { productionKtFiles, productionRsFiles, productionTsFiles } from "../../test-support/production-sources.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { RULE_IDS, SAMPLE_TABLE, TS_SAMPLES, scanTs, type CopyTable } from "./exit-rules.ts";

/** 换成两句那一趟收完时的合计（`ceiling` 只许往下改；改小了这里一起改小）。
 * 上调过一次（+3）：本机后端没起来那一格的词 · 句 · ［重试］从契约 crate 的 `local()`（不在扫描面）搬进壳自己那一格
 * （`local_backend_host.rs::local_backend`，`LocalBackend`），是搬家不是新增文案，全仓取字总数没变。
 * 又上调过一次（+86）：`branch` 那一句跟进同文件里取文的小函数，原来看不见的存量现形，不是新增。
 * 再上调一次（+3）：手机标签页重名编号与按键时长那三处原是出口拼的字面量（扫描器看不见），改成取带插值的键后现形，不是新增。 */
const CEILING_AT_MOST = 1848;

type Lang = "ts" | "rs" | "kt";

/** 名单按 kind 分组，一格是 `[文件, 句, 命中数]`。 */
interface Debt {
  roots: { dir: string; lang: Lang }[];
  ceiling: number;
  groups: { kind: string; why: string; cells: [string, string, number][] }[];
}

/** 首登 ＝ 换成两句那一趟第一次跑看见的存量，还没逐格分归哪一批；留 ＝ 不是违例（why 写凭什么）。 */
const KINDS = new Set(["首登", "留"]);

const debt = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/frontend/ui/exit-debt.json"), "utf8")) as Debt;
const entries = debt.groups.flatMap((g) => g.cells.map(([file, rule, n]) => ({ file, rule, n, kind: g.kind, why: g.why })));
const table = (JSON.parse(readFileSync(resolve(REPO_ROOT, "src/shared/copy/table.json"), "utf8")) as { entries: CopyTable }).entries;

function sourcesOf(dir: string, lang: Lang) {
  return { ts: productionTsFiles, rs: productionRsFiles, kt: productionKtFiles }[lang](dir);
}

describe("出口零判定零格式化：按意思判的两句 ＋ 只减不增的欠账名单", () => {
  it("★ TS 每一格实得 == 名单登的数（多一处红 · 删了不改名单也红 · 名单外的命中红）", () => {
    const got = new Map<string, number>();
    for (const { dir, lang } of debt.roots) {
      if (lang !== "ts") continue;
      for (const s of sourcesOf(dir, lang)) {
        const n = scanTs(s.text, table, s.file);
        for (const r of RULE_IDS) if (n[r] > 0) got.set(`${s.file} · ${r}`, n[r]);
      }
    }
    const tsRoots = debt.roots.filter((r) => r.lang === "ts").map((r) => `${r.dir}/`);
    const want = new Map(entries.filter((e) => tsRoots.some((d) => e.file.startsWith(d))).map((e) => [`${e.file} · ${e.rule}`, e.n]));
    const wrong: string[] = [];
    for (const [k, n] of got) {
      const w = want.get(k);
      if (w === undefined) wrong.push(`${k}：命中 ${n} 处，名单里没有 —— 出口新取了描述数据的字（交核心写好那一格，出口照摆）`);
      else if (w !== n) wrong.push(`${k}：命中 ${n} 处，名单登 ${w}${n < w ? " —— 收掉了就把名单那一格改成实数" : " —— 多出来的那几处交核心写"}`);
    }
    for (const [k, w] of want) if (!got.has(k)) wrong.push(`${k}：一处都没有了，名单还登 ${w} —— 删掉那一格`);
    expect(wrong, wrong.join("\n")).toEqual([]);
  });

  it("★ 合计 == ceiling，ceiling 只许往下改", () => {
    const sum = entries.reduce((a, e) => a + e.n, 0);
    expect(sum, "名单各格之和与 ceiling 不一致 —— 收掉几处就把 ceiling 降到现数").toBe(debt.ceiling);
    expect(debt.ceiling, `ceiling ${debt.ceiling} 比上一批收完时的 ${CEILING_AT_MOST} 大 —— 名单只许删`).toBeLessThanOrEqual(CEILING_AT_MOST);
    expect(debt.ceiling, `ceiling 降到了 ${debt.ceiling} ⇒ 本文件的 CEILING_AT_MOST 一起改小`).toBe(CEILING_AT_MOST);
  });

  it("名单自身：每棵树都扫到文件 · 每格 n > 0 · kind 在闭集里 · why 非空 · 句名认得 · 文件在盘上 · 没有重复格", () => {
    const bad: string[] = [];
    for (const { dir, lang } of debt.roots) {
      const n = sourcesOf(dir, lang).length;
      if (n <= 5) bad.push(`${dir}（${lang}）只扫到 ${n} 份生产文件 —— 遍历坏了，那一棵恒绿`);
    }
    const seen = new Set<string>();
    for (const e of entries) {
      const k = `${e.file} · ${e.rule}`;
      if (!debt.roots.some((r) => e.file.startsWith(`${r.dir}/`))) bad.push(`${k}：不在任何一棵 roots 底下`);
      if (!existsSync(resolve(REPO_ROOT, e.file))) bad.push(`${k}：文件不在了 —— 删掉那一格`);
      if (!(RULE_IDS as readonly string[]).includes(e.rule)) bad.push(`${k}：没有这一句`);
      if (seen.has(k)) bad.push(`${k}：重复`);
      seen.add(k);
      if (!(Number.isInteger(e.n) && e.n > 0)) bad.push(`${k}：n 要是正整数`);
      if (!KINDS.has(e.kind)) bad.push(`${k}：kind「${e.kind}」不在闭集 ${[...KINDS].join(" / ")}`);
      if (e.why.trim() === "") bad.push(`${k}：why 空着`);
    }
    // 手机那一半只认这份名单：detekt 的 baseline 里压着这两条，名单就被绕过去了。
    //（baseline 一个模块一份，带类型解析的变体另一份：`baseline-<模块>[-debug|-main].xml`，模块照 settings.gradle.kts 的 include）
    const modules = [...readFileSync(resolve(REPO_ROOT, "src/mobile/settings.gradle.kts"), "utf8").matchAll(/^include\(":([\w-]+)"\)/gm)].map((m) => m[1]!);
    expect(modules.length, "settings.gradle.kts 里一个模块都没读到 —— 下面恒绿").toBeGreaterThan(5);
    for (const m of modules) {
      for (const f of [`baseline-${m}.xml`, `baseline-${m}-debug.xml`, `baseline-${m}-main.xml`]) {
        const p = resolve(REPO_ROOT, "src/mobile/config/detekt", f);
        if (existsSync(p) && /ExitCopy(Key|Branch)/.test(readFileSync(p, "utf8"))) bad.push(`src/mobile/config/detekt/${f}：baseline 里压着出口扫描判据 —— 删掉那几行，照名单登`);
      }
    }
    expect(bad, bad.join("\n")).toEqual([]);
  });

  it("对照样例：每一段两句各命中的数恰好是登的数", () => {
    const bad: string[] = [];
    for (const s of TS_SAMPLES) {
      const got = scanTs(s.code, SAMPLE_TABLE);
      for (const r of RULE_IDS) if (got[r] !== s.want[r]) bad.push(`${r}：「${s.code}」命中 ${got[r]}，该是 ${s.want[r]}`);
    }
    expect(bad, bad.join("\n")).toEqual([]);
  });
});
