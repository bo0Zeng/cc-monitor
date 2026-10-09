/**
 * 〔vitest `setupFiles`〕测试页里也有壳注入的那一份本机能力：缺省是 Linux 那一行（jsdom 的页从前按 UA 也被认成 Linux）。
 * 单条测试要别的平台 ⇒ `__setHostFactsForTests(factsOn(…))`。
 */
import { factsOn } from "./host-facts";

if (typeof window !== "undefined") (window as { __CCM_HOST__?: unknown }).__CCM_HOST__ = factsOn("linux");
