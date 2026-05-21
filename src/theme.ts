/**
 * 主题层：把用户偏好（颜色 / 字体 / 字号）持久化到 ~/.claude/claudecode-frontend/config.json
 * 并实时映射到 :root 上的 CSS 变量。
 *
 * 解耦原则：
 * - UI 层（settings/panel）只调本模块的 applyTheme / loadTheme / saveTheme，不直接碰 setProperty 或 IPC
 * - 本模块只调 config.ts 的 IPC 通道，不写死 invoke 命令名
 * - Rust config 命令不解释配置内容，只做原子 R/W
 */

import { loadConfig, saveConfig } from "./config";

/**
 * 主题可配置项 —— 与 styles.css :root 的令牌一一对应。
 * 命名沿用 CSS 变量名（去掉 `--` 前缀），方便 TOKEN_MAP 直接映射。
 */
export interface ThemeConfig {
  /* === 字体 === */
  "font-base"?: string;
  "font-mono"?: string;
  "font-size-base"?: number; // 单位 px，applyTheme 时自动补
  /* === 颜色 === */
  bg?: string;
  "bg-2"?: string;
  card?: string;
  text?: string;
  "text-2"?: string;
  user?: string;
  assistant?: string;
  success?: string;
  warn?: string;
  error?: string;
  live?: string;
}

/** key → CSS variable 名 + 是否需要带 px 单位 */
const TOKENS: ReadonlyArray<{ key: keyof ThemeConfig; cssVar: string; unit?: "px" }> = [
  { key: "font-base", cssVar: "--font-base" },
  { key: "font-mono", cssVar: "--font-mono" },
  { key: "font-size-base", cssVar: "--font-size-base", unit: "px" },
  { key: "bg", cssVar: "--bg" },
  { key: "bg-2", cssVar: "--bg-2" },
  { key: "card", cssVar: "--card" },
  { key: "text", cssVar: "--text" },
  { key: "text-2", cssVar: "--text-2" },
  { key: "user", cssVar: "--user" },
  { key: "assistant", cssVar: "--assistant" },
  { key: "success", cssVar: "--success" },
  { key: "warn", cssVar: "--warn" },
  { key: "error", cssVar: "--error" },
  { key: "live", cssVar: "--live" },
];

/** 把 theme 应用到 :root；undefined / 空串字段移除对应 CSS var，回退到 styles.css 默认 */
export function applyTheme(cfg: ThemeConfig): void {
  for (const { key } of TOKENS) {
    applyThemeToken(key, cfg[key] as string | number | undefined);
  }
}

/**
 * 单 token 增量应用。比 applyTheme 便宜 14 倍（不动其他 token）。
 *
 * 调用方场景：拖动 color picker / 输入数字字号时 `input` 事件高频触发。
 * 拖动 60Hz × applyTheme 全量 setProperty 14 次 = 全局重排被压垮 → 卡顿。
 * 改用 applyThemeToken 只动这一个，每帧仅触发该变量的下游样式重算。
 */
export function applyThemeToken(
  key: keyof ThemeConfig,
  value: string | number | undefined,
): void {
  const token = TOKENS.find((t) => t.key === key);
  if (!token) return;
  const root = document.documentElement;
  if (value === undefined || value === null || value === "") {
    root.style.removeProperty(token.cssVar);
    return;
  }
  const str = token.unit === "px" ? `${value}px` : String(value);
  root.style.setProperty(token.cssVar, str);
}

/** 启动时调用：从配置文件读取并应用 */
export async function loadTheme(): Promise<ThemeConfig> {
  try {
    const cfg = await loadConfig();
    const theme = (cfg.theme as ThemeConfig | undefined) ?? {};
    applyTheme(theme);
    return theme;
  } catch (e) {
    console.warn("loadTheme failed:", e);
    return {};
  }
}

/** 保存并应用。会合并到现有 config 的 theme 字段，保留其它字段。 */
export async function saveTheme(theme: ThemeConfig): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg.theme = theme;
  await saveConfig(cfg);
  applyTheme(theme);
}
