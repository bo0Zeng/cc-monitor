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
 * - 远端 shell 里若有 `cc-acct-iso shellinit` 生成的 `export CLAUDE_CONFIG_DIR=<某账号>`，
 *   「不注入」会继承它、「unset」落回 `~/.claude` —— **两者落到的是不同的账号**。
 *
 * ★ 最能说明问题的一点：`machine-card.ts` 那句文案**上一行的注释一直是对的**
 * （「用远端 `~/.claude` 那套基座凭据，不受当前账号影响」）。
 * 写代码的人知道，**给用户看的那句没跟上**。
 *
 * ⚠ 兜底渲染路（不发 `--base`）才是**真的不注入**。文案按**主路径**写，这一点写进两处代码注释。
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";

const REPO = resolve(__dirname, "..");
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
  return [...prod.matchAll(/"([^"\\\n]*不指定账号[^"\\\n]*)"/g)].map((m) => m[1]);
}

/** 「不指定账号」这句话的两份副本。加第三份时把它登记进来。 */
const COPIES: ReadonlyArray<readonly [file: string, where: string]> = [
  ["src/settings/machine-card.ts", "机器卡片的新会话账号下拉，空选项"],
  ["src/launch-menu.ts", "resume 浮层的账号修饰选项"],
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

  it("★ 文案赖以成立的那个事实还在：ccm 收到 --base 会 unset（两处）", () => {
    const ccm = read("shared/ccm");
    const unsets = [...ccm.matchAll(/use_base"?\]?\s*=\s*1\s*\]\s*&&[^\n]*unset CLAUDE_CONFIG_DIR/g)];
    expect(
      unsets.length,
      "在 `shared/ccm` 里找不到「`--base` ⇒ unset CLAUDE_CONFIG_DIR」那两处 —— " +
        "本文件整套文案论证都建立在它上面。它一变，上面几条要求的文案就成了新的假话。\n" +
        "（另有 `base-flag-contract-guard.vitest.ts` 从跨语言双写点那一面钉同一个事实。）",
    ).toBe(2);
  });
});

/**
 * `withAccount` 的调用点分两派：**传 `follow`** 与**不传**。
 * 不传 ⇒ `resolveAccount` 直接 `{kind:"base"}`（`accounts.ts:621`），**完全不看当前账号**。
 *
 * ⚠ 「哪一派才对」**今天没有共识**（那正是功能件 §4 判定不硬做的那件事）。
 * 本表**不裁决**，只做两件事：把分歧记下来，并让**下一个新调用点必须表态**。
 */
const WITH_ACCOUNT_SITES: ReadonlyArray<
  readonly [file: string, count: number, follow: boolean, why: string]
> = [
  [
    "src/tabs.ts",
    3,
    true,
    "tab 上的 resume/重启：有 sid，能读到 lastAccount ⇒ 跟随（上次用哪个号就还用哪个）。",
  ],
  [
    "src/views/history.ts",
    2,
    true,
    "历史里拉起旧会话：同上，有 sid 就跟随；其中一处 `{ follow: {} }` 是「跟随但没有 pin」。",
  ],
  [
    "src/settings/machine-card.ts",
    1,
    false,
    "★ **唯一不传的一派**：机器卡片建**新**会话，没有 sid ⇒ 没有 lastAccount 可跟随。" +
      "但它也**不看当前账号** —— 同一个应用里「没指定账号」在这里和别处含义相反。" +
      "⚠ 这是报告 I-13 点名的分歧，**本区不裁决**（改它是语义重定义、且没有共识）；" +
      "登记在此，免得下一个人以为这里是漏写的。",
  ],
] as const;

describe("withAccount 的两派调用点（audit-0805 F12：记下分歧，不裁决）", () => {
  /**
   * 扫**全仓**（不是只扫已登记的那几个文件）。
   *
   * ⚠ 第一版只遍历 `WITH_ACCOUNT_SITES` 里的文件 —— 那样**新文件里冒出来的调用点会静默绿**，
   * 而这张表存在的全部理由就是「让新调用点表态」。变异实测（往未登记文件里加一处）把它照出来了。
   */
  function scanAll(): Map<string, number> {
    const out = new Map<string, number>();
    const walk = (dir: string): void => {
      for (const name of readdirSync(dir)) {
        const p = join(dir, name);
        if (statSync(p).isDirectory()) {
          walk(p);
          continue;
        }
        if (!name.endsWith(".ts")) continue;
        if (name.includes(".vitest.") || name.includes(".test.") || name.endsWith(".d.ts")) continue;
        const src = readFileSync(p, "utf8")
          .split("\n")
          .filter((l) => {
            const t = l.trimStart();
            return !(t.startsWith("//") || t.startsWith("*") || t.startsWith("/*"));
          })
          // 定义处自己不算调用点。
          .filter((l) => !/function withAccount\(/.test(l))
          .join("\n");
        const n = (src.match(/withAccount\(/g) ?? []).length;
        if (n > 0) out.set(p.slice(REPO.length + 1).replace(/\\/g, "/"), n);
      }
    };
    walk(join(REPO, "src"));
    return out;
  }

  it("★ 全仓扫描：新增一处调用点必须回来表态（登记表不是豁免清单）", () => {
    const found = scanAll();
    const total = [...found.values()].reduce((a, b) => a + b, 0);
    // 抽取器自检：**只判下界**。判等值的话「多了一处」会被误报成「抽取器坏了」——
    // 变异实测踩过，**诊断说的和实际发生的是两回事**（F01 Phase D 的教训）。
    expect(
      total,
      `全仓只数到 ${total} 处 withAccount 调用（08-05 实测 6）—— 抽取器坏了，下面的对拍会零命中地绿`,
    ).toBeGreaterThanOrEqual(6);

    const HINT =
      "\n★ 那正是该表态的时刻：这一处**传不传 `follow`**？\n" +
      "  传 ⇒ 跟随 lastAccount → 当前账号 → 基座（有 sid 的路子）\n" +
      '  不传 ⇒ 直接 `{kind:"base"}`，**完全不看当前账号**（`accounts.ts:621`）\n' +
      "⚠ 哪一派才对**今天没有共识**（报告 I-13），本表不裁决 —— 但新调用点不许悄悄挑一边。";

    const registered = new Map(WITH_ACCOUNT_SITES.map(([f, n]) => [f, n]));
    const unregistered = [...found].filter(([f]) => !registered.has(f));
    expect(
      unregistered.map(([f, n]) => `${f}（${n} 处）`),
      `有 withAccount 调用点没登记：${HINT}`,
    ).toEqual([]);

    for (const [file, want] of registered) {
      expect(found.get(file), `${file} 的 withAccount 调用处数变了（表里 ${want}）。${HINT}`).toBe(
        want,
      );
    }
  });

  it("★ 两派都还在 —— 只剩一派时这张表就该退休，而分歧也就该被裁决了", () => {
    const yes = WITH_ACCOUNT_SITES.filter(([, , f]) => f).length;
    const no = WITH_ACCOUNT_SITES.filter(([, , f]) => !f).length;
    expect(
      yes > 0 && no > 0,
      `登记表里只剩一派（传 ${yes} 条 / 不传 ${no} 条）—— ` +
        "分歧没了就把这张表删掉，并把结论写进 `accounts.ts::resolveAccount` 的头注。",
    ).toBe(true);
  });

  it("不传 follow 那一派必须写清「为什么不传」，不许只登记文件名", () => {
    for (const [file, , follow, why] of WITH_ACCOUNT_SITES) {
      if (follow) continue;
      expect(why.includes("没有 sid") || why.includes("新"), `${file} 没说清为什么不传：「${why}」`).toBe(
        true,
      );
      expect(why.length, `${file} 的理由太短，像是占位`).toBeGreaterThan(40);
    }
  });
});
