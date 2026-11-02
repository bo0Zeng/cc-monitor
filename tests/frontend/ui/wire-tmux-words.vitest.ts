/**
 * 线上的 tmux 词只在 tmux 专有那几格（A3-3-7）：两种宿主（tmux · 托管）都一样的地方说「终端」。
 *
 * 扫两样：
 * - 生成的 TS 线上类型（`src/frontend/ui/generated/`，剥了注释）里每一个带 tmux 的词（格名 · 字面量 · 码）；
 * - 命令表 `registry/` 每一条命令的说明串（剥了注释的串字面量）。
 * 名单 `TMUX_ONLY` 一项一个「文件：词」，各写为什么是 tmux 专有；名单里不在了的那一项也红（删了就从名单删）。
 */
import { describe, it, expect } from "vitest";
import { basename } from "node:path";
import { sourceFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";

const GENERATED = "src/frontend/ui/generated";
const REGISTRY = "src/backend/stream/inbound/registry";

/** 生成的线上类型里许出现 tmux 的那几格：`文件:词` ⇒ 为什么只跟 tmux。 */
const TMUX_ONLY: Record<string, string> = {
  "Launch.ts:tmux": "放在哪的一形：tmux",
  "TerminalHost.ts:tmux": "终端宿主的一形：tmux",
  "SessionContainer.ts:tmux": "会话容器的一形：在 tmux 里",
  "ForkTerminal.ts:tmux": "分叉源会话所在宿主的一形：tmux",
  "TypedCommands.ts:no_tmux": "这台没装 tmux",
  "TypedCommands.ts:tmux_too_old": "这台的 tmux 版本太旧",
  "TypedCommands.ts:bad_tmux_name": "新建 tmux 会话的名字不合 tmux 的规矩",
  "TypedCommands.ts:tmux_taken": "新建 tmux 会话的名字已被占",
  "StandingWord.ts:no_tmux": "这台没装 tmux（会话在哪那一格）",
  "NewOpen.ts:tmuxName": "新建 tmux 会话的名字",
  "DirChecked.ts:tmuxName": "这台会给新 tmux 会话铸的名字",
  "OpenField.ts:tmuxName": "起会话框上 tmux 会话名那一格",
  "ResumeWhere.ts:tmux": "恢复时放在哪的一形：tmux",
  "DataReport.ts:tmux": "这台装没装 tmux（数据页）",
  "SessionWindow.ts:tmux": "↗ 切窗口要不要切 tmux 窗格",
  "Showing.ts:tmux": "会话在不在 tmux 里（出口据它给不给［在终端里打开］）",
  "ProfileAccountShape.ts:tmux": "启动配置里的 tmux 那一格",
  "ProfileForm.ts:ProfileTmuxPick": "启动配置里的 tmux 那一格",
  "ProfileForm.ts:tmux": "启动配置里的 tmux 那一格",
  "ProfileForm.ts:tmuxSize": "启动配置里新建 tmux 会话的尺寸",
  "ProfilesBook.ts:tmux": "这台装没装 tmux（启动配置「在哪起」选不选得了 tmux）",
  "ProfilesBook.ts:ProfileTmuxNaming": "启动配置里 tmux 那一格的三种取名说明",
  "ProfilesBook.ts:tmuxNaming": "启动配置里 tmux 那一格的三种取名说明",
  "ProfileTmuxNaming.ts:ProfileTmuxMode": "启动配置里的 tmux 那一格",
  "ProfileTmuxNaming.ts:ProfileTmuxNaming": "启动配置里 tmux 那一格的三种取名说明",
  "ProfileSlotId.ts:tmux": "启动配置里的 tmux 那一格",
  "ProfileSlotId.ts:tmuxSize": "启动配置里新建 tmux 会话的尺寸",
  "ProfileTmuxMode.ts:ProfileTmuxMode": "启动配置里的 tmux 那一格",
  "ProfileTmuxPick.ts:ProfileTmuxMode": "启动配置里的 tmux 那一格",
  "ProfileTmuxPick.ts:ProfileTmuxPick": "启动配置里的 tmux 那一格",
};

/** 命令说明串里许提 tmux 的：命令说明里的那一段 ⇒ 为什么。 */
const TMUX_ONLY_SAID: Record<string, string> = {
  "tmux 那一形": "订画面在 tmux 那一形多一个只读客户端，会触发用户自己的钩子（托管那一形没有）",
};

const words = (s: string): string[] => [...new Set(s.match(/[A-Za-z0-9_-]*tmux[A-Za-z0-9_-]*/gi) ?? [])];

function generatedHits(): string[] {
  const out: string[] = [];
  for (const { file, text } of sourceFiles(GENERATED, { ext: ".ts", shallow: true })) {
    const f = basename(file);
    const code = stripComments(text, "ts");
    for (const w of words(code)) out.push(`${f}:${w}`);
  }
  return out;
}

/** 说明串里带 tmux 的每一条（去掉名单里许的那几段之后还带 tmux 的）。 */
function saidHits(): { stray: string[]; used: Set<string> } {
  const stray: string[] = [];
  const used = new Set<string>();
  for (const { file, text } of sourceFiles(REGISTRY, { ext: ".rs", shallow: true })) {
    const f = basename(file);
    const code = stripComments(text, "rust");
    for (const lit of code.match(/"(?:[^"\\]|\\.)*"/g) ?? []) {
      if (!/tmux/i.test(lit)) continue;
      let rest = lit;
      for (const ok of Object.keys(TMUX_ONLY_SAID)) {
        if (rest.includes(ok)) used.add(ok);
        rest = rest.split(ok).join("");
      }
      if (/tmux/i.test(rest)) stray.push(`${f}: ${lit}`);
    }
  }
  return { stray, used };
}

describe("线上的 tmux 词只在 tmux 专有那几格", () => {
  it("生成的线上类型：带 tmux 的词都在名单里", () => {
    const stray = generatedHits().filter((h) => !(h in TMUX_ONLY));
    expect(stray, "两种宿主通用的格说「终端」；真是 tmux 专有的，进名单并写为什么").toEqual([]);
  });

  it("名单里每一项此刻都还在（不在了就删）", () => {
    const hits = new Set(generatedHits());
    expect(Object.keys(TMUX_ONLY).filter((k) => !hits.has(k))).toEqual([]);
  });

  it("命令说明串：只在 tmux 那一形的话里提 tmux", () => {
    const { stray, used } = saidHits();
    expect(stray, "说明串两种宿主都成立就不提 tmux").toEqual([]);
    expect(Object.keys(TMUX_ONLY_SAID).filter((k) => !used.has(k)), "名单里没用上的那一段").toEqual([]);
  });
});
