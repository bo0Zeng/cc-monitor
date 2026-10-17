// T03 的结构性守卫：「待贴配置文本」这件事**只有一个实现**。
//
// 守法是**白名单**（本会话已九次栽在黑名单上）：枚举全仓每一处写剪贴板（`writeClipboard`：界面写剪贴板只这一口，
// 经壳拿真成败 —— `clipboard.ts`；网页自己的 `writeText` 全仓零处，由 `eslint.config.js` 里那条 `no-restricted-syntax` 钉），
// 要求它落在两张已知名单之一——族 A（待贴配置文本，必须走 `buildPasteBlock`）
// 或族 B（复制点东西给人看，契约不同，已登记不收）。新增一处两边都不在 → 红。
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, sep } from "node:path";
import { nonEmpty } from "../../test-support/non-empty";

/** 族 A：待贴进配置文件才生效。**必须**走统一组件。 */
const FAMILY_A: string[] = [
  // 别名那一行的「我自己贴」不再就地弹代码：点了去「待办」里那一件（`data-page.ts`，见族 B 那一行）。
  // `ext-section.ts`（cc-bus 那一行每台要加的钩子）：MCP 那一行原来的［去登录］（复制登录命令）换成开登录抽屉之后，
  //   它不再自己写剪贴板，从族 AB 回到族 A。
  "src/frontend/ui/settings/ext-section.ts",
];

/**
 * 族 B：复制完就完事，不贴进任何配置。**如实登记为一处已知重复，暂不收**
 * ——它的 UX 契约不同（没有"贴到哪"、没有"怎样才生效"），硬塞进同一个组件
 * 会让族 A 的三个必填槽在族 B 全是空的。
 */
const FAMILY_B = [
  "src/frontend/ui/settings/diagnostics-section.ts", // 日志页「复制诊断信息」（复制完就完事，不贴进任何配置）
  // 〔三入口拆分 · 住址搬家〕代码块「复制」那处全局代理从 `main.ts` 搬到了主窗与 viewer 窗
  // 共用的 `entry-render-common.ts`（viewer 不再加载 `main.ts`）。语义一字未改。
  "src/frontend/ui/entry-render-common.ts",
  "src/frontend/ui/session-open.ts", // 开不了终端：复制那一行让用户自己跑
  "src/frontend/ui/terminal-open.ts", // 找不到终端时的［复制命令］：用户点了才复制那一行
  "src/frontend/ui/kit/detail.ts", // 全产品那一颗［复制详情］：一条报错的那句 ＋ 详情，复制给人看（不贴进任何配置）
  // 原先这里还有 `src/frontend/ui/views/usage-view.ts`（用量视图里「复制这一屏」）——
  // 用量 ② 轴整轴退役，那份文件整删。
  "src/frontend/ui/paste-block.ts", // 组件自己
  "src/frontend/ui/clipboard.ts", // 写剪贴板那一口自己（经壳）
  // 账号分节：复制账号目录路径 · 本机不开终端窗口时把登录那一行复制给人自己跑。〔账号库收进后端〕rc 片段那两处待贴块
  //   随功能删了（账号别名由后端自动写进别名文件），它从族 AB 回到族 B。
  "src/frontend/ui/settings/accounts-section.ts",
  "src/frontend/ui/settings/profiles-list.ts", // 别名清单：复制要删的 .bashrc 行号（人自己去删，cc-monitor 不改它）
  // 「待办」：要贴的那几行 / 合好的整份。贴到哪 · 改哪一行 · 何时生效由那台后端成品的编号步骤与 diff 给
  //   （定下的样子，不是三槽组件那一形），复制之后那一件标「已复制 · 等你贴」、存盘后后端自己认出。
  "src/frontend/ui/settings/data-page.ts",
  "src/frontend/ui/views/plan.ts", // 计划页：认不出的接手 / 签收人那一枚点了复制那个 id（给人看，不贴进任何配置）
  "src/frontend/ui/views/plan-review.ts", // 退回框：送不了时［复制这一行］（给人自己去会话里贴，不贴进任何配置）
  "src/frontend/ui/views/plan-cell.ts", // 一格详情：复制「跟 agent 说时用」的编号（给人看，不贴进任何配置）
  "src/frontend/ui/login-drawer.ts", // 登录抽屉［复制链接］：把登录链接给别的设备用（不贴进任何配置）
];

