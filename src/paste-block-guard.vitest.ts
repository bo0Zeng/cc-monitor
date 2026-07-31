// T03 的结构性守卫：「待贴配置文本」这件事**只有一个实现**。
//
// 守法是**白名单**（本会话已九次栽在黑名单上）：枚举全仓每一处 `writeText`，
// 要求它落在两张已知名单之一——族 A（待贴配置文本，必须走 `buildPasteBlock`）
// 或族 B（复制点东西给人看，契约不同，已登记不收）。新增一处两边都不在 → 红。
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, sep } from "node:path";

/** 族 A：待贴进配置文件才生效。**必须**走统一组件。 */
const FAMILY_A = [
  "src/launcher-diagnostics.ts", // 别名函数 → ~/.bashrc
  "src/settings/cc-bus-hooks-section.ts", // hooks JSON → ~/.claude/settings.json
  "src/settings/remote-section.ts", // ccm wrapper → 远端 ~/.bashrc
];

/**
 * 族 B：复制完就完事，不贴进任何配置。**如实登记为一处已知重复，本轮不收**
 * ——它的 UX 契约不同（没有"贴到哪"、没有"怎样才生效"），硬塞进同一个组件
 * 会让族 A 的三个必填槽在族 B 全是空的。
 */
const FAMILY_B = [
  "src/settings/config-surface-section.ts", // T02 复制诊断文本
  "src/main.ts",
  "src/remote-launch-run.ts", // 回退：复制命令让用户自己跑
  // S10：用量视图里 plan 窗口读不出来时，把抓到的原始屏交给用户复制去报障。
  // **是族 B**（复制完就完事），不是待贴配置文本——没人要把它贴进任何文件。
  "src/views/usage-view.ts",
  "src/paste-block.ts", // 组件自己
];

/**
 * ★ 族 AB：**两族都属于**的文件。Z05 撞出来的：`accounts-section.ts` 原先是纯族 B
 * （复制命令 / 复制路径 / 复制诊断文本 = 复制完就完事），Z05 给它加了一个真·待贴块
 *（rc 片段 → 远端 `~/.bashrc`）⇒ 它同时有两种语义。
 *
 * **原先的 A/B 二分覆盖不了这种文件**：塞进族 A 会被「族 A 里不得再出现裸 writeText」
 * 打红（那三处 `writeText` 是正当的族 B 用途）；留在族 B 会被「族 B 不许出现待贴语义」打红。
 *
 * **判据换成计数上下界，不是「有没有 writeText」**：族 A 那条规则真正要防的是
 * 「有人手搓一个复制按钮绕开组件」。对混合文件而言，能表达这个性质的是
 * **`writeText` 处数恰好等于已登记的族 B 用途数** —— 多出一处就说明又手搓了一个。
 */
const FAMILY_AB: Array<{ file: string; writeTextUses: number; why: string }> = [
  {
    file: "src/settings/accounts-section.ts",
    // 2026-07-30 实测：复制命令(:300) / 复制路径(:703) / 复制诊断文本(:799)。
    writeTextUses: 3,
    why: "族 B 三处（复制命令/路径/诊断）+ Z05 的 rc 片段待贴块（走组件）",
  },
];

