/**
 * F48：SFTP 面板的纯路径逻辑(零 DOM/IPC,便于 vitest)。远端路径恒用 `/`(SFTP 约定)。
 */

/** 目录项(镜像 Rust sftp_pool::SftpEntry,camelCase)。 */
export interface SftpEntry {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  lossyName: boolean;
}

/** 面包屑:绝对路径 → 段 + 累积路径(根为 `/`)。用于路径条点跳祖先。 */
export function breadcrumbs(path: string): { name: string; path: string }[] {
  const norm = normalize(path);
  const out: { name: string; path: string }[] = [{ name: "/", path: "/" }];
  if (norm === "/") return out;
  let acc = "";
  for (const seg of norm.split("/").filter(Boolean)) {
    acc += `/${seg}`;
    out.push({ name: seg, path: acc });
  }
  return out;
}

/** 归一:去重复/尾随 `/`(根保留单个 `/`);空/相对 → 原样加前导按调用方保证。 */
export function normalize(path: string): string {
  const collapsed = path.replace(/\/+/g, "/");
  if (collapsed === "/") return "/";
  return collapsed.replace(/\/$/, "");
}

/** 目录 + 名 → 绝对路径(根特判,避免 `//name`)。 */
export function joinPath(dir: string, name: string): string {
  const d = normalize(dir);
  return d === "/" ? `/${name}` : `${d}/${name}`;
}

/** 父目录:去掉最后一段;根/单段 → `/`。 */
export function parentPath(path: string): string {
  const norm = normalize(path);
  const i = norm.lastIndexOf("/");
  if (i <= 0) return "/";
  return norm.slice(0, i);
}

export type SortBy = "name" | "size" | "type";

/** 本地重排:恒目录在前,再按 by。返回新数组(不改入参)。 */
export function sortEntries(list: SftpEntry[], by: SortBy): SftpEntry[] {
  const cmpName = (a: SftpEntry, b: SftpEntry) =>
    a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  return [...list].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1; // 目录恒在前
    if (by === "size") return b.size - a.size || cmpName(a, b);
    if (by === "type") {
      const ext = (n: string) => n.slice(n.lastIndexOf(".") + 1).toLowerCase();
      return ext(a.name).localeCompare(ext(b.name)) || cmpName(a, b);
    }
    return cmpName(a, b);
  });
}

/** 路径的最后一段(basename)。用于上传时从本地路径取文件名。 */
export function basename(path: string): string {
  const norm = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const i = norm.lastIndexOf("/");
  return i >= 0 ? norm.slice(i + 1) : norm;
}

/** 传输 id(F47 要求全局唯一,防取消注册表串味)。 */
export function newTransferId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `t-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
