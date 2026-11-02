// ESLint flat config。**基线/顾问式**（同 CI 里 clippy 不强制）——
// 不追一次清零；既有告警作基线、`npm run lint` 本地看，CI 步骤 advisory 不阻断（见 ci.yml）。
// 非 type-checked 预设（不需 parserOptions.project）：快、且不因文件不在 tsconfig 里而报错。
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import globals from "globals";
import { importX, createNodeResolver } from "eslint-plugin-import-x";

const UI = "src/frontend/ui";

/**
 * 产品 TS 里「某件事只一个口」那几条：每一项是一个语法树形状（`selector`，进 `no-restricted-syntax`）或一条导入限制（`imports`，
 * 进 `no-restricted-imports`）· 一句报错 · 它的家（`homes`：住在那里的不报）· 射程（`ui`：只管 `src/frontend/ui/`，否则 `src/` 全部）。
 * 按语法树认：注释不算，别名导入照样认。`tests/frontend/ui/single-home-lint.vitest.ts` 证每一项会抓人、家里放过。
 */
function singleDoors() {
  const doors = [];
  const door = (message, { homes = [], ui = false }, ...selectors) => {
    for (const selector of selectors) doors.push({ selector, message, homes, ui });
  };
  const onlyImport = (message, homes, pattern) => doors.push({ imports: { ...pattern, message }, homes, ui: true });
  /** 标识符，或串 / 模板串里的整词。 */
  const words = (names) => [
    `Identifier[name=/^(?:${names})$/]`,
    `Literal[value=/\\b(?:${names})\\b/]`,
    `TemplateElement[value.cooked=/\\b(?:${names})\\b/]`,
  ];
  /** 串 / 模板串的值（整串或片段，看正则；正则里不许有 `/`）。 */
  const strings = (re) => [`Literal[value=/${re}/]`, `TemplateElement[value.cooked=/${re}/]`];

  // 剪贴板只经 `src/frontend/ui/clipboard.ts`（壳命令 `clipboard_write` · `clipboard_read` 回系统接口的真成败）；网页自己的剪贴板口
  //   在 WebView2 上剪贴板被占着时也回成功，界面就会说「已复制」而什么都没写进去。直呼那两条壳命令的只有它。
  const clip = "剪贴板只经 `writeClipboard` · `readClipboard`（src/frontend/ui/clipboard.ts）：网页自己的剪贴板口在 WebView2 上会假报成功。";
  door(clip, {}, "MemberExpression[property.name='clipboard']", "MemberExpression[property.value='clipboard']",
    "CallExpression[callee.property.name='writeText']", "CallExpression[callee.property.name='readText']");
  door(clip, { homes: [`${UI}/clipboard.ts`] }, "CallExpression[callee.property.name=/^clipboard_(?:write|read)$/]",
    "CallExpression[callee.name=/^clipboard_(?:write|read)$/]");
  // 换号重启整条在后端做：请求压缩用哪一句住后端适配层。
  door("请求压缩那一句住后端适配层（换号重启整条在后端做），界面不写它。", {}, "Literal[value='/compact']", "TemplateElement[value.cooked='/compact']");

  // 会话是哪一家、那一家的记录长什么样是后端的事（加一家 agent 只改后端 `agents/<名>/`）；界面只读后端给的成品（通用记录 `LineRecord`）。
  const agentFmt = "界面不认某一家的记录格式、不认「当前那一家」：会话是哪一家、记录长什么样由后端判，界面只读后端给的成品。";
  door(agentFmt, { ui: true }, ...words("ACTIVE_AGENT|AGENT_PROFILE|parentUuid|stop_reason|isApiErrorMessage"),
    ...strings("rollout-|<synthetic>"), ...strings("^(?:queue-operation|ai-title)$"));
  door("起法意图不带启动命令：它住那台的偏好（prefs.json），那台自己读。", { ui: true }, "Property[key.name='launcherOverride']");

  // 界面一个解码：线上来的经 `decodeJson`，本地 / 会话存储经 `parseStored`。
  door("JSON.parse 只在 ipc/decode.ts（线上来的字节；别处用 decodeJson）与 local-storage.ts（本地存储；别处用 parseStored）。",
    { ui: true, homes: [`${UI}/ipc/decode.ts`, `${UI}/local-storage.ts`] },
    "MemberExpression[object.name='JSON'][property.name='parse']", "MemberExpression[object.name='JSON'][property.value='parse']");
  // 一种订阅：「那台某样东西变了」只经 `events.ts` 订，设置几页调 `subscribeTopics`；`bindEvents` 只给主窗口。
  onlyImport("「那台某样东西变了」只经 events.ts 订（别处调 subscribeTopics）：changedItems 只有 events.ts 引。",
    [`${UI}/events.ts`], { regex: "(?:^|/)changed-stream(?:\\.ts)?$", importNames: ["changedItems"] });
  onlyImport("bindEvents 只给主窗口（main.ts）：设置几页经 subscribeTopics 订。",
    [`${UI}/main.ts`], { regex: "(?:^|/)events(?:\\.ts)?$", importNames: ["bindEvents"] });

  // 起会话交给终端的命令串只住后端（`render_launch_payload` / `render_ccm_launch`）；前端交结构化请求。
  door("前端不拼 shell 串（串里零 `&&` 与 `tmux <动词> -`）：交结构化请求给后端渲（session-open）。", {},
    ...strings("&&|\\btmux\\s+(?:-u\\s+)?(?:new-session|send-keys|attach|attach-session|kill-session|rename-session|set-option|has-session|capture-pane|list-sessions|switch-client)\\s+-"));
  door("TS 起会话兜底那一族已删：命令串只住后端。", {},
    "Identifier[name=/^(?:renderFallback|SESSION_BACKEND|TMUX_BACKEND|buildResumeDirectCmd|buildResumeTmuxCmd|buildResumeIntoExistingTmuxCmd|buildLauncherCmd|buildAttachCmd|buildOpenTerminalCmd|buildEnvPrefix|posixQuote)$/]");
  // 起会话的几件事各一个家：发请求 · 按码挑按钮 · 开那一行 · 交等报到住 `session-open.ts`；在哪个 tmux 会话里只问那台、一个问口。
  door("起会话 · 恢复 · 接回只经 session-open.ts 发 session-open。", { homes: [`${UI}/session-open.ts`] },
    "CallExpression[callee.name='callTyped'][arguments.1.value='session-open']");
  door("「在哪个 tmux 会话里」只经 sessions-where.ts 问那台。", { homes: [`${UI}/sessions-where.ts`] }, ...strings("^sessions-where$"));
  door("界面不拿 tmux 名单自己判、不判起会话挑哪个号：判定在那台后端（sessions-where · session-open）。", {},
    "Identifier[name=/^(?:listTmux|list_local_tmux|list_remote_tmux|withAccount|resolveLaunchAccount|localFollowPlan|followRecordName)$/]");
  door("线上 `kind: \"named\"`（点名的号）只从 launch-account.ts 出。", { homes: [`${UI}/launch-account.ts`] },
    "Property[key.name='kind'][value.value='named']");

  // 「是不是本机」只在 `ipc/origin.ts` 判（`isLocalOrigin` / `isRemoteOrigin`）：同一条规则两份实现会漂，漂开是静默的错。
  //   买不到：转一手再比（`const L = LOCAL; x === L`）· 查表 · `startsWith("<")` 之类的变形。
  const local = "「是不是本机」只在 ipc/origin.ts 判：用 isLocalOrigin / isRemoteOrigin，不直比 LOCAL / \"<local>\"。";
  const eq = "BinaryExpression[operator=/^[=!]==?$/]";
  door(local, { homes: [`${UI}/ipc/origin.ts`] },
    `${eq} > Identifier[name='LOCAL']`, `${eq} > MemberExpression[property.name='LOCAL']`,
    `${eq} > Literal[value='<local>']`, `${eq} > TemplateLiteral[expressions.length=0][quasis.0.value.cooked='<local>']`,
    "SwitchCase[test.name='LOCAL']", "SwitchCase[test.property.name='LOCAL']", "SwitchCase[test.value='<local>']");
  door("LOCAL 不起别名导入：别名直比认不出（「是不是本机」只在 ipc/origin.ts 判）。", {},
    "ImportSpecifier[imported.name='LOCAL'][local.name!='LOCAL']");

  // 会话此刻是什么样、能做什么只由那台后端判（整份脸 `SessionFace`）：界面没有转移表 · 落值口 · 两轴的格。
  door("会话状态只由那台后端判（SessionFace）：界面不写状态转移（转移表 · 落值口 · 判活 · 可恢复两轴）。", {},
    ...words("nextState|applyState|StateEvent|liveness|recoverability"));
  // 「在跑」只读一格：会话在不在跑只看后端写好的会话脸那一格 `life`，读它的只有 `face-can.ts` 那三个短名。
  door("「在跑」只读一格：会话一生（life）只在 face-can.ts 读，别处调 isLive · isUnclear · activityOf。",
    { homes: [`${UI}/face-can.ts`] },
    "MemberExpression[property.name='is'][object.name='life']", "MemberExpression[property.name='is'][object.property.name='life']",
    "MemberExpression[property.value='is'][object.property.name='life']");
  door("「在跑」只读一格：不拿状态点（dot === \"unknown\"）推在不在跑 / 说不清，调 face-can.ts 的 isUnclear。", {},
    "ConditionalExpression > BinaryExpression.test[operator=/^[=!]==?$/][left.name='dot'][right.value='unknown']",
    "ConditionalExpression > BinaryExpression.test[operator=/^[=!]==?$/][left.property.name='dot'][right.value='unknown']");

  // 一个组件一个家：对话框 · 弹出菜单 · 悬停提示 · toast 栈 · 点外面关 · 按钮 · 建元素的小函数只在 `kit/` 里建。
  const kit = [`${UI}/kit/**`];
  const kitRole = "/^(?:(?:alert)?dialog|menu(?:item\\w*)?|tooltip)$/";
  door("对话框 · 弹出菜单 · 悬停提示 · toast 栈 · 点外面关只在 kit/ 里建（kit/dialog · menu · tooltip · toast · popover）。", { homes: kit },
    `CallExpression[callee.property.name='setAttribute'][arguments.0.value='role'][arguments.1.value=${kitRole}]`,
    `AssignmentExpression[left.property.name='role'][right.value=${kitRole}]`,
    "Literal[value='aria-modal']", ...strings("toast-stack"),
    "CallExpression[callee.property.name='addEventListener'][callee.object.name=/^(?:document|window)$/][arguments.0.value='pointerdown']");
  // 设置页那套按钮类拼出来写：免得本文件自己成了全仓 grep 的一处命中。
  door("按钮只经 kit/button.ts：设置页那套按钮类已删。", { homes: kit }, ...strings(["settings", "btn"].join("-")));
  door("建元素只经 kit/el.ts 的 el()：kit 外不另写本地 el。", { homes: kit },
    "FunctionDeclaration[id.name='el']", "VariableDeclarator[id.name='el'][init.type=/^(?:Arrow)?FunctionExpression$/]");

  // 账号可用性只读那台后端写好的 `selectable`（规则住 `acct_core::account_selectable`）：鉴权那几格只在 `accounts.ts` 出现。
  door("账号可用性只读那台写好的 selectable（规则住 acct_core::account_selectable）：loggedIn · authReady · authKind 只在 accounts.ts 出现。",
    { homes: [`${UI}/accounts.ts`] }, ...words("loggedIn|authReady|authKind"));
  // INVARIANTS §12：关键失败要有 toast / banner。`confirm` · `prompt` 问的是要不要干，不在这一条里。
  door("错误反馈不用 alert（可能没看清就关掉）：用 toast / banner（INVARIANTS §12）。", {},
    "CallExpression[callee.name='alert']", "CallExpression[callee.object.name='window'][callee.property.name='alert']");
  return doors;
}

