/**
 * 🔴 〔`设计/99 §2.5 P12` 2026-09-21〕**「配置里出现不认识的键」必须出声**，
 * 以及那一刀的由头 —— 落盘键 `forceLegacyLaunchRenderer` → `forceLaunchPayloadRenderer`。
 *
 * # 这一族守的是什么
 *
 * `P12` 逐字：「真正的毛病不是改名，是**未知键静默忽略** —— 那是本仓的头号病形
 *（『关掉了』与『过了』在终端上一模一样）。」
 * 又逐字：「**旧键出现时必须有一条会红的判据**，否则『出声』这件事本身没人守。」
 *
 * ⇒ 本文件就是那条。它**不 mock `src/config.ts`** —— 只 mock 最外面那层 `ipc/commands`，
 * 于是 `loadConfig` / `getBehavior` / `setBehavior` 走的是**生产那条真链**。
 * （`判据不在执行链上就等于不存在`：把 `config.ts` mock 掉，就等于把被守的东西换成了替身。）
 *
 * # 射程：它盖不到什么
 *
 * - **不判用户那一侧真不真的显示出来**。那条在 `tests/settings/unknown-keys-notice.vitest.ts`
 *   （真渲染设置面板，去 DOM 里找那个键名）。两条缺一不可：这里证明「数得出来」，
 *   那里证明「说出来了」。
 * - **不盖「一个新模块开始用 config.json 却没登记」**。今天没有目录遍历的余量
 *   （`tests/scanning-guard-registry.vitest.ts` 的 `WALKER_CEILING` 已经顶格），
 *   所以下面的对拍是**逐个登记的主人**去核，不是全树扫。
 *   ⚠ 但那种漏登记**不是静默的**：漏了的键会当场被当成未知键、指名喊出来。
 *   吵，但不瞎 —— 失效方向是对的。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync, existsSync } from "node:fs";

const store = vi.hoisted(() => ({
  cfg: {} as Record<string, unknown>,
  saved: [] as Record<string, unknown>[],
}));

vi.mock("../src/ipc/commands", () => ({
  commands: {
    load_config: vi.fn(async () => store.cfg),
    save_config: vi.fn(async ({ value }: { value: Record<string, unknown> }) => {
      store.saved.push(value);
      store.cfg = value;
    }),
  },
}));

import {
  CONFIG_KEY_OWNERS,
  KNOWN_CONFIG_KEYS,
  loadConfig,
  unknownConfigKeys,
  unknownKeysIn,
  __resetUnknownConfigKeysForTests,
} from "../src/config";
import { getBehavior, setBehavior } from "../src/behavior";

/** 退役的那个旧名字。**逐字**写在这里 —— 本条判据的全部意义就是它出现时会红。 */
const RETIRED_KEY = "forceLegacyLaunchRenderer";
/** 它今天的名字。 */
const LIVE_KEY = "forceLaunchPayloadRenderer";

const sorted = (xs: readonly string[]): string[] => [...xs].sort();

/**
 * 🔴 **config.json 顶层键的第二份普查 —— 刻意手写，刻意不从 `config.ts` 派生。**
 *
 * 它存在的唯一理由是**不让那条相等变成恒真**：如果这一份也从 `KNOWN_CONFIG_KEYS`
 * 生成，那么「登记表缩水」时两边会一起缩，判据照样绿 ——
 * 本工作区吃过这个亏（「恒等两侧同源会恒真」）。
 *
 * 这一份的来源是**现打的反向全扫**（2026-09-21）：`src/` 下每个 `import … from "./config"`
 * 的模块，逐个读它对 `cfg[…]` / `cfg.…` 的顶层存取。Rust 侧另读三个
 *（`claudeDir` / `showBgSessions` / `remote`），都已在列。
 */
const CENSUS: readonly string[] = [
  "theme", // src/theme.ts
  "claudeDir", // src/paths.ts
  "keybindings", // src/keybindings/store.ts
  "accounts", // src/accounts.ts
  "backendPolicy", // src/backend-policy.ts
  "remote", // src/remote-config.ts
  "tabBar", // src/tab-bar-state.ts
  "tabCollections", // src/tab-collections.ts
  "contextLimits", // src/usage-hud.ts（只读）
  "autoFollowUserActive", // ── src/behavior.ts 那一族 ──
  "bringMonitorToFrontOnUserActive",
  "showBgSessions",
  "resumeCommandLocal",
  "resumeCommandRemote",
  "resumeCommandLocalPresets",
  "resumeCommandRemotePresets",
  "notifyTurnEnd",
  "forceLaunchPayloadRenderer",
];

