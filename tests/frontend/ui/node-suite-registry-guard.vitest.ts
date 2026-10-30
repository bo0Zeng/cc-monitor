/**
 * 前端测试的几样跑法前提：`npm test` 只有 vitest 一套 · 覆盖率那两步真在执行 · `.mjs` 的最小闸门。
 */
import { execFile } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { nonEmpty } from "../../test-support/non-empty.ts";
import { filePaths } from "../../test-support/production-sources.ts";

// 手写跑器（自定义 `test()` ＋ `failed` 计数 ＋ 收尾 throw，由 tsx 跑）与 vitest 是两套跑法：
// 那一套删光测试或删掉收尾都退 0，`npm` 那一格也数不到它跑了几条 ⇒ 只许 vitest 一套。
describe("前端测试只有 vitest 一套跑法", () => {
  it("仓里零份 `*.test.ts`（`vitest.config.ts` 只收 `*.vitest.ts`，别的名字没人跑）", () => {
    // 正控：遍历塌了 ⇒ 下面零命中地绿。
    nonEmpty(filePaths("tests", { ext: ".vitest.ts", tests: true }));
    const stray = filePaths(".", { ext: ".test.ts", tests: true, generated: true, allowEmpty: true });
    expect(stray, "这些文件不归 vitest 收，要改名 `*.vitest.ts`、用 vitest 的 test/expect").toEqual([]);
  });

  it("`package.json` 的 `test` 就是 `vitest run`，没有脚本用 tsx 跑测试文件", () => {
    const scripts = (
      JSON.parse(readFileSync(resolve(REPO_ROOT, "package.json"), "utf8")) as { scripts: Record<string, string> }
    ).scripts;
    expect(scripts["test"]).toBe("vitest run");
    const tsx = Object.entries(scripts)
      .filter(([, cmd]) => /(^|\s)(npx\s+)?tsx\s+(--\S+\s+)*\S+\.(test|vitest)\.ts\b/.test(cmd))
      .map(([name]) => name);
    expect(tsx, "这些脚本用 tsx 跑测试文件：另一套跑法").toEqual([]);
  });
});

// ★★ **CI 里那两步的「有效性」压在两个 npm 脚本的内容上**
// 〔audit-0805 08-08，Phase G 第 67 件〕。
//
// `ci.yml` 有两步：`coverage floor (vitest jsdom)` → `npm run coverage`，
// 随后 `coverage per-file floors + zero-coverage ratchet` → 读覆盖率产物。
// `shared_crate_registry` 已经钉住「这两步在 CI 里还在」。
//
// **但没人读 `coverage` 脚本本身**。08-08 实测：把它从 `vitest run --coverage`
// 改成 `vitest run`，**monitor 993 + vitest 1290 全绿** ——
// 而 `vitest.config.ts` 里那组 `thresholds` **只在 `--coverage` 时生效**，
// 下一步要读的覆盖率产物也不会生成。步骤名还在、CI 照旧绿，门槛整个不再执行。
//
// ⇒ 与 F58/F61 同族（散文/步骤名指着一件其实没在发生的事），这次载体是 npm 脚本。
describe("覆盖率那两步的有效性", () => {
  it("`npm run coverage` 必须真的带 --coverage", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    const pkg = JSON.parse(readFileSync(resolve(ROOT, "package.json"), "utf8"));
    const script = pkg.scripts?.coverage;
    expect(script, "`package.json` 里没有 `coverage` 脚本 —— 抽取坏了或它被删了").toBeTypeOf(
      "string",
    );
    expect(
      script,
      `\`coverage\` 脚本是 ${JSON.stringify(script)}，没带 --coverage。\n` +
        "⚠ `vitest.config.ts` 里那组 thresholds **只在 --coverage 时生效**，\n" +
        "而 CI 下一步 `assert-coverage-floors.mjs` 要读的覆盖率产物也不会生成。\n" +
        "结果是：CI 那两步照跑、照绿，而覆盖率门槛整个不再执行。",
    ).toContain("--coverage");
  });

  it("阈值确实写在 vitest 配置里（否则上面那条守的是一件不存在的事）", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    const cfg = readFileSync(resolve(ROOT, "vitest.config.ts"), "utf8");
    // ⚠ 要认的是**配置键**，不是「这个词出现过」：同一文件的注释里就写着
    //   「设地板阈值（下方 thresholds）」，只查词的话把 key 改名照样绿 ——
    //   变异当场证伪（F24 那一族，本会话第 N 次）。
    const keyed = cfg.split("\n").some((l) => /^\s*thresholds\s*:/.test(l));
    expect(
      keyed,
      "`vitest.config.ts` 里找不到 `thresholds:` 这个**配置键**（注释里提到不算）—— \n" +
        "那么 --coverage 也不会让谁红，上面那条就是在守一件不存在的事（本条此刻无效）。",
    ).toBe(true);
  });
});

