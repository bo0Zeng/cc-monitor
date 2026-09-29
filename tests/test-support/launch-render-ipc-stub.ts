/**
 * `render_launch_payload`（那条 tauri 命令）在 vitest 里的**忠实最小镜像** —— 一个家。
 *
 * # 它为什么存在，以及它**不是**什么
 *
 * 前端那几套 vitest 桩掉 `@tauri-apps/api/core::invoke`，于是「后端渲出什么」要在 JS 侧给。
 * 步 22b·B（`设计/90 §4 E` 收官）把**外层 tmux 那三格**也切到这条命令之后，
 * `remote-launch-run.vitest.ts` 与 `send-into-backend.vitest.ts` 两处的桩**都得会拼外层** ——
 * 而那两处此前各有一份手写镜像（一份只会内层，一份干脆返回一个常量串）。
 *
 * 🔴 **两份手写镜像必漂**，这是本仓反复记过的形状（`K-R105` 那两把尺子、
 * `doc_claim_registry` 那一整套都是同一个病的产物）⇒ 收成这一份。
 *
 * ⚠⚠ **它不是第三份渲染实现，也不许被当成真相源。**
 * 「这条命令渲出来的字节对不对」由**入库夹具的跨语言逐字节对拍**钉着：
 * `src/backend/control/launch_render/fixtures/payload-golden.json`（内层，10 条）与
 * `fixtures/tmux-outer-golden.json`（外层三格，13 条）——
 * 左边是用例表里的手写期望（〔LR2〕原来是 TS 的真渲染器 + 真座，那一族删了），右边是 Rust 的生产命令。
 * 本文件只负责让 IPC 桩**吐出形状对的串**，好让上层那些「终端那条命令里有没有
 * `send-keys`／有没有 `attach`／会话名让到 `-2` 了没有」的判据判得动。
 *
 * ⇒ **一个字节的差异不许在这里被当成 bug 或被当成正确**。要改字节，改 Rust 渲染器 ＋ 用例表的手写期望 ＋ 重生成夹具。
 *
 * # 它**刻意**模拟的两道 fail-closed 闸（不是顺手加的）
 *
 * 真命令对「attach 那一格带了载荷」与「两层 cwd 同时送」是 `REFUSE:` 的
 *（`launch_wire.rs::render_launch_payload`，两条各有一条 Rust 判据）。
 * 这里照拼一遍，理由只有一条：**桩要是对这两种坏请求照渲**，
 * 那么上层任何一次「把格搞错了」的回归都会渲出一条看起来对的串、静默绿 ——
 * 那正是 22b·A 死值验 `M6` 逮到的那一形（判据喂的是渲染器本体，走不到 wire 那层）。
 * ⚠ **它不模拟别的闸**（空会话名 · `Raw` 越出白名单 · 控制符 · 越界 `@ccm_sid` ·
 * 空串 cwd · create/send-into 少送载荷）—— 那几道的判据在 Rust 那侧，
 * 上层 vitest 要验拒绝就直接 mock 一次 reject，别指望本文件替它拒。
 */
import type { PayloadRenderRequest, WireTmuxOuter } from "../../src/frontend/ui/launch-cli-wire.ts";

/** Rust 那侧 `payload::refuse()` 打的标。前端按它分流（`remote-launch-run.ts::REFUSE_TAG`）。 */
export const STUB_REFUSE_TAG = "REFUSE:";

