package com.ccmonitor.mobile.core.remote

/**
 * POSIX shell 单引号转义，全仓唯一实现。放在中立叶子模块 `core-remote`，core-claude 与 app 都看得见。
 *
 * 整串裹单引号，串内的 `'` 换成 `'\''`（闭合 → 转义单引号 → 重开）。
 * 结果对任意输入都是一个 shell 词，不会被 word-split、不会触发 glob/变量展开。
 *
 * ```
 * shellQuote("main")  == "'main'"
 * shellQuote("a b")   == "'a b'"
 * shellQuote("it's")  == "'it'\''s'"
 * ```
 *
 * 注意：被引用的值里 `~` / `$HOME` 不会展开。需要远端展开的表达式（如默认 `$CLAUDE_CONFIG_DIR`）
 * 走 [ShellWord.configDir] 的双引号分支。
 *
 * 这是 POSIX sh 方言；PowerShell 的转义规则不同（`'` → `''`），见 `ShellDialect`，别用这个函数。
 */
fun shellQuote(s: String): String = "'" + s.replace("'", "'\\''") + "'"

/**
 * 一个已经能原样拼进 POSIX sh 命令行的词。「已经 quote 过」由类型担保：
 * 接 [ShellWord] 的函数原样拼，接 `String` 的函数当字面路径 quote，两者混用编译不过，
 * 防止对已 quote 的词再包一层（那样远端会去开一个名字里带引号、`${…}` 没展开的文件）。
 *
 * 只有三种造法（构造器私有）：[literal]（字面 → 单引号）、[configDir]（该走哪种引号的判定）、
 * [plus]（在词后面接一段字面尾巴）。所以拿到一个 [ShellWord] 就意味着它是一个安全的词。
 */
@JvmInline
value class ShellWord private constructor(
    val text: String,
) {
    /** 拼进命令行是它唯一的用途 ⇒ 字符串模板里直接用（`"cat $word"`）。 */
    override fun toString(): String = text

    /**
     * 在这个词后面**接一段字面尾巴**，结果仍是**同一个词**（相邻的引号段在 shell 里连成一个词）。
     *
     * 尾巴只含 `[A-Za-z0-9/._-]` 时原样接（如 `"<表达式>"/projects/-x/<编号>.jsonl`）；
     * 否则整段 [shellQuote]，绝不让一段没引的尾巴把命令拆开（编号若带了空格或 `;`，原样接就是注入）。
     */
    operator fun plus(literalTail: String): ShellWord =
        ShellWord(text + if (BARE_SAFE.matches(literalTail)) literalTail else shellQuote(literalTail))

    companion object {
        private val BARE_SAFE = Regex("[A-Za-z0-9/._-]*")

        /** 双引号里有特殊含义的字符 —— 进双引号的尾巴不许带。 */
        private const val SPECIAL_IN_DOUBLE_QUOTES = "\"$`\\"

        /** 字面路径 → 词（单引号；`~`、`$HOME` 一律不展开）。 */
        fun literal(s: String): ShellWord = ShellWord(shellQuote(s))

        /**
         * 配置目录该走哪种引号，全仓唯一的判定。
         *
         * - [dir] 正是 [trustedDefault]（写死的 shell 表达式，如 `${CLAUDE_CONFIG_DIR:-$HOME/.claude}`）
         *   ⇒ 双引号：变量交给远端 shell 展开，展开后家目录带空格也不被拆开。
         *   不能走 [shellQuote]：单引号里 `${…}` 不展开，拼出来的是一个必然 no-such-file 的字面名。
         * - 否则（设置里填的）⇒ 字面路径 ⇒ [shellQuote]。填的值里 `~`/`$HOME` 不展开，覆写要填绝对路径。
         *
         * [innerTail] 落在引号里面（`"<表达式>/projects"`、`'<路径>/projects'`）。
         * 它只许是调用方自己的常量：双引号里 `"` `$` `` ` `` `\` 都有特殊含义，出现就当场炸。
         */
        fun configDir(
            dir: String,
            trustedDefault: String,
            innerTail: String = "",
        ): ShellWord {
            require(innerTail.none { it in SPECIAL_IN_DOUBLE_QUOTES }) { "innerTail 要进双引号，不许带 $SPECIAL_IN_DOUBLE_QUOTES：$innerTail" }
            return if (dir == trustedDefault) ShellWord("\"$dir$innerTail\"") else literal(dir + innerTail)
        }
    }
}
