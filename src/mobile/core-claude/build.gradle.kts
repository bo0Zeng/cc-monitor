// 纯 JVM 模块（kotlin("jvm")）：编译器保证没有 Android 耦合。
// 任务名因此不同：测试是 `test`，静态分析是 `detektMain`（带类型解析；plain `detekt` 不做类型解析，会漏规则）。
plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.ksp) // 线上类（`link/wire/`）的 Moshi 解码器在编译期生成
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

// Kotlin jvmTarget 在根 subprojects{} 里统一配置。

// 与 `:app` 接同一个共享测试源目录 `testing/src/test/kotlin`（源码词法扫描器）；目录里的自检在本模块也跑一遍。
// 手机自己的身份（版本门的对照物）只有一处：本仓后端 `src/backend/lib.rs` 的 `pub const BUILD_ID`；构建时抄进生成的
// `EmbeddedBuild.ID`，不在手机里另写一份。认不出那一行 ⇒ 构建失败，不猜。
//
// 包里带着给远端装的两份后端（`src/frontend/shell/embedded-backends/`，桌面同一个目录：开发树由 `re-embed.sh` 铺、发版由
// `build-mobile` 取同一趟 `build-backends` 的产物放进来；`:app` 把它们放进 assets）。带了就**看字节**（同桌面 `shell/build.rs`）：
// 每一份扫出恰一个身份戳；几份不同版 · 与 `BUILD_ID` 不同 ⇒ 构建失败（「我这一版」与推出去的字节是同一个东西）。
// 带了哪几格进 `EmbeddedBuild.CARRIED`；一份都没铺 ⇒ 空（装后端那一步照实说「这一版没带」）。
val genEmbeddedBuild =
    tasks.register("genEmbeddedBuild") {
        // 输入输出只在任务里定：doLast 里引脚本顶层的 val 会把整个脚本对象带进配置缓存（存不下）。
        val backendLib = rootProject.file("../backend/lib.rs")
        val stampMarks = rootProject.file("../common/deploy-contract/src/lib.rs")
        val embeddedBackends = rootProject.file("../frontend/shell/embedded-backends")
        val backendArches = listOf("x86_64", "aarch64")
        val embeddedBuildDir = layout.buildDirectory.dir("generated/embeddedBuild")
        inputs.file(backendLib)
        inputs.file(stampMarks)
        inputs.files(backendArches.map { embeddedBackends.resolve("cc-monitor-backend-$it") }).optional()
        outputs.dir(embeddedBuildDir)
        doLast {
            fun constOf(
                f: File,
                name: String,
            ): String =
                Regex("(?m)^pub const $name: &str = \"([^\"]+)\";").find(f.readText())?.groupValues?.get(1)
                    ?: throw GradleException("${f.path} 里认不出 `pub const $name: &str = \"…\";`")
            val id = constOf(backendLib, "BUILD_ID")
            val open = Regex.escape(constOf(stampMarks, "STAMP_OPEN"))
            val close = Regex.escape(constOf(stampMarks, "STAMP_CLOSE"))
            val stamp = Regex("$open([A-Za-z0-9_.-]+)$close")
            val carried =
                backendArches.mapNotNull { arch ->
                    val f = embeddedBackends.resolve("cc-monitor-backend-$arch")
                    if (!f.isFile) return@mapNotNull null
                    val ids = stamp.findAll(String(f.readBytes(), Charsets.ISO_8859_1)).map { it.groupValues[1] }.toSet()
                    val one = ids.singleOrNull() ?: throw GradleException("${f.path} 里扫出 ${ids.size} 个身份戳（该恰一个）：问不出它是哪一版，不带它")
                    arch to one
                }
            val versions = carried.map { it.second }.toSet()
            if (versions.size > 1 || (versions.isNotEmpty() && versions.single() != id)) {
                throw GradleException(
                    "包里要带的后端不是这一版：${carried.joinToString(" · ") { "${it.first} = `${it.second}`" }} · 源码 `BUILD_ID` = `$id`。\n" +
                        "重铺：`bash tests/scripts/re-embed.sh`（两个 musl arch 从同一份源码重编）；不带：`bash tests/scripts/re-embed.sh --clean`。",
                )
            }
            val out = embeddedBuildDir.get().file("com/ccmonitor/mobile/core/claude/link/EmbeddedBuild.kt").asFile
            out.parentFile.mkdirs()
            out.writeText(
                "// 生成物：core-claude/build.gradle.kts 的 genEmbeddedBuild 从 src/backend/lib.rs 与包里带的后端字节扫来，勿手改。\n" +
                    "package com.ccmonitor.mobile.core.claude.link\n\n" +
                    "/** 这一版手机的身份（与本仓后端同一个 `BUILD_ID`；带了后端字节时也是它们自报的那一个）· 包里带了哪几格后端（arch）。 */\n" +
                    "object EmbeddedBuild {\n    const val ID: String = \"$id\"\n\n" +
                    "    val CARRIED: List<String> = listOf(${carried.joinToString(", ") { "\"${it.first}\"" }})\n}\n",
            )
        }
    }

// 生成物落在加进源码集的目录里：读源码集的任务（ktlint · detekt）都要排在生成之后。
tasks.matching { it.name.startsWith("runKtlint") || it.name.startsWith("ktlint") || it.name.startsWith("detekt") }.configureEach {
    dependsOn(genEmbeddedBuild)
}

kotlin {
    sourceSets.named("main") {
        kotlin.srcDir(genEmbeddedBuild)
    }
    sourceSets.named("test") {
        kotlin.srcDir(rootProject.file("testing/src/test/kotlin"))
    }
}

// 判据读仓根夹具目录里的跨语言金样（通用记录那几份等）：整个目录登记成测试任务的输入，金样一变就重跑，不吃缓存的绿。
tasks.named<Test>("test") {
    inputs.dir(rootProject.file("../../tests/__fixtures__")).withPathSensitivity(PathSensitivity.RELATIVE)
    // 线上类的判据对着 `cmd-wire` 导出的 TS 核：生成物一变就重跑。
    inputs.dir(rootProject.file("../frontend/ui/generated")).withPathSensitivity(PathSensitivity.RELATIVE)
}

dependencies {
    // 用 api：RemoteExecutor 出现在公开构造参数里（OneShot），消费方的 compile classpath 上要见到它。
    api(project(":core-remote"))
    implementation(libs.moshi)
    ksp(libs.moshi.kotlin.codegen)
    // 传输用 Flow<ByteArray>。用 -core（纯 JVM artifact），不引 Android 耦合。
    implementation(libs.kotlinx.coroutines.core)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
}