/** POSIX 单引号包裹。与 `shell-quote.ts::posixQuote` / `shell_quote_core::posix_quote` 同形。 */
function q(v: string): string {
  return `'${v.split("'").join(`'\\''`)}'`;
}

type OuterTarget = { name: string; quoting: "raw" | "quoted" };

/** `new-session -s <名>` 收的是**名字**不是 target ⇒ 不加 `=`/`:`。 */
function token(t: OuterTarget): string {
  return t.quoting === "quoted" ? q(t.name) : t.name;
}

/** `-t` 一律精确匹配形态（`=名:`）—— 裸 `-t 名` 会命中前缀/glob。 */
function exact(t: OuterTarget): string {
  const marked = `=${t.name}:`;
  return t.quoting === "quoted" ? q(marked) : marked;
}

/** 内层载荷：`env 前缀 → [cd →] launcher + args`。 */
function renderInner(req: PayloadRenderRequest): string {
  const env = req.env
    .map((op) => {
      switch (op.kind) {
        case "export-config-dir":
          return `export CLAUDE_CONFIG_DIR=${q(op.value)}; `;
        case "export-model":
          return `export ANTHROPIC_MODEL=${q(op.value)}; `;
        // `设计/80 §8` 步 1。⚠ 桩**不模拟**真命令那道形状闸
        // （`payload.rs::rbind_token_shape_ok` ⇒ `REFUSE:`）—— 同本文件头注那条口径：
        // 它只模拟已登记的那两道，别的拒绝要验就直接 mock 一次 reject。
        case "export-rbind-token":
          return `export CCM_RBIND_TOKEN=${q(op.value)}; `;
        // 〔RL1〕同上口径：桩不模拟 `relay_base_url_shape_ok` 那道形状闸。
        // 〔RK1〕钥匙段是读钥匙文件的命令替换（与真命令 `payload.rs::relay_env_prefix_posix` 同形）。
        case "export-relay-base-url": {
          const m = /^(http:\/\/[^/]*\/)(.*)$/.exec(op.value);
          const [origin, path] = m ? [m[1], `/${m[2]}`] : [op.value, ""];
          return `export ANTHROPIC_BASE_URL=${q(origin)}"$(cat "$HOME/.cc-monitor/relay-key")"${q(path)}; `;
        }
        case "unset-config-dir":
          return "unset CLAUDE_CONFIG_DIR; ";
        case "unset-nested-env":
          return `unset ${req.nestedEnv.join(" ")}; `;
      }
    })
    .join("");
  // ★ **tmux 那两格的内层没有 `cd`** —— cwd 归外层的 `new-session -c`。
  //   真命令在 `outer` 非空时把顶层 cwd 强制当 `None`（并且两个都送会拒，见下）。
  const cd = req.outer === undefined && req.cwd ? `cd ${q(req.cwd)} && ` : "";
  return `${env}${cd}${[req.launcher, ...req.args].join(" ")}`;
}

function renderOuter(outer: WireTmuxOuter, payload: string | null): string {
  const t = exact(outer);
  switch (outer.mode) {
    case "attach":
      return `tmux attach -t ${t}`;
    case "send-into":
      return `tmux send-keys -t ${t} ${q(payload ?? "")} Enter; tmux attach -t ${t}`;
    case "create": {
      const cflag = outer.cwd !== null ? ` -c ${q(outer.cwd)}` : "";
      const setSid =
        outer.ccmSid !== null
          ? `(tmux set-option -t ${t} @ccm_sid ${outer.ccmSid} 2>/dev/null || true) && ` +
            `(tmux set-option -t ${t} set-titles on 2>/dev/null || true) && ` +
            `(tmux set-option -t ${t} set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && `
          : "";
      return (
        `tmux new-session -d -s ${token(outer)}${cflag} && ` +
        setSid +
        `tmux send-keys -t ${t} ${q(payload ?? "")} Enter && tmux attach -t ${t}`
      );
    }
  }
}

/**
 * 桩体：喂一个生产形状的 `req`，回一条形状对的命令串。
 *
 * **两道闸照真命令拒**（见顶注）：抛出来的串带 `REFUSE:` 标，
 * 与真命令经 IPC 传上来的错误同形 ⇒ 前端那条按标分流的逻辑在桩上也走得到。
 */
export function renderLaunchPayloadStub(req: PayloadRenderRequest): string {
  const outer = req.outer;
  if (outer !== undefined && outer.mode === "attach") {
    if (req.env.length > 0 || req.args.length > 0 || req.launcher !== "") {
      throw new Error(
        `${STUB_REFUSE_TAG} attach 那一格带了载荷字段（env / args / launcher）—— 请求形状对不上，拒。`,
      );
    }
    return renderOuter(outer, null);
  }
  if (outer !== undefined && req.cwd !== null) {
    throw new Error(
      `${STUB_REFUSE_TAG} 同时送了顶层 cwd 与外层容器 —— tmux 那两格的 cwd 归外层的 \`new-session -c\`，拒。`,
    );
  }
  const payload = renderInner(req);
  return outer === undefined ? payload : renderOuter(outer, payload);
}