/**
 * `singleDoors` 摊成几块配置。同一条规则后一块整份盖掉前一块的选项 ⇒ 不能一项一块：界面以外的 `src/` 一块（只拿全仓那几项）、
 * 界面里没有家的文件一块、每个家一块（只少禁住在它那里的那几项）。
 */
function singleDoorBlocks() {
  const doors = singleDoors();
  const rules = (ds) => {
    const patterns = ds.filter((d) => d.imports).map((d) => d.imports);
    return {
      "no-restricted-syntax": ["error", ...ds.filter((d) => d.selector).map(({ selector, message }) => ({ selector, message }))],
      ...(patterns.length ? { "no-restricted-imports": ["error", { patterns }] } : {}),
    };
  };
  const homes = [...new Set(doors.flatMap((d) => d.homes))];
  return [
    { files: ["src/**/*.ts"], ignores: [`${UI}/**`], rules: rules(doors.filter((d) => !d.ui)) },
    { files: [`${UI}/**/*.ts`], ignores: homes, rules: rules(doors) },
    ...homes.map((h) => ({ files: [h], rules: rules(doors.filter((d) => !d.homes.includes(h))) })),
  ];
}

export default tseslint.config(
  {
    // Rust、产物、依赖、覆盖率报告、各类 config 自身不 lint。
    ignores: [
      // C01：ts-rs 生成物，没人该手动去修它（Phase D 审计 S5）
      "src/frontend/ui/generated/**",
      "dist/**",
      // 🔴 2026-09-18：构建输出已统一改到 `.build/`（`vite.config.ts` 的 `outDir` ＋
      //   两份 `.cargo/config.toml` 的 `target-dir`）⇒ 上面那条 `dist/**` 从此**罩不住任何东西**。
      //   漏掉这一条的症状：`npx eslint .` 从 7 个错涨到 **4092 个**，其中 4085 个全在
      //   `.build/dist/assets/*.js`（打包产物）与 `.build/shell/**/out/*.js`（Tauri 生成的 API 壳）。
      //   这正是本文件下面那段注释预言的形状：「每新增一个带脚本的目录，洞就复发一次」。
      ".build/**",
      // 施工纪律第 21 条把各路编译产物放 `<工作树>/.scratch/` ⇒ 同一个洞第二次复发：
      //   cargo 在那里生成的一份 js 被 `eslint .` 扫进来，`eslint-baseline` 假红（多数一条）。
      ".scratch/**",
      "node_modules/**",
      "src/frontend/shell/**",
      "src/backend/**",
      "coverage/**",
      "*.config.js",
      "*.config.ts",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // 前端源码：浏览器全局（DOM/window/document）。
    // 〔src/test 分离〕`tests/**` 一并纳入 —— vitest 跑在 jsdom 里，测试文件同样用
    // `document`/`window`。漏掉它会让 18 条 `no-undef` 一次性涌进来（实测 7 → 25）。
    files: ["src/**/*.ts", "tests/**/*.ts"],
    languageOptions: {
      globals: { ...globals.browser },
    },
    rules: {
      // 对齐既有 `_`-前缀「有意不用」约定（tsconfig 的 noUnusedParameters 本就认它）——
      // 这是配置正确性、非改代码：不 lint 掉作者刻意留的占位绑定。
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
          destructuredArrayIgnorePattern: "^_",
        },
      ],
    },
  },
  // 产品 TS 里「某件事只一个口」那几条（`singleDoors` 那张表）：编进 lint 规则，`eslint .` 的基线是 0 ⇒ 别处写一处就红。
  ...singleDoorBlocks(),
  // 产品 TS 的 import 图（`tests/frontend/ui/import-graph-lint.vitest.ts` 证这两条会抓人）：
  // - 零运行期环：环两头的用点今天都在函数体里不炸，换个求值顺序就是 TDZ。只算运行期边：`import type` 编译期擦除，
  //   `import-x/no-cycle` 不跟它走（整句 type-only、逐项全是 type 的都不算）。`export … from` 算边；
  //   只为副作用的 `import "./x"` 它也不算（本仓只入口文件这样写，入口没人 import）。
  // - 主窗口的视图（`views/`）与设置窗（`settings/`）互不引用：只一边用的挪到那一边，两边都用的挪到 `src/frontend/ui/` 那一层。
  // 解析照 TS：带 `.ts` 后缀的与不带的都认，目录取 `index.ts`；裸包名是外部依赖，不进图。
  {
    files: ["src/**/*.ts"],
    plugins: { "import-x": importX },
    settings: {
      "import-x/resolver-next": [createNodeResolver({ extensions: [".ts", ".js"] })],
      "import-x/extensions": [".ts", ".js"],
      "import-x/parsers": { "@typescript-eslint/parser": [".ts"] },
    },
    rules: {
      "import-x/no-cycle": ["error", { ignoreExternal: true }],
      "import-x/no-restricted-paths": [
        "error",
        {
          zones: [
            { target: "./src/frontend/ui/views", from: "./src/frontend/ui/settings", message: "主窗口的视图不引设置窗：两边都用的挪到 src/frontend/ui/ 那一层。" },
            { target: "./src/frontend/ui/settings", from: "./src/frontend/ui/views", message: "设置窗不引主窗口的视图：两边都用的挪到 src/frontend/ui/ 那一层。" },
          ],
        },
      ],
    },
  },
  {
    // 测试文件（*.vitest.ts）：补 node 全局。
    files: ["tests/**/*.vitest.ts"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
  {
    // 判据里不睡定时长：睡一会儿再判，机器一慢就红、一快就漏。等一件事 ⇒ `vi.waitFor` 等它本身；判到点前后 ⇒ 假时钟拨到界上两侧；
    //   让别的先走 ⇒ `setTimeout(…, 0)` 让一拍。
    files: ["tests/**/*.vitest.ts", "tests/test-support/**/*.ts"],
    rules: {
      "no-restricted-syntax": [
        "error",
        ...["setTimeout", "setInterval"].flatMap((name) => {
          const notZero = ":not([arguments.1.type='Literal'][arguments.1.value=0])";
          const message = `判据里不睡定时长（\`${name}\` 只许 0）：等事件用 \`vi.waitFor\`，判到点前后用假时钟。`;
          return [
            { selector: `CallExpression[callee.name='${name}'][arguments.length>1]${notZero}`, message },
            { selector: `CallExpression[callee.property.name='${name}'][arguments.length>1]${notZero}`, message },
          ];
        }),
      ],
    },
  },
  {
    // E83（2026-08-01）：`tests/e2e/` 下那些 `.mjs`（wdio 配置、restart-shims、spec）**此前从没被 lint 过**。
    //
    // 病灶不是「它们脏」，是**作用面与配置意图对不上**：本文件的 `ignores` 明明是**仓级**的
    // （逐条列出 dist / node_modules / src/frontend/shell / src/backend / coverage），
    // 而 `npm run lint` 只跑 `eslint src` ⇒ 那份仓级意图从来没被兑现，
    // `npx eslint .` 是 46 个告警、比 `eslint src` 的 7 个多出 39 个，
    // **全在这几个文件里、全是 `no-undef: process/console/it`（纯缺一段 globals）**。
    // 也就是说：39 个告警永远不会被任何人看到，而它们一个真问题都不是。
    //
    // ⇒ 补上 globals（node + wdio 的 mocha 风格全局），并把 `npm run lint` 放开到 `eslint .`。
    // 补完实测：全仓当时是 7 个，与 `eslint src` 的基线**一致** —— 基线数字不变，覆盖面变大。
    // 那天剩三处：`src/frontend/ui/settings/machine-aliases.ts` 里那四处空 catch（原「终端集成」
    //   记住上次选择的 localStorage 写）随那段代码删了（`tests/frontend/ui/eslint-baseline.vitest.ts` 的基线常量同拍 7 → 3）。
    // 那天剩两处：`src/frontend/ui/shell-quote.ts` 那处 `no-control-regex` 随 `isValidConfigDir` 删了（基线常量同拍 3 → 2）。
    // 那天剩一处：写类工具口径的前端那份测试随被测对象（搬进后端）整份删了，
    //   它那一处未用变量跟着没了（合并 DUP1 时按「基数 3 ＋ 两边各 −1」算成 1，eslint 现打核过）。
    // 〔发版后〕今天全仓 0 个：`render.ts` 那处 `no-control-regex`（代码 stub 的 `\u0000` 哨兵）换成非字符 `\uFDD0`。
    files: ["tests/e2e/**/*.mjs"],
    languageOptions: {
      globals: {
        ...globals.node,
        ...globals.mocha, // wdio 的 describe/it/before（`tests/e2e/tier2/**`）
        // `browser.execute(() => …)` 的**函数体在页面里跑**，所以 DOM 全局在这里是真实存在的
        // （不是漏声明）。wdio 自己注入的 `browser`/`$`/`$$` 同理。
        ...globals.browser,
        browser: "readonly",
        $: "readonly",
        $$: "readonly",
      },
    },
    rules: {
      // 与 `src/**` 同一条 `_`-前缀「有意不用」约定 —— 那条规则原本只挂在 `src/**/*.ts` 上，
      // 于是 shim 里刻意留的 `(_body, _opts)` 占位在这边会被判违规。约定该跟着仓走，不跟着目录走。
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
          destructuredArrayIgnorePattern: "^_",
        },
      ],
    },
  },
  {
    // `scripts/` 下的 `.mjs` **掉进了 E83 修过的同一个洞**。
    //
    // ⚠ 时间线是这条的全部要害（`git log` 实测，不是推的）：
    // E83 在 **07-31**（`a02f340`）把 `npm run lint` 从 `eslint src` 放开到 `eslint .`，
    // 当场实测「全仓当时是 7 个，与 `eslint src` 的基线一致」；而 `tests/scripts/assert-coverage-floors.mjs`
    // 是 **08-06**（`cab8a75`）才新建的 —— **晚 6 天**。它一进来就带 7 条 `no-undef`
    // （`console`/`process`，纯缺一段 globals），基线**从 7 静默变成 14**，
    // 而 `eslint.config.js` 与 `ci.yml` 里那两句「全仓实测（当时）仍是 7 项」**没人回来改**。
    //
    // ⇒ 这不是「E83 修漏了」，是**修法本身的形状问题**：`ignores` 是仓级的（全集），
    // 而 globals 是**按目录枚举**的（`src/**`、`tests/e2e/**`）—— 于是每新增一个带脚本的目录，
    // 洞就复发一次，且**没有任何判据数着那个基线**（对比 shellcheck 的文件数被
    // `shell_lint_registry` 钉成等号、e2e 套数被 `e2e_gate_registry` 四份副本对拍）。
    // 补这一块只是止血；钉住「下次再有新目录」那半在 `eslint-baseline.vitest.ts`。
    //
    // ★ 最该记住的一点：那 7 条**长在覆盖率门禁自己的执行体上** ——
    // 替我们数别人的那个脚本，自己没被数。
    files: ["tests/scripts/**/*.mjs"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
  {
    // `tests/evidence/` 下的 `.mjs`（如 `cdp.mjs`：经 WebView2 远程调试口在真页里求值的台架）
    // 同上面那个洞：新目录没有 globals 块，`no-undef: process/console/fetch/WebSocket` 一齐顶高基线。照上一块止血。
    files: ["tests/evidence/**/*.mjs"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
  {
    // 截图工具（`npm run shots`）的 Node 那一半：起 vite / 浏览器 / Xvfb、出图集。
    files: ["tests/shots/**/*.mjs"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
  {
    // 传输台架（冷开 · 延迟限速代理）的 Node 那一半。
    files: ["tests/benches/**/*.mjs"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
  {
    // 性能台架注进页面里跑的那两段（probe.js · profile.js）：浏览器那一半。
    files: ["tests/shots/perf/**/*.js"],
    languageOptions: {
      globals: { ...globals.browser },
    },
  },
);