beforeEach(() => {
  store.cfg = {};
  store.saved = [];
  __resetUnknownConfigKeysForTests();
});

describe("P12 ① 落盘键改名：`forceLegacyLaunchRenderer` → `forceLaunchPayloadRenderer`", () => {
  it("★ 新名字读得出来（盘上写 true ⇒ 逃生口真的开）", async () => {
    store.cfg = { [LIVE_KEY]: true };
    expect(
      (await getBehavior()).forceLaunchPayloadRenderer,
      "新落盘键读不出来 —— 改名把这个逃生口改没了",
    ).toBe(true);
  });

  it("★★ 旧名字**不留别名**：盘上写 true 也不驱动任何行为", async () => {
    store.cfg = { [RETIRED_KEY]: true };
    expect(
      (await getBehavior()).forceLaunchPayloadRenderer,
      "旧键还在被当成开关读 —— `no-legacy-compat` 要的是**退役**，不是双名并存。\n" +
        "（留别名等于这个名字永远改不完：两个住址都能开同一个开关，下一个人不知道该改哪个。）",
    ).toBe(false);
  });

  it("★★ 写盘写的是新名字，而且**只写行为那一族**（两向集合相等）", async () => {
    const before = await getBehavior();
    await setBehavior(before);
    expect(store.saved.length, "`setBehavior` 根本没写盘").toBe(1);
    const written = sorted(Object.keys(store.saved[0]));
    const owned = sorted(
      KNOWN_CONFIG_KEYS.filter((k) => CONFIG_KEY_OWNERS[k] === "src/behavior.ts"),
    );
    expect(
      written,
      "`setBehavior` 写下的顶层键 ≠ 登记给 `src/behavior.ts` 的那一族。\n" +
        "两个方向都要看：多写了 = 它在动不属于自己的键；少写了 = 登记表里有个键没人写，" +
        "而那种键会在下次读盘时被当成未知键喊出来。",
    ).toEqual(owned);
    expect(written).toContain(LIVE_KEY);
    expect(
      written,
      `写盘里还有旧键 \`${RETIRED_KEY}\` —— 改名只改了读的那一半`,
    ).not.toContain(RETIRED_KEY);
  });

  it("写盘**不吞掉**用户手写的未知键（未知 ≠ 该删）", async () => {
    store.cfg = { [RETIRED_KEY]: true, 某个错字: 1 };
    await setBehavior(await getBehavior());
    expect(
      Object.keys(store.saved[0]),
      "保存设置时把认不出的键顺手删了 —— 用户手写的东西被我们悄悄吃掉，" +
        "这比静默忽略更糟：连让他自己发现的机会都没有了",
    ).toContain(RETIRED_KEY);
  });
});

describe("P12 ② 未知键要被数出来（`unknownKeysIn` / `loadConfig` 那一格）", () => {
  it("🔴 旧键出现 ⇒ 数得出来，而且逐字就是它", () => {
    expect(
      unknownKeysIn({ [RETIRED_KEY]: true, theme: {} }),
      `盘上放了退役键 \`${RETIRED_KEY}\` 而这里一个都没数出来 —— 「静默忽略」原样还在`,
    ).toEqual([RETIRED_KEY]);
  });

  it("🔴 旧键出现 ⇒ **读盘那一步**就记下来了（在执行链上，不是纯函数自娱自乐）", async () => {
    store.cfg = { theme: {}, [RETIRED_KEY]: true };
    await loadConfig();
    expect(
      unknownConfigKeys(),
      "`loadConfig()` 读完之后那格快照是空的 —— 提示条到时候会无话可说，" +
        "而「没话说」与「配置很干净」在屏幕上一模一样",
    ).toEqual([RETIRED_KEY]);
  });

  it("★ 反空真正控：把 [`CENSUS`] 那 18 个键整份喂进去 ⇒ 一个未知键都不许报", () => {
    const full: Record<string, unknown> = {};
    for (const k of CENSUS) full[k] = true;
    expect(
      unknownKeysIn(full),
      "真实用户会写的那 18 个键里，有的被当成了未知键 —— 这把尺子会对着正常配置喊，" +
        "而一把乱喊的尺子用不了几天就会被关掉",
    ).toEqual([]);
  });

  it("★ 空配置 / 非对象都不报（那不是「多写了一个键」）", () => {
    expect(unknownKeysIn({})).toEqual([]);
    expect(unknownKeysIn(null)).toEqual([]);
    expect(unknownKeysIn([1, 2])).toEqual([]);
    expect(unknownKeysIn(7)).toEqual([]);
  });

  it("多个未知键按**盘上的顺序**全报，不截断", () => {
    expect(
      unknownKeysIn({ zz: 1, theme: {}, aa: 2, bb: 3 }),
      "少报了一个 —— 用户改完被报出来的那个，剩下的仍然在静默失效",
    ).toEqual(["zz", "aa", "bb"]);
  });
});