/**
 * 收集源文件，**路径一律用 `/`**。
 *
 * `join` 在 Windows 上产出 `src\main.ts`，而上面三张名单是手写的 `src/main.ts`
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

describe("待贴配置文本只有一个实现", () => {
  const files = walk("src");

  it("枚举全仓 writeText，每一处都必须在两张名单之一（白名单，新增即红）", () => {
    const hits = files.filter((f) =>
      readFileSync(f, "utf8").includes("writeText"),
    );
    // 反向自检：一处都没扫到 = 守卫失效了，不是代码变干净了。
    // 阈值 5 = 组件自己 + 族 B 的四个文件。**迁移前是 7 个文件 / 9 处**
    // （`accounts-section.ts` 独占 3 处）——审计核实我这条注释原先把"处"写成了"文件"，
    // 而这条注释正是阈值的论证依据。族 A 三处迁移后各自不再持有 `writeText`，
    // 于是 7 → 5：**这个数字下降就是迁移成功的直接证据**。
    expect(hits.length).toBeGreaterThanOrEqual(5);
    const known = new Set([...FAMILY_A, ...FAMILY_B, ...FAMILY_AB.map((x) => x.file)]);
    const unknown = hits.filter((f) => !known.has(f));
    expect(unknown).toEqual([]);
  });

  it("族 A 三处都必须走 buildPasteBlock，不许再自己拼复制按钮", () => {
    for (const f of FAMILY_A) {
      const src = readFileSync(f, "utf8");
      expect(src, `${f} 应引入统一组件`).toContain("buildPasteBlock");
    }
  });

  it("族 A 里不得再出现裸 writeText（那意味着又有一处绕开了组件）", () => {
    for (const f of FAMILY_A) {
      // 剥掉行注释，免得注释里提到 writeText 被当成代码（本会话踩过"把注释当代码"）
      const code = readFileSync(f, "utf8")
        .split("\n")
        .filter((l) => !l.trimStart().startsWith("//"))
        .join("\n");
      expect(code.includes("writeText"), `${f} 里仍有裸 writeText`).toBe(false);
    }
  });

  it("族 B 成员不许悄悄变成待贴块（白名单不能只白在文件名上）", () => {
    // 审计实测的绕过：新建一个含 `writeText` 的文件 → 守卫红（对）；
    // **把文件名加进 FAMILY_B → 全绿**。族 B 成员身上原先一条断言都没有，
    // 任何人手搓一个待贴块只需往数组里加一行。
    // 正向约束：族 B 的活儿是"复制点东西给人看"，**不该出现待贴语义的文案**
    // （那三句话是族 A 的契约）。真要做待贴块，就得走组件、从族 B 挪进族 A。
    for (const f of FAMILY_B) {
      if (f === "src/paste-block.ts") continue; // 组件自己就是那套文案的产地
      const code = readFileSync(f, "utf8")
        .split("\n")
        .filter((l) => !l.trimStart().startsWith("//"))
        .join("\n");
      // 判据要**精确**：第一版用了 `"贴到 "`，结果打在 `accounts-section.ts:721`
      // 那句"把它贴到 cc-monitor 的 GitHub issue 里"上——那是贴到 issue，不是贴进配置。
      // 现在只认组件独有的两个信号：`import` 了组件，或吐出组件那句专属文案。
      for (const smell of [
        'from "../paste-block"',
        'from "./paste-block"',
        "生效条件：",
      ]) {
        expect(
          code.includes(smell),
          `${f} 出现了待贴语义 ${smell}：要么它其实是族 A（走组件），要么改个说法`,
        ).toBe(false);
      }
    }
  });

  it("★ 族 AB：既走组件，又只保留已登记的那几处族 B 复制（多一处即红）", () => {
    for (const { file, writeTextUses, why } of FAMILY_AB) {
      const raw = readFileSync(file, "utf8");
      const code = raw
        .split("\n")
        .filter((l) => !l.trimStart().startsWith("//"))
        .join("\n");
      // 族 A 侧的约束：待贴块必须来自组件，不许手搓。
      expect(code, `${file} 是族 AB，待贴那半必须走组件（${why}）`).toContain("buildPasteBlock");
      // 族 B 侧的约束换成**计数上下界**：这才是「没有人手搓第 N+1 个复制按钮」的可判据。
      const uses = code.split("writeText").length - 1;
      expect(
        uses,
        `${file} 的 writeText 处数从 ${writeTextUses} 变成了 ${uses}：` +
          "多出来的那处要么该走组件（待贴语义），要么在这里登记它是第几处族 B 用途",
      ).toBe(writeTextUses);
    }
  });

  it("组件本身是唯一持有 writeText 的地方（族 A 侧），且不吞错误", () => {
    const raw = readFileSync("src/paste-block.ts", "utf8");
    expect(raw).toContain("writeText");
    // **剥注释再查**——第一版没剥，被本文件自己的注释「不许吞进 console」判成违规。
    // 「把注释当代码」这条本会话已栽过一次，这次栽在断言侧：守卫要剥，断言也要剥。
    const code = raw
      .split("\n")
      .filter((l) => !l.trimStart().startsWith("//"))
      .join("\n");
    expect(code).toContain("writeText"); // 反向自检：别剥过头
    expect(code).not.toContain("console.warn"); // 迁移前 A3 的缺陷不许回来
  });
});
