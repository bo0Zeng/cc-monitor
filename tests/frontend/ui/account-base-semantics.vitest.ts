/**
 * audit-0805 F12 下半（报告 I-13）：**「不指定账号」这四个字，UI 说的和系统做的对不对得上**。
 *
 * # 为什么这一件只做「说清楚」，不做「改行为」
 *
 * 功能件 §4 判定账号规则那半「不在本件硬做」，理由是它要同时动三份规则、两派调用点、
 * 两句文案，而「不指定账号到底该落哪个号」**还没有共识** —— 在没有共识的地方改行为
 * 只会制造第三种说法。那条推理**对「改行为」成立**。
 *
 * 但里面有两件**零行为改动**的活，不需要任何共识：
 *
 * 1. **两句给用户看的文案在主路径上是假的** —— 描述现状不需要先决定现状该不该变。
 * 2. **两派调用点没人登记** —— 记下分歧、逼下一个新调用点表态，也不需要先解决分歧。
 *
 * # 那句文案错在哪（实测）
 *
 * 原文（`settings/machine-card.ts`）：「不指定账号（用远端已登录的那个，**不注入** CLAUDE_CONFIG_DIR）」
 *
 * - `kind: "base"` → `ACCOUNT_DIMENSION`（`applies` **恒真**，已被 `launch-dimensions.test.ts`
 *   钉住）→ CLI 路**必发 `--base`**；
 * - `shared/ccm` 收到 `--base` 是 **`unset CLAUDE_CONFIG_DIR`**（`:674` 送进 tmux 的载荷行
 *   ＋ `:709` 会话级 env，两处都 unset），**不是「不注入」**；
 * - 远端 shell 里若有一句 `export CLAUDE_CONFIG_DIR=<某账号>`（用户自己写的，或早先的账号工具留下的），
 *   「不注入」会继承它、「unset」落回 `~/.claude` —— **两者落到的是不同的账号**。
 *
 * ★ 最能说明问题的一点：`machine-card.ts` 那句文案**上一行的注释一直是对的**
 * （「用远端 `~/.claude` 那套基座凭据，不受当前账号影响」）。
 * 写代码的人知道，**给用户看的那句没跟上**。
 *
 * ⚠ 兜底渲染路（不发 `--base`）才是**真的不注入**。文案按**主路径**写，这一点写进两处代码注释。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { copyTableTextsIn } from "../../test-support/copy-refs.ts";

const REPO = resolve(__dirname, "../../..");
const read = (rel: string): string => readFileSync(resolve(REPO, rel), "utf8");

/**
 * 只取**字符串字面量**里的文案 —— 注释里会原样引用旧文案（正是为了记住它错在哪）。
 *
 * 两道处理，作用**不对等**（变异实测，别读反）：
 *   ① **排除换行**（`[^"\n]`）—— **真正起作用的是这一道**。不排除的话 `[^"]*` 会跨行，
 *      从上一处引号一路吞到下一处，把整段注释卷进来。第一版缺了它，当场被自己咬红。
 *   ② **剥整行注释** —— 今天**去掉它判据照样绿**（变异实测），因为两处注释引用旧文案时
 *      用的是中文引号「」而不是 ASCII `"`。留着是**纵深防御**：哪天有人在注释里用
 *      ASCII 引号原样贴一句旧文案，没有这道就会把注释当成现行文案。
 *
 * ⚠ 写「两道都不能省」是错的 —— 那句话在上一件（F14 第六刀的 `[ -r ]`）上刚栽过一次。
 * **「哪一行在真正干活」这种断言必须变异验过再写。**
 */
function uiStrings(src: string): string[] {
  const prod = src
    .split("\n")
    .filter((l) => {
      const t = l.trimStart();
      return !(t.startsWith("//") || t.startsWith("*") || t.startsWith("/*"));
    })
    .join("\n");
  const literal = [...prod.matchAll(/"([^"\\\n]*不指定账号[^"\\\n]*)"/g)].map((m) => m[1]);
  // 抽表之后这句话住文案表，文件里只剩 copyText("key") ⇒ 表条目一起算（否则抽完就零命中地绿）。
  return [...literal, ...copyTableTextsIn(prod).filter((zh) => zh.includes("不指定账号"))];
}

/** 「不指定账号」这句话的两份副本。加第三份时把它登记进来。 */
const COPIES: ReadonlyArray<readonly [file: string, where: string]> = [
  ["src/frontend/ui/settings/machine-card.ts", "机器卡片的新会话账号下拉，空选项"],
  ["src/frontend/ui/launch-menu.ts", "resume 浮层的账号修饰选项"],
] as const;

