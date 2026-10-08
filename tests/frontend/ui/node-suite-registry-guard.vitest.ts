/**
 * U0（2026-08-01）：**给 16 个 tsx node 套件补上机检地板。**
 *
 * # 洞在哪
 *
 * `npm test`（`ci.yml:136` 前端 job 跑的就是它）把 16 个 `*.test.ts` 用 `&&` 串起来跑
 * （tsx，非 vitest）。它们各自是这个形状：
 *
 * ```
 * let failed = 0;
 * function test(name, fn) { try { fn(); } catch { failed++; console.error(`✗ …`); } }
 * …
 * if (failed > 0) { throw new Error(`… ${failed} failed`); }
 * ```
 *
 * 这个形状有**两条**都能导致「退出码 0 但什么都没验」的路：
 * ① **把测试删光** ⇒ `failed` 恒 0 ⇒ 静默绿。
 * ② **把收尾的 `if (failed > 0) … throw` 删掉** ⇒ 测试还在跑、还在打 `✗`，退出码照样 0。
 *
 * ②比①更重（表面上一切正常），而且**一行编辑就能重开**。Phase D 审计实测：
 * 删掉 `context-limit.test.ts`（当时叫 `pricing.test.ts`）的收尾 + 让一条断言必然失败 ⇒
 * `npm run test:context-limit` **RC=0**。
 * e2e 那几套由 `assert-pass-floor.sh` 兜这一类（收尾 `合计 PASS=<n> FAIL=0` 抓不到 ⇒ 红），这 16 套 **242 条**一直没有。
 *
 * > 把它记成「既无断言地板又被 `coverage.exclude` 排掉，双重不设防」——
 * > **「双重」那半不成立**：`coverage.exclude` 里的 `src/**\/*.test.ts` 排的是测试文件自身
 * > （标准做法），被测的生产代码仍在 `include` 里；且 `vitest.config.ts:21` 另有一条
 * > `src/**\/*.vitest.ts`，所以「放不放 `test-support/`」在覆盖率上没有差别。
 * > 真洞只有「无地板」这一条，本守卫只补这一条。
 *
 * # 判据（六条，互相咬）
 *
 * - **a 不空**：每个套件**行首** `test(` 至少一条（补上面那条①：测试删光 ⇒ 红）。条数本身不登记 —— 删一条测试是常事，
 *   被测对象还在、测试被悄悄删掉，那是逐条的 review 管的，不是一个会腐的数管得住的。
 * - **b 集合**：登记集合 == `package.json` 里所有「跑 `.test.ts` 的 tsx 脚本」。
 *   **匹配 `tsx` 用的是词边界正则而不是 `startsWith("tsx ")`** —— Phase D 审计实测
 *   `"npx tsx …"` 能从 `startsWith` 版本里**静默逃逸**（加个不登记不进链的套件，守卫全绿）。
 * - **b2 路径**：登记的文件路径与 `package.json` 的命令逐字相符。
 * - **c 链路**：每个登记脚本都出现在 `npm test` 的链里，且链上不许有跑 `.test.ts` 却没登记的。
 *   **这条是关键** —— 套件文件还在、登记还在，但从链里被摘掉，a/b 都发现不了，
 *   而后果就是它再也不跑。另外**链的连接符必须是 `&&`**：换成 `;` 的话前面任何一套失败
 *   都会被最后一条的退出码盖掉，与「静默绿」同一类。
 * - **d 收尾**：每个套件都必须留着 `if (failed > 0)` + `throw` 的失败收尾。补上面那条②。
 *
 * # 它挡不住什么（诚实段；在一条专治安慰剂的守卫里，这段不完整本身就是缺陷）
 *
 * - **`test()` 还在但断言被掏空。** 运行期 PASS 计数同样挡不住（空 body 照样计一次 PASS），
 *   所以这不是选静态计数的损失。那一类归伪测试扫荡（R02）管。
 * - **非行首的 `test(` 不在计数里** —— 缩进的、`await test(`、循环里生成的，a 条看不见。
 *   Phase D 审计实测：给某套件加一条缩进两格的 `test(` ⇒ 守卫全绿。
 *   今天 16 个套件里**0 处**非行首写法（非行首命中全是 `function test(` 定义、
 *   `/re/.test(x)` 方法调用、模板串里的 `test(s) failed`），所以是**潜伏不是现患**。
 * - **不以 `test:` 开头的脚本名**（`unit:foo`）整个绕过 b。
 * - `d` 只看收尾**在不在**，不看它是否真的能被触达（比如被 `if (false)` 包住）。
 */