// ★★ **覆盖率地板的「只许降」纪律本身是散文**〔audit-0805 08-08，Phase G 第 68 件〕。
//
// `tests/scripts/assert-coverage-floors.mjs` 是一条递减棘轮：逐文件地板 + 0% 文件数上限，
// 头注逐字写着「**只许降**」「地板设在**当前值下方 ~5 点**」「实测值一起写下，
// 只改数字不写实测，下一个人看不出它过期没过期」。
//
// 那三句撑着整条棘轮，而**没人读它们**。08-08 实测：把 `src/frontend/ui/tabs.ts` 的地板从
// `64/54` 调到 `10/5`（实测那两列原样不动），**monitor 993 + vitest 1292 全绿** ——
// 棘轮从此形同虚设，而它自己的诊断照旧说「地板通过」。
//
// ⇒ 钉的是**表自身的自洽**：既然每行都带着「写下时实测%」，就要求地板不许离它太远。
// 这不需要跑覆盖率，纯读表 —— 与那条真的跑覆盖率的门禁互补（一条在 CI 里跑、
// 一条在单测里读，失效模式不同）。
describe("覆盖率地板表的自洽", () => {
  // ★ 棘轮的**另一半**：0% 文件数上限。08-08 实测把 `ZERO_COUNT_CEILING` 从 13 抬到 40
  // （「只许降」那段历史注释一字不改），**vitest 1293 + monitor 15 套全绿** —— 一个数字
  // 的静默编辑就能把整条棘轮松掉一倍多。
  //
  // 这一半没法像地板那样对着「写下时实测」判（0% 文件数要跑完覆盖率才知道）。
  // 能钉的是**它与自己那段递减记录的关系**：注释里逐格记着 `17→16`、`16→…→14→13`，
  // 而常数是 13。要求「常数 = 记录的最后一格」，于是抬上限**必须同时把这一格写进记录** ——
  // 一次静默的数字编辑就变成一次要过 review 的显式改写。这与地板那条同一个手法：
  // **让表自己带着的信息去判表**。
  it("0% 文件数上限等于那段递减记录的最后一格，且记录本身不回头", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    const src = readFileSync(resolve(ROOT, "tests/scripts/assert-coverage-floors.mjs"), "utf8");

    const decl = /const ZERO_COUNT_CEILING = (\d+);/.exec(src);
    expect(decl, "`tests/scripts/assert-coverage-floors.mjs` 里找不到 `ZERO_COUNT_CEILING` 的声明了").not.toBeNull();
    const ceiling = Number(decl![1]);

    // 递减记录 = 声明**紧上方**那段注释里的箭头链（`17→16`、`16→…→14→13`）。
    // ⚠ 区域必须收到 `ZERO_TODAY` 的 `];` 之后：整个文件里还有别的箭头
    //（`PER_FILE_FLOORS` 那句「53.33 → 84.44」是覆盖率提升，不是棘轮记录）——
    // 08-08 第一版取「声明之前的全部」，就把它扫了进来，诊断说「33 → 84 回头了」。
    const record = src.slice(src.lastIndexOf("];", decl!.index), decl!.index);
    const chains = [...record.matchAll(/(?:\d+|…)(?:\s*→\s*(?:\d+|…))+/g)].map((m) => m[0]);
    const steps = chains.flatMap((c) => [...c.matchAll(/\d+/g)].map((m) => Number(m[0])));
    nonEmpty(steps);

    for (let i = 1; i < steps.length; i++) {
      expect(
        steps[i],
        `递减记录回头了：${steps[i - 1]} → ${steps[i]}。棘轮的话逐字是「**只许降**」。`,
      ).toBeLessThanOrEqual(steps[i - 1]);
    }
    expect(
      ceiling,
      `\`ZERO_COUNT_CEILING\` 是 ${ceiling}，而那段递减记录停在 ${steps[steps.length - 1]}。\n` +
        "⚠ 08-08 实测：把上限从 13 静默抬到 40，两侧门禁一声不吭全绿。\n" +
        "⇒ 改这个数**必须同时把新的一格写进上面那段记录**（`…→13→12` 这样），\n" +
        "  抬高更要写清为什么 —— 不然下一个人看不出这条棘轮是被棘紧的还是被松开的。",
    ).toBe(steps[steps.length - 1]);
  });

  it("每行地板都在「写下时实测」下方 ~5 点以内，且不高于实测", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    const src = readFileSync(resolve(ROOT, "tests/scripts/assert-coverage-floors.mjs"), "utf8");
    // 人群 = 表里每一行 `["<路径>", 语句地板, 实测, 分支地板, 实测],`
    const rows = [...src.matchAll(/\[\s*"([^"]+)"\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*\]/g)];
    nonEmpty(rows);
    const SLACK = 8; // 纪律写的是 ~5 点，留一点余量给 v8 版本差
    for (const [, file, sFloor, sNow, bFloor, bNow] of rows) {
      const [sf, sn, bf, bn] = [sFloor, sNow, bFloor, bNow].map(Number);
      expect(
        sf,
        `${file}: 语句地板 ${sf} 高于写下时实测 ${sn} —— 那不是地板，是天花板`,
      ).toBeLessThanOrEqual(sn);
      expect(
        sn - sf,
        `${file}: 语句地板 ${sf} 离实测 ${sn} 差了 ${(sn - sf).toFixed(1)} 点（纪律是 ~5）。\n` +
          "⚠ 头注逐字写着「只许降」——余量一放大，棘轮就形同虚设而诊断照旧说「通过」。\n" +
          "真要放宽：先说清为什么，并把实测那一列一起更新（不然下一个人看不出它过期没过期）。",
      ).toBeLessThanOrEqual(SLACK);
      expect(bf, `${file}: 分支地板 ${bf} 高于写下时实测 ${bn}`).toBeLessThanOrEqual(bn);
      expect(
        bn - bf,
        `${file}: 分支地板 ${bf} 离实测 ${bn} 差了 ${(bn - bf).toFixed(1)} 点（纪律是 ~5）`,
      ).toBeLessThanOrEqual(SLACK);
    }
  });
});

