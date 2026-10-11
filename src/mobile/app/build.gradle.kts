import java.util.Properties
import java.util.zip.ZipFile

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// 发布签名：从 gitignored 的 keystore.properties 读凭据；没有这个文件就产出未签名的 release，照样能构建。
// 文件默认在手机工程根；`-Pccm.keystoreProperties=<路径>` 指到别处（发版流水线把它写在 runner 的临时目录里，不落工作树）。
val keystorePropsFile =
    providers.gradleProperty("ccm.keystoreProperties").orNull?.let(::file) ?: rootProject.file("keystore.properties")
val keystoreProps =
    Properties().apply {
        // 用 UTF-8 reader：InputStream 按 ISO-8859-1 解码，非 ASCII 路径会乱码，找不到密钥库。
        if (keystorePropsFile.exists()) keystorePropsFile.reader(Charsets.UTF_8).use { load(it) }
    }

// 版本号跟仓里的走：versionName 就是仓根 `package.json` 的 `version`（桌面那几份的权威源），不在手机里另写一份；
// bump 桌面版本号时手机这份跟着变，不用另改。versionCode 由它算：主 × 1_000_000 ＋ 次 × 1_000 ＋ 修订
// （次、修订各在 0..999，越界就构建失败）⇒ 版本号只往上走，versionCode 就只往上走。认不出那一行 ⇒ 构建失败，不猜。
val repoVersion: String =
    Regex("(?m)^  \"version\": \"(\\d+\\.\\d+\\.\\d+)\",$")
        .find(rootProject.file("../../package.json").readText())
        ?.groupValues
        ?.get(1)
        ?: throw GradleException("仓根 package.json 里认不出顶层那行 `\"version\": \"X.Y.Z\",`")
val repoVersionCode: Int =
    repoVersion.split('.').map { it.toInt() }.let { (major, minor, patch) ->
        if (minor > 999 || patch > 999) throw GradleException("版本号 $repoVersion 的次 / 修订超过 999，versionCode 算法要改")
        major * 1_000_000 + minor * 1_000 + patch
    }