import { execFile } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

/**
 * `(npm 脚本名, 文件)`。
 */
const NODE_SUITES: readonly (readonly [string, string])[] = [
  ["test:diff", "tests/frontend/ui/cards/diff.test.ts"],
  ["test:branching", "tests/frontend/ui/branching.test.ts"],
  ["test:api-error", "tests/frontend/ui/cards/api-error.test.ts"],
  // `test:bash` 整份删了：斜杠命令与 `!` 输入 / 输出的解析随「谁说的」进了后端（`text_tests.rs::slash_and_bash_forms`）。
  ["test:remote-health", "tests/frontend/ui/remote-health.test.ts"],
  // F04b +1：`isValidNewTmuxName` 也禁 `=`（别创建一个主路杀不掉的名字）。
  // `K-R96` +1（`KR96D3`：名字可读、sid 一个片段都不进去 + `@ccm_sid` 必须还在）。
  // 🔴 **44 → 37**：五个 builder 删了（生产调用 0），它们的用例改测生产的
  //    `plan*` ＋ `buildLaunchRenderRequest`（字节归 Rust 夹具）；合并掉的几条：直起 cwd 引号并进第一条 ·
  //    tmux cwd 引号并进空 cwd 那条 · `buildEnvPrefix` 三条收成「非法 configDir 在 plan 那一步就拒」一条 ·
  //    `posixQuote` 一条随函数删 · `buildOpenTerminalCmd` 一条随函数删（期望搬进 Rust `shell_tests`）。
  //    **37 → 39**：`session-backend.test.ts` 里两条与座无关的搬进来（F01 shim 漂移守卫 · P3s-Y2 铸名口数据流）。
  // **39 → 38**（被测对象没了）：`sanitizeRemoteLauncher` 一条 · `isValidConfigDir` 一条随函数删；
  //    加一条「launcher 空白 ⇒ 默认、注入字符原样上线」（前端只剩缺省那一格）；「非法 configDir 拒」那条改测「前端不判、原样上线」（条数不变）。
  // **38 → 37**：`isValidSessionId` 那条随函数删；三条「非法 sid ⇒ throw」改测「前端不判、resumeSid 单报」（条数不变）。
  ["test:remote-launch", "tests/frontend/ui/remote-launch.test.ts"],
  ["test:format", "tests/frontend/ui/format.test.ts"],
  // 🔴 〔删用量 09-18〕原先这里有 `["test:usage-pivot", "tests/frontend/ui/views/usage-pivot.test.ts", 14]`。
  // 用量 ② 轴整轴退役 ⇒ 套件文件整删（**被测对象没了**，不是把测试删光了）。
  // ⚠ **`package.json` 那一半不在本轮写区里**：`test:usage-pivot` 与 `test:usage-probe`
  //    两条 script、以及 `test` 那条 `&&` 链里的 `npm run test:usage-pivot`，要由
  //    改 `package.json` 的那一路同拍摘掉 —— 在那之前本文件的 b/c 两条会红，已随本件上报。
  // §5 步 12：`views/pricing.ts` → `views/context-limit.ts`（只剩 context 上限那半，
  // 名字名不副实），套件与脚本名同拍改。条数 6 → 5：`equivalentInputTokens` 那一例随 ② 轴
  // 退役（`RELATIVE_COST` 的唯一消费者是用量视图）。
  // 条数 5 → 3：上下文上限的判定搬进后端（`observe/facts_query.rs::context_limit`，逐格归 `facts_query_tests.rs`），
  // `contextLimit` 三条与 `contextPercent` 一条随函数删；前端只剩排版（`contextPercentOf`）与读设置表（`readContextLimits`）各一条。
  // 3 → 4：上限判不出时只写用了多少（`contextTokensText`）。
  ["test:context-limit", "tests/frontend/ui/views/context-limit.test.ts"],
  // 🔴 原先这里有 `["test:session-backend", "tests/session-backend.test.ts", 10]`。TS 座
  // `session-backend.ts` 零生产调用、删了 ⇒ 套件整删（**被测对象没了**，8 条测座本身）；外层 tmux 三格的字节由
  // 入库夹具 `tmux-outer-golden.json` ＋ Rust `payload_tests.rs` 接着（对照见）；
  // 与座无关的 2 条（shim 漂移守卫 · 铸名口数据流）搬进了 `remote-launch.test.ts`。
  // 🔴 原先这里有 `["test:panorama-session-files", "tests/frontend/ui/panorama/session-files.test.ts", 7]`。
  // 写类工具那张表与它的口径搬进了后端（会话事实由后端出成品）⇒ 被测对象 `collectEditedFiles` 删了、套件整删
  // （**被测对象没了**，不是把测试删光了）；七条逐条搬进 `tests/backend/observe/facts_query_tests.rs::edit_tools_rules_moved_from_the_frontend_suite`。
  // 🔴 原先这里有 `["test:launch-dimensions", "tests/frontend/ui/launch-dimensions.test.ts", 31]`。
  // 维度表只为载荷渲染那条存在，起会话只剩那一行 `ccm …` ⇒ 维度表与套件整删（**被测对象没了**）。
  // 🔴 原先这里有 `["test:launch-render-cli", "tests/launch-render-cli.test.ts", 30]`。
  // TS 那份 `ccm …` 调用行渲染器删了 ⇒ 套件整删（**被测对象没了**）；它测的行为逐条由
  // Rust `ccm_invocation_tests.rs` 与入库夹具 `cli-golden.json` 接着（对照见）。
];