// ★★ **`.mjs` 这一类今天零强制检查**〔audit-0805 08-08，Phase G 第 84 件〕。
//
// 08-08 沿「扫描面之外还有谁」量到最后一格。**先核先纠正了我自己上一轮记的话**：
// `.mts` 其实**在 tsc 范围内**（`tsconfig.json` 的 `include` 是 `["src", "e2e"]`，
// 实测往 `tests/e2e/` 下的 `.mts` 塞一个类型错，`tsc --noEmit` 当场报 TS2322）。
//
// 而 `.mjs` **不在**：`allowJs` 没开 ⇒ tsc 整类不看。实测把
// `tests/scripts/assert-coverage-floors.mjs` 结尾塞一个不闭合的对象字面量，
// **tsc 绿、vitest 1294 全绿** —— 而那个文件是 CI 覆盖率门禁的执行体。
//
// ⚠ 「CI 会跑它、跑挂了就知道」这句话在本仓**不成立**：〔用 08-05〕裁定不再 push，
// CI 结构上不会跑（`shared_crate_registry` 有一条判据专门钉这个前提）。
// 而 `tests/e2e/tier2/*.mjs` 那两个连 CI 都不跑（tier2 是 Windows VM 上手跑的）。
//
// ⇒ 补一条**本地**判据：`node --check` 是语法层的最小闸门（与 CI 里那条
// `python3 -m py_compile tests/e2e/*.py` 是同一族的先例）。人群从文件系统派生，不手写清单。
describe("`.mjs` 与 `.mts` 的最小闸门", () => {
  // 读目录只经 `test-support/production-sources.ts`；`vendor` 底下的不算（三方带进来的脚本）。
  const mjsFiles = filePaths(".", { ext: ".mjs", tests: true, generated: true })
    .filter((f) => !f.split("/").slice(0, -1).some((d) => d.includes("vendor")))
    .map((f) => resolve(REPO_ROOT, f));

  it("每个 .mjs 都过 `node --check`（tsc 整类看不见它们）", async () => {
    // 抽取器自检：遍历塌了的话下面那条就是零命中地绿。
    nonEmpty(mjsFiles);
    // 每份一个 `node --check` 子进程，**同时**起、一起等：原先逐个同步起（十来份 × 一次 node 冷启动），
    //   机器忙时排队等调度的时间逐份累加，整套并跑时把这一格挤过 5 s 默认期限。量具不变，仍是 `node --check` 本身。
    const broken = (
      await Promise.all(
        mjsFiles.map(
          (f) =>
            new Promise<string | null>((done) => {
              execFile(process.execPath, ["--check", f], (e, _out, stderr) =>
                done(e ? `${f.slice(REPO_ROOT.length + 1)}: ${String(stderr || e).slice(0, 200)}` : null),
              );
            }),
        ),
      )
    ).filter((b): b is string => b !== null);
    expect(
      broken,
      "这些 .mjs 连语法都不过：\n" +
        broken.join("\n") +
        "\n★ tsc 整类看不见它们（allowJs 没开），vitest 也不导入它们 ——\n" +
        "  在本条之前，一个语法错能一路留到「有人真去跑它」那一刻。\n" +
        "  而本仓不 push ⇒ CI 结构上不会跑；tier2 那两个连 CI 都不跑。",
    ).toEqual([]);
  });

  // 前提触发器：`.mts` 那一半是靠 tsconfig 的 include 覆盖的 —— 那句话一旦不成立，
  // 上面这条只管了 `.mjs`，而 `.mts` 会**悄悄**变成同样的盲区。
  // 〔e2e 并入 tests/ 之后〕本条**原文是 `toContain("e2e")`** —— 那在 `e2e/` 住仓根时成立。
  // 并进 `tests/e2e/` 之后 `include` 里不再有 `"e2e"` 这个条目，而 `.mts` 那一批**照旧被覆盖**
  // （它们住 `tests/e2e/`，被 `"tests"` 收进去了）⇒ 原断言红的是**布局变了**，不是覆盖没了。
  // 处置纪律与「守卫钉死的计数」同一条：不是把红的那条删掉，而是问「它今天该读哪个条目」。
  // ⚠ 本条读的是**字符串**，不是真的类型检查覆盖面；它只是那句话的前提触发器。
  //   真正的证据是 `tsc --noEmit` 本身 —— 这一轮现打 EXIT=0。
  it("tsconfig 仍然把 tests 收进 include（`.mts` 那半靠它）", () => {
    const raw = readFileSync(resolve(REPO_ROOT, "tsconfig.json"), "utf8");
    const include = /"include"\s*:\s*\[([^\]]*)\]/.exec(raw);
    expect(include, "`tsconfig.json` 里找不到 `include` —— 读法坏了").not.toBeNull();
    expect(
      include![1],
      "`tsconfig.json` 的 include 不再收 `tests` —— `tests/e2e/` 下那一批 `.mts`\n" +
        "会**悄悄**退出类型检查，变成和 `.mjs` 一样的盲区。要么把它们并进别的 include，\n" +
        "要么把它们也纳入上面那条 `node --check`（但那只挡语法，挡不住类型）。",
    ).toContain("tests");
  });
});