describe("「不指定账号」的文案必须与 --base 的真实语义对上（audit-0805 F12，E3）", () => {
  it("★ 抽取器自检：两份副本都真的抠到了文案", () => {
    for (const [file, where] of COPIES) {
      const found = uiStrings(read(file));
      expect(
        found.length,
        `${file}（${where}）里抠不到「不指定账号」的字符串字面量 —— ` +
          "文案被改写或搬走了，下面几条会零命中地绿",
      ).toBeGreaterThan(0);
    }
  });

  it("★ 不许再说「已登录的那个」—— 主路径上它是假的", () => {
    for (const [file, where] of COPIES) {
      for (const s of uiStrings(read(file))) {
        expect(
          s,
          `${file}（${where}）的文案又说回「已登录的那个」了：「${s}」\n` +
            "★ 实测不是这样：`kind:\"base\"` 经 ACCOUNT_DIMENSION（applies 恒真）必发 `--base`，\n" +
            "  而 ccm 收到 `--base` 是 **unset CLAUDE_CONFIG_DIR**（shared/ccm:674 + :709）\n" +
            "  ⇒ 落 `~/.claude` 基座，而基座常常没凭据。远端 shell 若有 shellinit 写的\n" +
            "  `export CLAUDE_CONFIG_DIR=<某账号>`，「不注入」会继承它、「unset」不会 ——\n" +
            "  两者落到的是**不同的账号**。",
        ).not.toContain("已登录的那个");
      }
    }
  });

  it("★ 也不许说「不注入」—— 那弱于事实（实际是 unset）", () => {
    for (const [file, where] of COPIES) {
      for (const s of uiStrings(read(file))) {
        expect(
          s.includes("不注入") && !s.includes("清掉"),
          `${file}（${where}）的文案说「不注入」而没说会清掉：「${s}」\n` +
            "「不注入」与「unset」在**远端 shell 已有继承值**时结论相反。",
        ).toBe(false);
      }
    }
  });

  it("正向要求：得说清落到哪儿（`~/.claude`）", () => {
    for (const [file, where] of COPIES) {
      for (const s of uiStrings(read(file))) {
        expect(
          /~\/\.claude/.test(s),
          `${file}（${where}）的文案没说清落到哪儿：「${s}」—— ` +
            "只说「不指定」等于把后果留给用户猜，而这两条路的后果是不同的账号。\n" +
            "⚠ **别用「基座」** —— 那是内部叫法，`settings/base-wording-guard.vitest.ts`（S8）明令禁止。\n" +
            "  S8 钉的是**词汇**，本条钉的是**真伪**，两条互补、不冲突。",
        ).toBe(true);
      }
    }
  });

  it("★ 文案赖以成立的那个事实还在：ccm 收到 --base 会 unset 账号载体", () => {
    // 🔴 `shared/ccm` 那个 bash 脚本删了
    //（〔用@09-11 `K33`〕「不要有什么 bash 脚本」），`--base` 的落点搬进了后端本体。
    // ⚠ **「两处」变「一处」不是判据放宽**：bash 那版 send-keys 载荷与进程自身 env 是
    //   **两段手写副本**（所以要数 2，缺一处就漏）；原生实现里 `--print` 与真跑
    //   **读同一个 `Plan`** ⇒ 那两段不可能分家。那条结构事实由后端侧
    //   `print_and_exec_cannot_drift_because_they_read_the_same_plan` 钉着。
    // ⚠ **按「整行」认，不用裸 `.includes("…")`**：后者的匹配单位（子串）比事实
    //   （那一行代码）小，把事实撑大的改动（`unset_config_dir: o.use_base && never()`）
    //   会从缝里溜过去而判据照样绿。`scanning-guard-registry.vitest.ts` 的递减棘轮
    //   盯的正是这一形 —— 本条第一版就是裸 `.includes`，被它当场逮住。
    const planLines = read("src/backend/control/ccm/plan.rs")
      .split("\n")
      .map((l) => l.trim());
    expect(
      planLines.filter((l) => l === "unset_config_dir: o.use_base,").length,
      "`--base` 不再逐字落到 `Plan.unset_config_dir` 上（要恰好一处）—— " +
        "本文件整套文案论证都建立在「它会清掉账号载体」上。它一变，上面几条要求的文案就成了新的假话。",
    ).toBe(1);
    expect(
      // `unset` 的写法搬进后端 OS 适配层（`platform::shell::posix::unset`），落点这一行跟着换形。
      planLines.filter((l) => l === "line.push_str(&posix::unset(&[cfg_env]));").length,
      "那条 `unset <账号载体>` 的渲染没了（要恰好一处）—— 同上。\n" +
        "（另有 `base-flag-contract-guard.vitest.ts` 从跨语言双写点那一面钉同一个事实。）",
    ).toBe(1);
  });
});

// 这里原来一张「挑号编排的两派调用点」登记表：那个编排随起会话判号下沉到会话所在那台的后端删了（界面只交「跟随 / 点名 / 账号 0」），表随之退休。