/** 判定一条 npm 命令是不是「用 tsx 跑某个 `.test.ts`」。`tsx …` 与 `npx tsx …` 都算。 */
const TSX_SUITE_CMD = /(^|\s)(npx\s+)?tsx\s+(--\S+\s+)*(\S+\.test\.ts)\s*$/;

function scripts(): Record<string, string> {
  const pkg = JSON.parse(readFileSync(resolve(REPO_ROOT, "package.json"), "utf8")) as {
    scripts: Record<string, string>;
  };
  return pkg.scripts;
}

/** 行首 `test(` 的条数。先剥注释，免得注释里写了 `test(` 也被数进去。 */
function lineStartTestCount(src: string): number {
  return (stripComments(src, "ts").match(/^test\(/gm) ?? []).length;
}

function readSuite(file: string): string {
  return readFileSync(resolve(REPO_ROOT, file), "utf8");
}

describe("U0：tsx node 套件的机检地板", () => {
  it("a · 每个套件的行首 test() 至少一条（测试删光 ⇒ 退出码 0 却什么都没验）", () => {
    const empty = NODE_SUITES.filter(([, file]) => lineStartTestCount(readSuite(file)) === 0).map(
      ([script, file]) => `${script} (${file})`,
    );
    expect(empty, `这些套件一条行首 test() 都没有了：\n  ${empty.join("\n  ")}`).toEqual([]);
  });

  it("b · 登记表覆盖 package.json 里全部 tsx 套件，不多不少", () => {
    const declared = Object.entries(scripts())
      .filter(([k, v]) => k.startsWith("test:") && TSX_SUITE_CMD.test(v))
      .map(([k]) => k)
      .sort();
    const registered = NODE_SUITES.map(([s]) => s).sort();
    expect(
      registered,
      "登记表与 package.json 的 tsx 套件集合不一致 —— 新加套件要登记，删套件要清登记。\n" +
        `package.json: ${declared.join(", ")}\n登记表: ${registered.join(", ")}`,
    ).toEqual(declared);
  });

  it("b2 · 登记的文件路径与 package.json 的命令逐字相符", () => {
    const sc = scripts();
    for (const [script, file] of NODE_SUITES) {
      expect(sc[script], `${script} 在 package.json 里不存在`).toBeTruthy();
      expect(sc[script].trim(), `${script} 的命令与登记的文件对不上（登记 ${file}）`).toBe(
        `tsx ${file}`,
      );
    }
  });

  it("c · 每个套件都真的挂在 `npm test` 链上，且链是 && 串的", () => {
    const chain = scripts()["test"];
    expect(chain, "package.json 里没有 `test` 脚本").toBeTruthy();
    expect(
      chain,
      "`npm test` 的链必须全用 `&&` 串：换成 `;` 或 `||` 之后，前面任何一套失败" +
        "都会被最后一条的退出码盖掉 —— 与「静默绿」同一类。",
    ).not.toMatch(/;|\|\|/);
    const missing = NODE_SUITES.map(([s]) => s).filter(
      // 用后随的分隔符钉住，免得 `test:history-cache` 被 `test:history-cache-extra` 误判成命中。
      (s) => !new RegExp(`npm run ${s}(\\s|$)`).test(chain),
    );
    expect(
      missing,
      `这些套件登记了、文件也在，但**没挂在 \`npm test\` 链上**⇒ 它们根本不跑：${missing.join(", ")}`,
    ).toEqual([]);
    // 反向：链上凡是「跑 .test.ts 的 tsx 脚本」都必须登记。
    // **按命令形态判，不按名字白名单** —— 白名单版本会让 `test:f40`（bash e2e）这类
    // 正当地挂进链时以「不在登记表里」误红，把人指向错误方向。
    const sc = scripts();
    for (const ref of chain.match(/npm run (test:[a-z0-9-]+)/g) ?? []) {
      const name = ref.replace("npm run ", "");
      if (!TSX_SUITE_CMD.test(sc[name] ?? "")) continue; // 非 tsx 套件（test:dom 等）不归本表管
      expect(
        NODE_SUITES.some(([s]) => s === name),
        `\`npm test\` 链上的 ${name} 跑的是 .test.ts 却不在登记表里`,
      ).toBe(true);
    }
  });

  it("d · 每个套件都留着 `if (failed > 0)` + throw 的失败收尾", () => {
    const broken = NODE_SUITES.flatMap(([script, file]) => {
      const src = stripComments(readSuite(file), "ts");
      const i = src.indexOf("if (failed > 0)");
      if (i < 0) return [`${script} (${file}): 找不到 \`if (failed > 0)\` 收尾`];
      // 收尾之后必须真的抛。窗口给宽些，容得下 `{ …console… throw … }` 的写法。
      return src.slice(i, i + 400).includes("throw")
        ? []
        : [`${script} (${file}): 有 \`if (failed > 0)\` 但其后 400 字符内没有 throw`];
    });
    expect(
      broken,
      "有套件丢了失败收尾：\n  " +
        broken.join("\n  ") +
        "\n没有它，测试照样跑、照样打 ✗，而**退出码是 0** —— `npm test` 全绿、CI 全绿。\n" +
        "这是本守卫要挡的第二条静默绿路（Phase D 审计实测复现过）。",
    ).toEqual([]);
  });
});

// ★★ **CI 里那两步的「有效性」压在两个 npm 脚本的内容上**
// 〔audit-0805 08-08，Phase G 第 67 件〕。
//
// `ci.yml` 有两步：`coverage floor (vitest jsdom)` → `npm run coverage`，
// 随后 `coverage per-file floors + zero-coverage ratchet` → 读覆盖率产物。
// `shared_crate_registry` 已经钉住「这两步在 CI 里还在」，
// 隔壁那条也钉住「每个套件都真的挂在 `npm test` 链上」。
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
    expect(
      steps.length,
      "抽不到那条递减记录的箭头链（08-08 实测 `17→16` 与 `16→…→14→13` 共 5 格）——\n" +
        "记录的写法变了（比如换了箭头字符），本条会零命中地绿，先修抽取器。",
    ).toBeGreaterThanOrEqual(4);

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
    expect(
      rows.length,
      "从 `PER_FILE_FLOORS` 抽不到足够的行（08-08 实测 6 行）—— 抽取坏了，本条此刻无效",
    ).toBeGreaterThanOrEqual(4);
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
// 实测往 `tests/e2e/tmux-target-emit.mts` 塞一个类型错，`tsc --noEmit` 当场报 TS2322）。
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
  const mjsFiles = (() => {
    const skip = new Set([".git", "node_modules", "target", "dist", "coverage", ".vite"]);
    const out: string[] = [];
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        if (e.isDirectory()) {
          if (!skip.has(e.name) && !dir.includes("vendor")) walk(`${dir}/${e.name}`);
        } else if (e.name.endsWith(".mjs")) {
          out.push(`${dir}/${e.name}`);
        }
      }
    };
    walk(REPO_ROOT);
    return out.sort();
  })();

  it("每个 .mjs 都过 `node --check`（tsc 整类看不见它们）", async () => {
    // 抽取器自检：遍历塌了的话下面那条就是零命中地绿。
    expect(
      mjsFiles.length,
      "全仓一个 .mjs 都没扫到（08-08 实测 6 个）—— 遍历坏了，本条此刻无效",
    ).toBeGreaterThanOrEqual(4);
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
      "`tsconfig.json` 的 include 不再收 `tests` —— `.mts` 那一批（08-08 实测 3 个，\n" +
        "今天住 `tests/e2e/`：\n" +
        "`tmux-target-emit` / `ccm-print-parity-emit` / `launch-payload-golden-emit`）\n" +
        "会**悄悄**退出类型检查，变成和 `.mjs` 一样的盲区。要么把它们并进别的 include，\n" +
        "要么把它们也纳入上面那条 `node --check`（但那只挡语法，挡不住类型）。",
    ).toContain("tests");
  });
});
