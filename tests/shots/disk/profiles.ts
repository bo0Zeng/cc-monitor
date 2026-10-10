/**
 * 配置文件那一页的盘上原文（`~/.cc-monitor/profiles.toml` 与它旁边那两份记录）：照设计稿那台的清单（cc · cct 两条根，各号基于它们），
 * 按样子换（写错一段 · TOML 写坏 · 没有配置文件 · 刚迁移过 · 手改过）。读法与成品全在真后端（`assets/aliases/`），这里只写原文。
 */
import { HOME, putAccounts, type MachineDisk } from "./index";

export type ProfilesLook = "normal" | "broken" | "syntax" | "empty" | "migrated" | "edited" | "stale";

/** 那台账号库里的号（表单账号那一格的分段按钮照这张排）。 */
export const PROFILE_ACCOUNTS = ["a", "lab", "team", "work"];

const BOOK = (broken: boolean): string =>
  [
    "# 我的配置（cc · cct 两条根，各号基于它们）",
    "",
    "[cc]",
    'cwd-if = [["~", "~/projects/notes"]]',
    "",
    "[acc]",
    'from = "cc"',
    'account = "a"',
    "",
    "[teamcc]",
    'from = "cc"',
    'account = "team"',
    "",
    "[labcc]",
    'from = "cc"',
    'account = "lab"',
    "",
    "[cct]",
    'from = "cc"',
    "ccm-tmux = true",
    "",
    "[acct]",
    'from = "cct"',
    'account = "a"',
    "",
    "[teamcct]",
    'from = "cct"',
    broken ? 'tmux-sise = "200x50"' : 'account = "team"',
    "",
    "[workcct]",
    'from = "cct"',
    'account = "work"',
    "",
    "[workcc]",
    'from = "cc"',
    'account = "work"',
    "",
    "[codex]",
    'ccm-agent = "codex"',
    "",
    "[pcc]",
    "base = true",
    'cwd = "/srv/p"',
    "",
  ].join("\n");

/** 配置文件在家目录里的位置。 */
export const PROFILES_FILE = ".cc-monitor/profiles.toml";

/** 照设计稿那台的那一份原文（「保存时被别处改过」那一张在它后面补一行）。 */
export const staleBook = (): string => BOOK(false);

/** 那一台照这个样子放好配置文件（账号库换成表单里那几个号）。 */
export function putProfiles(d: MachineDisk, look: ProfilesLook): void {
  putAccounts(d, PROFILE_ACCOUNTS.map((name, i) => ({ name, kind: "sub", isDefault: i === 0 })));
  if (look === "empty") return;
  d.files[PROFILES_FILE] = look === "syntax" ? BOOK(false).replace("[pcc]", "[pcc") : BOOK(look === "broken");
  if (look === "edited") d.files[".cc-monitor/profiles-written.json"] = JSON.stringify({ fingerprint: "0-0000000000000000" }) + "\n";
  if (look === "migrated") d.files[".cc-monitor/profiles-migrated.json"] = JSON.stringify({ count: 11, path: `${HOME}/.cc-monitor/profiles.toml`, skipped: [] }) + "\n";
}
