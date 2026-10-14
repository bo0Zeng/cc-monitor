/**
 * 「需手动」浮层与登录抽屉：浮层的几种情形 · 抽屉从起登录到各种结局 · 别的入口。
 *
 * 会话外的需手动清单（`manual-list`）与登录那三条（`login-start` · `login-finish` · `login-cancel`）都是场景合成的答法；
 * 不起任何登录程序。号名 · 服务器名 · 链接全是编的。
 */
import type { Scene } from "./index";
import { Refuse, type World } from "../fake/types";
import { defaultWorld, LOCAL } from "../fake/world";
import { putAccounts } from "../disk";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { byText, click, mainReady, sleep, waitFor } from "./helpers";

const ALL_TABS = 7;

function scene(id: string, title: string, desc: string, act: Scene["act"], world: () => World, page: Scene["page"] = "index", size: [number, number] = [1280, 800]): Scene {
  return { id, page, dir: "需手动与登录", title, desc, width: size[0], height: size[1], world, act };
}

const acctFix = (account: string) => ({ command: "login-start", args: { kind: "account", agent: "claude", account } });
const mcpFix = (account: string, server: string) => ({ command: "login-start", args: { kind: "mcp", agent: "claude", account, server, projectDir: null } });
const acctItem = (account: string) => ({ kind: "accountLogin", agent: "claude", account, said: copyText("beManual.list.accountLogin", { account }), fix: acctFix(account) });
const mcpItem = (account: string, server: string) => ({ kind: "mcpLogin", account, server, scope: "user", said: copyText("beManual.list.mcpLogin", { server, account }), fix: mcpFix(account, server) });

/** 默认世界 ＋ 各台会话外的清单（按 origin；没写 ⇒ 空）。 */
function manualWorld(lists: Record<string, unknown[] | Refuse>): () => World {
  return () => {
    const w = defaultWorld();
    w.ops["manual-list"] = (origin) => {
      const got = lists[origin] ?? [];
      if (got instanceof Refuse) throw got;
      return { items: got };
    };
    return w;
  };
}

const busyWorld = manualWorld({ [LOCAL]: [acctItem("team"), mcpItem("work", "linear")] });

/** 点开「需手动」。 */
async function openPanel(): Promise<void> {
  await mainReady(ALL_TABS);
  await sleep(400);
  await click(".tab-needs");
  await waitFor('[role="dialog"] [role="listbox"]');
  await sleep(600);
}

/** 点开浮层里会话外那一行的［登录…］（第 `nth` 个）。 */
async function loginFromPanel(nth = 0): Promise<void> {
  await openPanel();
  document.querySelectorAll<HTMLElement>('[data-action="manual-login"]')[nth]!.click();
  await waitFor("aside[data-login-kind]");
}

/** 登录那三条的答法：起登录回 `started`（或抛）· 交码照 `finish` · 撤单照答。 */
function loginWorld(started: Record<string, unknown> | Refuse, finish: (() => unknown) | null = null, base: () => World = busyWorld): () => World {
  return () => {
    const w = base();
    w.ops["login-start"] = () => {
      if (started instanceof Refuse) throw started;
      return started;
    };
    w.ops["login-finish"] = () => (finish ? finish() : { target: { kind: "account", agent: "claude", account: "team" } });
    w.ops["login-cancel"] = () => ({ was: true });
    return w;
  };
}

const STARTED = { job: "job-1", url: "https://claude.example/oauth/authorize?code=true&client_id=demo&state=demo-state", paste: "code", auto: true };
const MCP_STARTED = { job: "job-2", url: "https://mcp.linear.example/authorize?state=demo-state&redirect_uri=http%3A%2F%2Flocalhost%3A41873%2Fcallback", paste: "redirectUrl", auto: false };

/** 往粘贴那一格里填一段（照人粘进去那样派 input）。 */
function fill(text: string): void {
  const input = document.querySelector<HTMLInputElement>('aside[data-login-kind] [data-part="paste"] input')!;
  input.value = text;
  input.dispatchEvent(new Event("input"));
}

