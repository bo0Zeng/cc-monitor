import java.util.Properties
import java.util.zip.ZipFile

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    // Baseline Profile：给每个不可调试的构建类型派生 `nonMinified<类型>`（录 profile）· `benchmark<类型>`（量冷启），见 `:baselineprofile`。
    alias(libs.plugins.androidx.baselineprofile)
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
            // R8 full mode：开 minify，CI 借此验 Tink 的反射与 Rust 核心的 JNI 名字在收缩与混淆下不破。
            // Rust 核心的 keep 来自 core-rs 的 consumer-rules.pro；Tink 的 keep 在 app 的 proguard-rules.pro。
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            // 正式签名：keystore.properties 存在则用，否则不签。
            if (keystorePropsFile.exists()) signingConfig = signingConfigs.getByName("release")
        }
        // 混淆过的包、调试签名：收缩 · 混淆照正式包那一套（同几份规则），只换签名，装得进模拟器、不碰正式 keystore。
        // 冒烟多跑一趟它（`tests/mobile-smoke/smoke.sh`）：门禁只编正式包不跑，靠反射认名字的地方在混淆后坏了，这一趟露出来。
        // ABI 照模拟器（`ccm.abi.minified`）；别的模块没有这个构建类型 ⇒ 用它们的 release。
        create("minified") {
            initWith(getByName("release"))
            signingConfig = signingConfigs.getByName("debug")
            matchingFallbacks += listOf("release")
            ndk {
                abiFilters.clear()
                abiFilters += providers.gradleProperty("ccm.abi.minified").get()
            }
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

// 给远端装的两份后端进 assets（`backends/cc-monitor-backend-<arch>`）：取 `src/frontend/shell/embedded-backends/`（桌面同一个目录）。
// 哪几格 · 是不是这一版由 `:core-claude:genEmbeddedBuild` 看字节判（几份不同版 · 与 `BUILD_ID` 不同 ⇒ 那一步让构建失败）⇒ 先跑它。
// 一份都没铺 ⇒ assets 里没有这一目录，装后端那一步说「这一版没带」。按压缩存；真要装时 App 把那一格解到私有目录（每版一次）。
abstract class StageBackends : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val backends: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val outDir: DirectoryProperty

    @TaskAction
    fun stage() {
        val dest = outDir.get().asFile.resolve("backends")
        outDir.get().asFile.deleteRecursively()
        backends.files.filter { it.isFile }.forEach { it.copyTo(dest.resolve(it.name), overwrite = true) }
    }
}

val stageBackends =
    tasks.register<StageBackends>("stageBackends") {
        dependsOn(":core-claude:genEmbeddedBuild")
        backends.from(listOf("x86_64", "aarch64").map { rootProject.file("../frontend/shell/embedded-backends/cc-monitor-backend-$it") })
    }

androidComponents {
    onVariants { variant -> variant.sources.assets?.addGeneratedSourceDirectory(stageBackends, StageBackends::outDir) }
}

// ── Baseline Profile ──
// 录出来的规则并进 `src/main/generated/baselineProfiles/`（进仓；正式包带上，侧装的由 profileinstaller 首启后台装）。
// 只在 `minified` 派生的那一支上录与量：ABI 照 `ccm.abi.minified`（本机模拟器是 x86_64）；`release` 派生的两支 ABI 照 release，本机装不上原生的。
// 设备用 adb 连着的那一台（`scripts/baseline-profile.sh` 起的私有模拟器），不用 Gradle 托管设备；构建时不自动重录。
baselineProfile {
    variants {
        create("minified") {
            mergeIntoMain = true
            // 造数那一份只在量测那两族里有，不进规则。
            filter { exclude("com.ccmonitor.mobile.bench.**") }
        }
    }
}

/** 量测那两族构建类型（插件派生的 `nonMinified<类型>` · `benchmark<类型>`）⇒ 它派生自哪个构建类型；别的 ⇒ 原样。 */
fun baseBuildType(buildType: String): String =
    listOf("nonMinified", "benchmark")
        .firstOrNull { buildType.startsWith(it) && buildType.length > it.length }
        ?.let { buildType.removePrefix(it).replaceFirstChar(Char::lowercase) }
        ?: buildType

