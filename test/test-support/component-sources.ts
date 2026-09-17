/**
 * S4b-3b-3：按**组件**收集源文件，供「源码级守卫」扫描。
 *
 * # 为什么需要它（主计划 §5-4）
 *
 * 那几条守卫（`cc-bus-section` 无定时器 / `cc-bus-hooks` 绝无写入 …）此前是
 * **写死单个文件名**去读的。问题：哪天有人把被守的那段代码搬到隔壁一个新文件，
 * **守卫还是绿的** —— 它只盯着原来那个文件名，而被守的代码已经跑到守卫看不见的地方。
 * 这不是假设：本区 S4b-3b-3 就把 `MachineCard` 从 `remote-section.ts` 搬走了 970 行。
 *
 * # 为什么**不是**「扫整个目录」
 *
 * 那些不变量是**针对某个组件**的，不是针对整个 `src/settings/`。
 * 「不许写盘」对 `cc-bus-hooks` 成立，对 `accounts-section` / `remote-section`
 * 根本不成立（它们本来就要写配置）。一刀切会得到一堆假红，然后守卫被放宽或删掉 ——
 * 比范围缩小更糟。
 *
 * ⇒ 折中：**按组件前缀收集**。`cc-bus-hooks` 会收到 `cc-bus-hooks-section.ts`、
 * 将来拆出的 `cc-bus-hooks-diag.ts` 等等。
 *
 * **局限如实说**：若拆出去的文件不带该前缀（叫 `hooks-diag.ts`），仍然漏。
 * 没有纯静态方案能完全解决「代码搬到哪儿了」；这一条把最常见的那种漂移堵上，
 * 并用 `expect(files.length)` 的反向自检保证它至少扫到了东西、不是空转。
 */
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const SETTINGS_DIR = resolve(process.cwd(), "src/settings");

/** 剥掉整行注释——守卫扫的是**代码**，文件头注里写了禁用模式不该打红自己。 */
export function stripLineComments(src: string): string {
  return src
    .split("\n")
    .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
    .join("\n");
}

export interface ComponentSource {
  file: string;
  /** 已剥掉整行注释的代码。 */
  code: string;
}

/**
 * 收集 `src/settings/` 下所有以 `prefix` 开头的**生产**源文件（排除测试）。
 * 调用方应断言返回非空（反向自检：别让守卫空转成假绿）。
 */
export function componentSources(prefix: string): ComponentSource[] {
  return readdirSync(SETTINGS_DIR)
    .filter(
      (f) =>
        f.startsWith(prefix) &&
        f.endsWith(".ts") &&
        !f.includes(".vitest.") &&
        !f.includes(".test."),
    )
    .sort()
    .map((f) => ({
      file: f,
      code: stripLineComments(readFileSync(resolve(SETTINGS_DIR, f), "utf8")),
    }));
}

/** 组件全部源码拼成一段（多数守卫只关心「整个组件里有没有出现 X」）。 */
export function componentCode(prefix: string): string {
  return componentSources(prefix)
    .map((s) => s.code)
    .join("\n");
}