describe("P12 ③ 登记表自己得是真的（否则上面每一条都在拿一张假名单对拍）", () => {
  it("★★ 登记表 == [`CENSUS`]（两向集合相等，**两份各自手写**）", () => {
    expect(
      sorted(KNOWN_CONFIG_KEYS),
      "`src/config.ts` 的登记表与本文件这份独立普查对不上。\n" +
        "少了 ⇒ 那个键从今天起会被当成未知键，对着一份正常配置喊；\n" +
        "多了 ⇒ 表上挂着一个谁都不写也不读的名字，而它会让真正的错字蒙混过去。\n" +
        "⚠ 这两份**必须各自手写**：从同一个来源派生出来的「相等」恒真，" +
        "而恒真与真绿在终端上一模一样。",
    ).toEqual(sorted(CENSUS));
  });

  it("分母：登记了多少个主人", () => {
    const owners = new Set(Object.values(CONFIG_KEY_OWNERS));
    expect(owners.size, `主人只剩 ${owners.size} 个（现打 10）`).toBe(10);
  });

  it("★ 每个登记的主人文件真的在盘上，而且那个键名逐字出现在它里面", () => {
    const missing: string[] = [];
    for (const [key, owner] of Object.entries(CONFIG_KEY_OWNERS)) {
      if (!existsSync(owner)) {
        missing.push(`${key} → ${owner}（文件不在）`);
        continue;
      }
      // ⚠ 参数是变量不是字面量 —— 本仓 `scanning-guard-registry` 数的是
      //   「磁盘语料上的裸 `.includes("…")`」，那一格是棘轮且已顶格。
      if (!readFileSync(owner, "utf8").includes(key)) {
        missing.push(`${key} → ${owner}（文件里找不到这个键名）`);
      }
    }
    expect(
      missing,
      "登记表指着一个不存在的住址，或者主人那边已经把键名改了而表没跟上。\n" +
        "后果是实的：表里那个名字从此**没人写也没人读**，而用户盘上真正的那个键\n" +
        "会被当成未知键喊出来 —— 喊的是对的，但喊的理由是我们自己的表烂了。\n" +
        `逐条：\n${missing.join("\n")}`,
    ).toEqual([]);
  });

  it("★★ `src/behavior.ts` 那一族：登记表 == 它自己 `const KEY_… = \"…\"` 的全集（两向）", () => {
    const src = readFileSync("src/behavior.ts", "utf8");
    const declared = sorted([...src.matchAll(/const KEY_\w+ = "([^"]+)";/g)].map((m) => m[1]));
    const registered = sorted(
      KNOWN_CONFIG_KEYS.filter((k) => CONFIG_KEY_OWNERS[k] === "src/behavior.ts"),
    );
    expect(declared.length, "一个 `const KEY_… =` 都没抽出来 —— 抽取器坏了，下面那条会空绿").toBeGreaterThan(0);
    expect(
      declared,
      "`behavior.ts` 声明的落盘键与 `config.ts` 登记的那一族对不上。\n" +
        "两个方向都要看：那边加了个键没登记 ⇒ 它自己会被当未知键喊；\n" +
        "这边登记了那边没有 ⇒ 表上挂着一个谁都不认的名字。",
    ).toEqual(registered);
  });
});