android {
    namespace = "com.ccmonitor.mobile"

    // 共享测试源目录 `testing/src/test/kotlin`（源码词法扫描器 KotlinSourceScanner）接进 test 源集。
    // `:core-claude` 接的是同一个目录，全仓只有一份实现；目录里的自检在两个模块各跑一遍，哪边接线掉了哪边编译不过。
    sourceSets.getByName("test") {
        kotlin.srcDir(rootProject.file("testing/src/test/kotlin"))
    }
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置；targetSdk 在这里。
    signingConfigs {
        if (keystorePropsFile.exists()) {
            create("release") {
                storeFile = file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    defaultConfig {
        applicationId = "com.ccmonitor.mobile"
        targetSdk = 36
        versionCode = repoVersionCode
        versionName = repoVersion
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        // 每个构建类型只装一个 ABI（`gradle.properties` 的 `ccm.abi.<构建类型>`，`:core-rs` 只为它编那一个）：
        // 32 位机一律不装（装上了一加载 .so 就崩）。包里真只有那一个 ABI 由 `verify<变体>Abis` 核（见文件末）。
        getByName("debug") { ndk { abiFilters += providers.gradleProperty("ccm.abi.debug").get() } }
        release {
            ndk { abiFilters += providers.gradleProperty("ccm.abi.release").get() }
            // R8 full mode：开 minify，CI 借此验 BC / sshj / Tink 的反射在收缩与混淆下不破。
            // BC / sshj 的 keep 来自 core-ssh 的 consumer-rules.pro；Tink 的 keep 在 app 的 proguard-rules.pro。
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            // 正式签名：keystore.properties 存在则用，否则不签。
            if (keystorePropsFile.exists()) signingConfig = signingConfigs.getByName("release")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true // BuildConfig.DEBUG 门控只在 debug 里做的播种
    }
    // :app:lintDebug 默认只扫 app 源，NewApi 一类问题多在 core-data / core-ssh，所以下钻依赖模块。
    lint {
        checkDependencies = true
    }
}

// prism4j 带进 org.jetbrains:annotations-java5，与主 annotations 重复，排除掉免 duplicate-class。
configurations.all {
    exclude(group = "org.jetbrains", module = "annotations-java5")
}

dependencies {
    implementation(project(":core-ui"))
    implementation(project(":core-data"))
    implementation(project(":core-ssh"))
    implementation(project(":core-terminal"))
    implementation(project(":core-claude"))
    implementation(project(":core-remote"))
    implementation(project(":core-copy"))
    implementation(project(":core-rs"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.activity.compose)

    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.ui.tooling.preview)
    implementation(libs.androidx.material3)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.google.material)
    implementation(libs.kotlinx.coroutines.android)

    implementation(libs.koin.android)
    implementation(libs.koin.androidx.compose)

    // Markdown 渲染
    implementation(libs.markwon.core)
    implementation(libs.markwon.ext.tables)
    implementation(libs.markwon.ext.strikethrough)
    implementation(libs.markwon.linkify)
    implementation(libs.markwon.ext.latex) // LaTeX 数学（JLaTeXMath）

    // 代码块语法高亮：syntax-highlight + Prism4j。bundler 是 Java 注解处理器，用 annotationProcessor
    // 处理 src/main/java 下的 @PrismBundle 类（生成 GrammarLocatorDef），不必为一个注解引入 kapt。
    implementation(libs.markwon.syntax.highlight)
    implementation(libs.prism4j)
    annotationProcessor(libs.prism4j.bundler)

    debugImplementation(libs.androidx.ui.tooling)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test) // ViewModel 测试：runTest + Dispatchers.setMain

    // Compose UI 测试。注意：androidTest 也要 platform(bom)，BOM 不跨 configuration 传递，漏了 ui-test-junit4 解析不到版本。
    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules) // GrantPermissionRule
    // ui-test-manifest 提供测试宿主 Activity 的 manifest 条目，必须落 debug 变体（不是 androidTest）。
    debugImplementation(libs.androidx.ui.test.manifest)
}

// ── 判据：包里恰好一个 ABI，而且就是这个构建类型登记的那一个（release = arm64-v8a · debug = x86_64），Rust 核心在里面 ──
// 读打出来的 APK 本身（不读配置）：`lib/<ABI>/` 那一层的目录集合 == {登记的那一个}，且其中有 `libmobcore.so`。
// 挂在 `assemble<变体>` 上：门禁 `mobile` 那一格的 assembleRelease · assembleDebug 两趟各核一次。
abstract class VerifyApkAbis : DefaultTask() {
    @get:InputDirectory
    abstract val apkDir: DirectoryProperty

    @get:Input
    abstract val expected: Property<String>

    @TaskAction
    fun verify() {
        val apks = apkDir.get().asFile.walkTopDown().filter { it.isFile && it.name.endsWith(".apk") }.toList()
        if (apks.size != 1) throw GradleException("要恰好一份 APK，实得 ${apks.map { it.name }}")
        val entries = ZipFile(apks.single()).use { z -> z.entries().asSequence().map { e -> e.name }.toList() }
        val abis = entries.filter { it.startsWith("lib/") }.map { it.split('/')[1] }.toSortedSet()
        val want = expected.get()
        if (abis != sortedSetOf(want)) throw GradleException("${apks.single().name} 里的 ABI 是 $abis，登记的是 [$want]")
        if ("lib/$want/libmobcore.so" !in entries) throw GradleException("${apks.single().name} 里没有 lib/$want/libmobcore.so")
    }
}

androidComponents {
    onVariants { variant ->
        val cap = variant.name.replaceFirstChar(Char::uppercase)
        val verify =
            tasks.register<VerifyApkAbis>("verify${cap}Abis") {
                apkDir.set(variant.artifacts.get(com.android.build.api.artifact.SingleArtifact.APK))
                expected.set(providers.gradleProperty("ccm.abi.${variant.buildType}"))
            }
        tasks.configureEach { if (name == "assemble$cap") finalizedBy(verify) }
    }
}
