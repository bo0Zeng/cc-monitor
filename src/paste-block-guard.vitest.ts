// T03 的结构性守卫：「待贴配置文本」这件事**只有一个实现**。
//
// 守法是**白名单**（本会话已九次栽在黑名单上）：枚举全仓每一处 `writeText`，
// 要求它落在两张已知名单之一——族 A（待贴配置文本，必须走 `buildPasteBlock`）
// 或族 B（复制点东西给人看，契约不同，已登记不收）。新增一处两边都不在 → 红。
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

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
  "src/settings/accounts-section.ts", // 复制命令 / 复制路径 / 复制诊断文本
  "src/settings/config-surface-section.ts", // T02 复制诊断文本
  "src/main.ts",
  "src/remote-launch-run.ts", // 回退：复制命令让用户自己跑
  "src/paste-block.ts", // 组件自己
];

function walk(dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".ts") && !p.endsWith(".vitest.ts")) out.push(p);
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
    // 阈值 5 = 组件自己 + 族 B 的四个文件。迁移前是 9 个文件带 writeText，
    // 族 A 三处迁移后各自不再持有它——**这个数字下降就是迁移成功的直接证据**。
    expect(hits.length).toBeGreaterThanOrEqual(5);
    const known = new Set([...FAMILY_A, ...FAMILY_B]);
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