// 量测用的造数（一台不可达的机器 ＋ 一条长会话，`BenchSeedActivity`）只进那两族：源码 · 清单在 `src/benchSeed/`，
// 会话清单照后端那份金样（构建时拷进 assets 的 `bench/`）。正式包 · 调试包 · 冒烟包都不带。
abstract class StageBenchSeed : DefaultTask() {
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val golden: RegularFileProperty

    @get:OutputDirectory
    abstract val outDir: DirectoryProperty

    @TaskAction
    fun stage() {
        val dest = outDir.get().asFile.resolve("bench")
        outDir.get().asFile.deleteRecursively()
        golden.get().asFile.copyTo(dest.resolve(golden.get().asFile.name), overwrite = true)
    }
}

val stageBenchSeed =
    tasks.register<StageBenchSeed>("stageBenchSeed") {
        golden.set(rootProject.file("../../tests/__fixtures__/history-list.golden.json"))
    }

androidComponents {
    // 派生支照抄了它那一支的回退（`minified` ⇒ 回退 release），插件再把那一支本身排在后面 ⇒ 别的模块里有同名构建类型的（`:core-rs` 的 minified）
    // 也被配成 release 那一份（ABI 不对，包里少了 Rust 核心）。改成先找它派生自的那一支、再照那一支的回退。
    finalizeDsl { ext ->
        ext.buildTypes.forEach { bt ->
            val base = baseBuildType(bt.name).takeIf { it != bt.name } ?: return@forEach
            val chain = listOf(base) + ext.buildTypes.getByName(base).matchingFallbacks
            bt.matchingFallbacks.clear()
            bt.matchingFallbacks += chain
        }
    }
    onVariants { variant ->
        if (baseBuildType(variant.buildType.orEmpty()) == variant.buildType) return@onVariants
        // 经 java 那一格接：Kotlin 编译收 java 源目录里的 .kt；`sources.kotlin` 那一格这一版的 Kotlin 插件不收。
        variant.sources.java?.addStaticSourceDirectory("src/benchSeed/kotlin")
        variant.sources.manifests.addStaticManifestFile("src/benchSeed/AndroidManifest.xml")
        variant.sources.assets?.addGeneratedSourceDirectory(stageBenchSeed, StageBenchSeed::outDir)
    }
}

// 进发布包的那一档依赖锁进 `gradle.lockfile`（进仓）：第三方许可声明（`tests/scripts/third-party-notices.py`）与发版尺子 ⑰e
// 都只读这份，不跑 Gradle。依赖一变、锁没跟上 ⇒ 解析失败；重写锁：
//   ./gradlew :app:dependencies --configuration releaseRuntimeClasspath --write-locks
// 测试 · 基准 · 构建插件不进包，不锁。
configurations.matching { it.name == "releaseRuntimeClasspath" }.configureEach {
    resolutionStrategy.activateDependencyLocking()
}

dependencies {
    implementation(project(":core-ui"))
    implementation(project(":core-data"))
    implementation(project(":core-ssh"))
    implementation(project(":core-terminal"))
    implementation(project(":core-claude"))
    implementation(project(":core-wire")) // 屏上直接读线上类型（回包 · 事件 · 菜单的生成类）
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

    // 正文公式：块树里的 TeX 经 JLaTeXMath 画成图（正文排版本身是 Compose，照后端的块树排）。
    implementation(libs.jlatexmath.android)

    // 侧装的包（不经商店）首启后台把 Baseline Profile 装上；Macrobenchmark 也经它装。
    implementation(libs.androidx.profileinstaller)
    baselineProfile(project(":baselineprofile"))

    debugImplementation(libs.androidx.ui.tooling)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test) // ViewModel 测试：runTest + Dispatchers.setMain
    testImplementation(libs.moshi) // 读跨语言金样（tests/__fixtures__）

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
        val apks =
            apkDir
                .get()
                .asFile
                .walkTopDown()
                .filter { it.isFile && it.name.endsWith(".apk") }
                .toList()
        if (apks.size != 1) throw GradleException("要恰好一份 APK，实得 ${apks.map { it.name }}")
        val entries =
            ZipFile(apks.single()).use { z ->
                z
                    .entries()
                    .asSequence()
                    .map { e -> e.name }
                    .toList()
            }
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
                // 量测那两族照它派生自的那个构建类型（同一份 abiFilters）。
                expected.set(providers.gradleProperty("ccm.abi.${baseBuildType(variant.buildType.orEmpty())}"))
            }
        tasks.configureEach { if (name == "assemble$cap") finalizedBy(verify) }
    }
}