/**
 * 收集源文件，**路径一律用 `/`**。
 *
 * `join` 在 Windows 上产出 `src\main.ts`，而上面三张名单是手写的 `src/frontend/ui/main.ts`
 * ⇒ 名单查不中 ⇒ **每一个白名单文件都被报成「未登记的 writeText」**，
 * 整条守卫在 Windows runner 上恒红。而这个项目的**主平台就是 Windows**，
 * 也就是说这条守卫在它最该生效的那台机器上从来没绿过。
 *
 * 归一化放在**产出侧**而不是比较侧：名单是给人读的，别让每次比较都记得转一道。
 */
function walk(dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".ts") && !p.endsWith(".vitest.ts")) {
      out.push(p.split(sep).join("/"));
    }
  }
  return out;
}

/** 写剪贴板那一口的名字（`src/frontend/ui/clipboard.ts`）。 */
const WRITE = "writeClipboard";

describe("待贴配置文本只有一个实现", () => {
  const files = walk("src");

  it("枚举全仓写剪贴板（writeClipboard），每一处都必须在两张名单之一（白名单，新增即红）", () => {
    const hits = files.filter((f) =>
      readFileSync(f, "utf8").includes(WRITE),
    );
    // 反向自检：一处都没扫到 = 守卫失效了，不是代码变干净了。
    nonEmpty(hits);
    const known = new Set([...FAMILY_A, ...FAMILY_B]);
    const unknown = hits.filter((f) => !known.has(f));
    expect(unknown).toEqual([]);
  });

  it("族 A 两处都必须走 buildPasteBlock，不许再自己拼复制按钮", () => {
    nonEmpty(FAMILY_A);
    for (const f of FAMILY_A) {
      const src = readFileSync(f, "utf8");
      expect(src, `${f} 应引入统一组件`).toContain("buildPasteBlock");
    }
  });

  it("族 A 里不得再出现裸写剪贴板（那意味着又有一处绕开了组件）", () => {
    for (const f of FAMILY_A) {
      // 剥掉行注释，免得注释里提到 writeText 被当成代码（本会话踩过"把注释当代码"）
      const code = readFileSync(f, "utf8")
        .split("\n")
        .filter((l) => !l.trimStart().startsWith("//"))
        .join("\n");
      expect(code.includes(WRITE), `${f} 里仍有裸 writeText`).toBe(false);
    }
  });

  it("族 B 成员不许悄悄变成待贴块（白名单不能只白在文件名上）", () => {
    // 审计实测的绕过：新建一个含 `writeText` 的文件 → 守卫红（对）；
    // **把文件名加进 FAMILY_B → 全绿**。族 B 成员身上原先一条断言都没有，
    // 任何人手搓一个待贴块只需往数组里加一行。
    // 正向约束：族 B 的活儿是"复制点东西给人看"，**不该出现待贴语义的文案**
    // （那三句话是族 A 的契约）。真要做待贴块，就得走组件、从族 B 挪进族 A。
    for (const f of FAMILY_B) {
      if (f === "src/frontend/ui/paste-block.ts") continue; // 组件自己就是那套文案的产地
      const code = readFileSync(f, "utf8")
        .split("\n")
        .filter((l) => !l.trimStart().startsWith("//"))
        .join("\n");
      // 判据要**精确**：第一版用了 `"贴到 "`，结果打在 `accounts-section.ts:721`
      // 那句"把它贴到 cc-monitor 的 GitHub issue 里"上——那是贴到 issue，不是贴进配置。
      // 现在只认组件独有的两个信号：`import` 了组件，或吐出组件那句专属文案。
      for (const smell of [
        'from "../paste-block"',
        'from "../../../src/frontend/ui/paste-block"',
        "生效条件：",
      ]) {
        expect(
          code.includes(smell),
          `${f} 出现了待贴语义 ${smell}：要么它其实是族 A（走组件），要么改个说法`,
        ).toBe(false);
      }
    }
  });

  it("组件本身是唯一持有写剪贴板的地方（族 A 侧），且不吞错误", () => {
    const raw = readFileSync("src/frontend/ui/paste-block.ts", "utf8");
    expect(raw).toContain(WRITE);
    // **剥注释再查**——第一版没剥，被本文件自己的注释「不许吞进 console」判成违规。
    // 「把注释当代码」这条本会话已栽过一次，这次栽在断言侧：守卫要剥，断言也要剥。
    const code = raw
      .split("\n")
      .filter((l) => !l.trimStart().startsWith("//"))
      .join("\n");
    expect(code).toContain(WRITE); // 反向自检：别剥过头
    expect(code).not.toContain("console.warn"); // 迁移前 A3 的缺陷不许回来
  });
});
