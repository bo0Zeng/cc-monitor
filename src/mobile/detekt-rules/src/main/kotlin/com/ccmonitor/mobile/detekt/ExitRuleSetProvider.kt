package com.ccmonitor.mobile.detekt

import dev.detekt.api.Config
import dev.detekt.api.Entity
import dev.detekt.api.Finding
import dev.detekt.api.Rule
import dev.detekt.api.RuleName
import dev.detekt.api.RuleSet
import dev.detekt.api.RuleSetId
import dev.detekt.api.RuleSetProvider
import org.jetbrains.kotlin.psi.KtFile
import java.io.File
import java.net.URI

/**
 * 出口扫描判据（`exit`）：[ExitScan] 的两句，每份生产文件的命中数要等于欠账名单 `tests/frontend/ui/exit-debt.json` 登的那一格
 * （多一处 ⇒ 报；少了 ⇒ 也报，名单要跟着改小）。名单是三门语言共用的一份，名单自身与合计由 `exit-judgment-scan.vitest.ts` 核。
 * 仓根从被查的那份文件往上找（有 `src/shared/copy/table.json` 的那一层）；找不到 ⇒ 报，不当零。
 */
class ExitRuleSetProvider : RuleSetProvider {
    override val ruleSetId = RuleSetId("exit")

    override fun instance(): RuleSet =
        RuleSet(
            ruleSetId,
            mapOf(
                RuleName("ExitCopyKey") to { c: Config -> ExitCopyRule(c, "key") },
                RuleName("ExitCopyBranch") to { c: Config -> ExitCopyRule(c, "branch") },
            ),
        )
}

class ExitCopyRule(
    config: Config,
    private val which: String,
) : Rule(config, "出口取文只许骨架那几档、不许在按码判的分支里取文；命中数照欠账名单", URI("https://github.com/")) {
    override fun visit(root: KtFile) {
        val path = File(root.viewProvider.virtualFile.path).absoluteFile
        if (!path.invariantSeparatorsPath.contains("/src/main/")) return
        val repo = Ledger.repoOf(path)
        if (repo == null) {
            report(Finding(Entity.atPackageOrFirstDecl(root), "找不到仓根（往上没有 ${Ledger.TABLE}）—— 判不了"))
            return
        }
        val ledger = Ledger.of(repo)
        val rel = path.relativeTo(repo).invariantSeparatorsPath
        val hits = ExitScan.scan(root, ledger.table)
        val got = if (which == "key") hits.key else hits.branch
        Ledger.mismatch(rel, which, got, ledger.debt)?.let { report(Finding(Entity.atPackageOrFirstDecl(root), it)) }
    }
}

/** 文案表与欠账名单（每个仓根读一次）。 */
class Ledger(
    val table: Map<String, ExitScan.Entry>,
    val debt: Map<String, Int>,
) {
    companion object {
        const val TABLE = "src/shared/copy/table.json"
        const val DEBT = "tests/frontend/ui/exit-debt.json"

        private val cache = mutableMapOf<File, Ledger>()

        fun repoOf(file: File): File? = generateSequence(file.parentFile) { it.parentFile }.firstOrNull { File(it, TABLE).isFile }

        @Synchronized
        fun of(repo: File): Ledger = cache.getOrPut(repo) { parse(File(repo, TABLE).readText(), File(repo, DEBT).readText()) }

        @Suppress("UNCHECKED_CAST")
        fun parse(
            tableJson: String,
            debtJson: String,
        ): Ledger {
            val entries = (Json.parse(tableJson) as Map<String, Any?>)["entries"] as Map<String, Map<String, Any?>>
            val table = entries.mapValues { (_, e) -> ExitScan.Entry(e["kind"] as String, (e["args"] as? List<*>)?.size ?: 0) }
            // 名单按 kind 分组，一格是 `[文件, 句, 命中数]`。
            val groups = (Json.parse(debtJson) as Map<String, Any?>)["groups"] as List<Map<String, Any?>>
            val debt =
                groups
                    .flatMap { it["cells"] as List<List<Any?>> }
                    .associate { "${it[0]} · ${it[1]}" to (it[2] as Number).toInt() }
            return Ledger(table, debt)
        }

        /** 这一格对不上 ⇒ 说怎么对不上；对得上 ⇒ null。 */
        fun mismatch(
            rel: String,
            rule: String,
            got: Int,
            debt: Map<String, Int>,
        ): String? {
            val k = "$rel · $rule"
            val want = debt[k] ?: 0
            return when {
                got == want -> null
                want == 0 -> "$k：命中 $got 处，名单里没有 —— 出口新取了描述数据的字（交核心写好那一格，出口照摆）"
                got < want -> "$k：命中 $got 处，名单登 $want —— 收掉了就把名单那一格改成实数（一处都没有了就删掉那一格）"
                else -> "$k：命中 $got 处，名单登 $want —— 多出来的那几处交核心写"
            }
        }
    }
}

/** 读名单与文案表用的最小 JSON 读法（对象 · 数组 · 字 · 数 · true / false / null）。 */
internal object Json {
    fun parse(s: String): Any? {
        val p = P(s)
        val v = p.value()
        p.ws()
        require(p.i == s.length) { "JSON 末尾多了东西（第 ${p.i} 个字）" }
        return v
    }

    private class P(
        val s: String,
    ) {
        var i = 0

        fun ws() {
            while (i < s.length && s[i].isWhitespace()) i++
        }

        fun value(): Any? {
            ws()
            return when (val c = s[i]) {
                '{' -> obj()
                '[' -> arr()
                '"' -> str()
                't' -> lit("true", true)
                'f' -> lit("false", false)
                'n' -> lit("null", null)
                else -> if (c == '-' || c.isDigit()) num() else error("JSON 第 $i 个字认不出：$c")
            }
        }

        fun lit(
            w: String,
            v: Any?,
        ): Any? {
            require(s.startsWith(w, i)) { "JSON 第 $i 个字该是 $w" }
            i += w.length
            return v
        }

        fun num(): Number {
            val st = i
            while (i < s.length && (s[i].isDigit() || s[i] in "-+.eE")) i++
            val t = s.substring(st, i)
            return t.toLongOrNull() ?: t.toDouble()
        }

        fun str(): String {
            i++
            val b = StringBuilder()
            while (s[i] != '"') {
                if (s[i] == '\\') {
                    i++
                    when (val e = s[i]) {
                        'n' -> b.append('\n')
                        't' -> b.append('\t')
                        'r' -> b.append('\r')
                        'b' -> b.append('\b')
                        'f' -> b.append('\u000c')
                        'u' -> {
                            b.append(s.substring(i + 1, i + 5).toInt(16).toChar())
                            i += 4
                        }
                        else -> b.append(e)
                    }
                } else {
                    b.append(s[i])
                }
                i++
            }
            i++
            return b.toString()
        }

        fun arr(): List<Any?> {
            i++
            val out = mutableListOf<Any?>()
            ws()
            if (s[i] == ']') {
                i++
                return out
            }
            while (true) {
                out.add(value())
                ws()
                if (s[i++] == ']') return out
            }
        }

        fun obj(): Map<String, Any?> {
            i++
            val out = linkedMapOf<String, Any?>()
            ws()
            if (s[i] == '}') {
                i++
                return out
            }
            while (true) {
                ws()
                val k = str()
                ws()
                require(s[i++] == ':') { "JSON 第 $i 个字该是冒号" }
                out[k] = value()
                ws()
                if (s[i++] == '}') return out
            }
        }
    }
}