async function submit(): Promise<void> {
  await click('aside[data-login-kind] [data-action="login-submit"]');
  await sleep(700);
}

/** 设置窗：点导航里的一页。 */
async function go(...ids: string[]): Promise<void> {
  await waitFor(".settings-nav");
  await sleep(600);
  for (const id of ids) {
    await click(`#${CSS.escape(`settings-tab-${id}`)}`);
    await sleep(700);
  }
  await sleep(600);
}

export const LOGIN_SCENES: Scene[] = [
  scene("needs-panel", "需手动浮层 · 有事", "点「需手动 4」：在等的两个会话（照旧的序）· 分隔线 · 会话外两项（号 / MCP 图标 · 后端写好的那一句 · 机器 ·［登录…］）", openPanel, busyWorld),
  scene(
    "needs-panel-offline",
    "需手动浮层 · 一台断开",
    "devbox 那一问够不着：底部一条中性灰虚线「devbox 离线 · 未计入」，数里不算它",
    openPanel,
    () => {
      const w = busyWorld();
      w.hopDown = { devbox: ["manual-list"] };
      return w;
    },
  ),
  scene(
    "needs-panel-unreadable",
    "需手动浮层 · 清单无法读取",
    "gpu-01 回了话、账号库无法解析（回码 unreadable）：入口不带数字、虚线框；面板照常列读到的，底部一条虚线说哪台无法读取（原话在悬停里）",
    openPanel,
    manualWorld({ [LOCAL]: [acctItem("team")], "gpu-01": new Refuse("unreadable", copyText("beAccountsQuery.loadManifest.unreadable", { path: "/home/user/.cc-monitor/accounts/accounts.json", why: "expected value at line 3 column 5" })) }),
  ),
  scene(
    "needs-panel-nofix",
    "需手动浮层 · 没有解法的那一项",
    "devbox 上 codex 那个 MCP 不在 app 里登：第二行写后端给的 why，不画按钮",
    openPanel,
    manualWorld({ [LOCAL]: [acctItem("team")], devbox: [{ kind: "mcpLogin", account: null, server: "notion", scope: "user", said: copyText("beManual.list.mcpLoginBare", { server: "notion" }), fix: null, why: copyText("beManual.list.mcpNoFix", { agent: "Codex" }) }] }),
  ),
  scene(
    "needs-panel-cleared",
    "需手动浮层 · 刚清空",
    "浮层开着时最后一项解决了（会话外那份重问回来空了、会话都答完了）：一行「无需手动事项」，入口去掉数字、不自己关",
    async (ctx) => {
      await openPanel();
      for (const id of ["manual"]) ctx.backend.pushChanged(LOCAL, id, {});
      await sleep(900);
    },
    () => {
      const w = manualWorld({ [LOCAL]: [acctItem("team")] })();
      // 会话都不在等；会话外那份第一问有一项、之后（推了「需手动变了」再问）就空了。
      for (const s of w.sessions) if (s.activity === "needs_you") s.activity = "idle";
      let asked = 0;
      w.ops["manual-list"] = (origin) => ({ items: origin === LOCAL && asked++ < 2 ? [acctItem("team")] : [] });
      return w;
    },
  ),
  scene("login-account-starting", "登录抽屉 · 账号 · 起登录中", "点 team 那一行的［登录…］：抽屉先开、login-start 在路上，［打开登录页］转圈", async () => {
    await loginFromPanel(0);
    await sleep(500);
  }, () => {
    const w = loginWorld(STARTED)();
    w.opDelayMs = { "login-start": 60_000 };
    return w;
  }),
  scene("login-account-auto", "登录抽屉 · 账号 · 有链接、会自己结束", "本机连本机（browserHere）⇒ auto：［打开登录页］·［复制链接］· 截断的链接 · 「登录页完成后自动结束」· 授权码那一格当后备", async () => {
    await loginFromPanel(0);
    await waitFor('aside[data-login-kind] [data-part="paste"]');
    await sleep(500);
  }, loginWorld(STARTED)),
  scene("login-mcp-paste", "登录抽屉 · MCP · 粘了回跳地址", "linear 那一行：标题「登录 MCP 服务器」· 目标行服务器名 · 号 · 机器；auto 为假不画自动结束；回跳地址那一格（悬停 ⓘ 是定义）粘好了", async () => {
    await loginFromPanel(1);
    await waitFor('aside[data-login-kind] [data-part="paste"]');
    fill("http://localhost:41873/callback?code=demo-code&state=demo-state");
    await sleep(500);
  }, loginWorld(MCP_STARTED)),
  scene("login-submitting", "登录抽屉 · 提交中", "［提交］转圈、那一格只读（login-finish 在路上）", async () => {
    await loginFromPanel(0);
    await waitFor('aside[data-login-kind] [data-part="paste"]');
    fill("Xk2pQ9-demo#Qm7x");
    await submit();
  }, () => {
    const w = loginWorld(STARTED)();
    w.opDelayMs = { "login-finish": 60_000 };
    return w;
  }),
  scene("login-bad-paste", "登录抽屉 · 粘的不对", "again: paste ⇒ 那一格下面一行红字，照回包那一句；作业还在，底部仍是［取消］", async () => {
    await loginFromPanel(0);
    await waitFor('aside[data-login-kind] [data-part="paste"]');
    fill("Xk2pQ9-demo");
    await submit();
  }, loginWorld(STARTED, () => {
    throw new Refuse("bad_paste", copyText("beLogin.finish.badPaste"), { again: "paste" });
  })),
  scene("login-restart", "登录抽屉 · 没成、要重来", "again: restart ⇒ 粘贴区收起、红条写后端那一句 ＋［重新开始］·［复制详情］，底部［关闭］", async () => {
    await loginFromPanel(0);
    await waitFor('aside[data-login-kind] [data-part="paste"]');
    fill("Xk2pQ9-demo");
    await submit();
  }, loginWorld(STARTED, () => {
    throw new Refuse("refused", copyText("beLogin.finish.refused"), { again: "restart" }, "OAuth error: invalid_grant");
  })),
  scene("login-none", "登录抽屉 · 不支持", "again: none（login-start 回 unsupported）⇒ 只有那一句，不给重来，底部［关闭］", async () => {
    await loginFromPanel(0);
    await waitFor("aside[data-login-kind] [data-again]");
    await sleep(400);
  }, loginWorld(new Refuse("unsupported", copyText("beLogin.agent.unsupported", { agent: "codex" }), { again: "none" }))),
  scene(
    "login-from-acct",
    "别的入口 · 账号页需登录那一行",
    "devbox 的账号页：team 没登录那一行右边［登录…］（「⋯」里另有「重新登录…」「在终端里登录…」）",
    async () => {
      await go("machine:devbox", "machine:devbox#acct");
      await waitFor('.settings-page:not([hidden]) [data-action="acct-login"]');
      await sleep(500);
    },
    () => {
      const w = defaultWorld();
      putAccounts(w.disk.devbox, [
        { name: "work", kind: "sub", isDefault: true },
        { name: "team", kind: "sub", isDefault: false, signedIn: false },
        { name: "lab", kind: "sub", isDefault: false },
      ]);
      return w;
    },
    "settings",
    [960, 740],
  ),
  scene(
    "login-from-ext",
    "别的入口 · 扩展页 MCP 那一行",
    "github 那一行点开：本机那一行「需登录 · 号 · 几点记下」＋ 每个要登的号一颗［登录…］（原来的「去登录 · 复制 /mcp」没了）",
    async () => {
      await go("ext");
      await sleep(600);
      await click(await byText(".settings-page:not([hidden]) .ext-filter button", "MCP"));
      await sleep(300);
      document.querySelector<HTMLElement>('.settings-page:not([hidden]) .ext-row[data-key="mcp/github"]')?.click();
      await waitFor('.settings-page:not([hidden]) [data-action="mcp-login"]');
      await sleep(500);
    },
    manualWorld({ [LOCAL]: [mcpItem("personal", "github"), mcpItem("work", "github")] }),
    "settings",
    [960, 740],
  ),
];
