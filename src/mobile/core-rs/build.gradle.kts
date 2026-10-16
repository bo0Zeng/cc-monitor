// 手机嵌的 Rust 核心（`libmobcore.so`）：Rust 源在 `rust/`（crate `mobcore`，后端那一棵 workspace 的成员，拨号是 `ssh-dial` 那一份），
// Kotlin 侧只有一个口 `Core`（手写 JNI，走消息）。
//
// 每个构建类型只编它自己那一个 ABI（`gradle.properties` 的 `ccm.abi.<构建类型>`；app 的 abiFilters 读同一行）：
//   release（发出去的包，真机也用它）= arm64-v8a；debug（模拟器 · androidTest · 冒烟）= x86_64。两个 ABI 从不在同一趟里编。
// cargo 由这里调：`cargoBuild<构建类型>` 只编那一个 target，产物抄进 `build/rustJniLibs/<构建类型>/<ABI>/`，那个目录接进同名源集的 jniLibs。
// cargo 自己按源码增量（没改就几百毫秒），所以本任务每趟都跑、不另记输入。
// JVM 单测加载同一个 crate 的宿主版（根 `build.gradle.kts` 的 `cargoBuildHost`，写字经它）；`Core` 只在真连接那一条路上用，单测走假传输。
plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

val repoRoot: File = rootProject.projectDir.parentFile.parentFile
val ndkVersionPinned: String = providers.gradleProperty("ccm.ndkVersion").get()

/** 一个构建类型 ⇒ 它那一个 ABI（`gradle.properties`，app 的 abiFilters 读同一行）。 */
fun abiOf(buildType: String): String = providers.gradleProperty("ccm.abi.$buildType").get()

/** ABI ⇒ Rust 的 target。 */
fun tripleOf(abi: String): String =
    when (abi) {
        "arm64-v8a" -> "aarch64-linux-android"
        "x86_64" -> "x86_64-linux-android"
        else -> throw GradleException("不认的 ABI：$abi")
    }

android {
    namespace = "com.ccmonitor.mobile.core.rs"
    ndkVersion = ndkVersionPinned
    defaultConfig {
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }
    buildTypes {
        getByName("debug") { ndk { abiFilters += abiOf("debug") } }
        getByName("release") { ndk { abiFilters += abiOf("release") } }
        // app 的混淆冒烟包（R8 照正式包、调试签名、模拟器的 ABI）：核心在这里只换 ABI。
        create("minified") {
            initWith(getByName("release"))
            ndk {
                abiFilters.clear()
                abiFilters += abiOf("minified")
            }
        }
    }
}

// 每个构建类型一份 cargo 任务 ＋ 一个 jniLibs 目录。
listOf("debug", "release", "minified").forEach { buildType ->
    val abi = abiOf(buildType)
    val triple = tripleOf(abi)
    // release ⇒ `mobile`（opt-level z · lto · 去符号 · panic=unwind）；debug · minified ⇒ `mobile-dev`（增量快、去调试信息；
    // minified 那一趟验的是 JVM 那一侧的混淆，核心照调试那一份编）。两份都住后端那一棵 workspace 的根清单。
    val profile = if (buildType == "release") "mobile" else "mobile-dev"
    val cap = buildType.replaceFirstChar(Char::uppercase)
    val jniDir = layout.buildDirectory.dir("rustJniLibs/$buildType")
    val cargo =
        tasks.register<Exec>("cargoBuild$cap") {
            group = "build"
            description = "cargo 编 libmobcore.so（$triple，profile $profile）"
            val ndk = androidComponents.sdkComponents.ndkDirectory
            // workspace 根 = 后端那份根清单所在的目录（cargo 在那里找 lock 与两份 profile）。
            val workspace = repoRoot.resolve("src/backend/Cargo.toml").parentFile
            val so = repoRoot.resolve(".build/backend/$triple/$profile/libmobcore.so") // 本地抄好：doLast 里引脚本顶层的 repoRoot 进不了配置缓存
            workingDir = workspace
            doFirst {
                val bin = ndk.get().asFile.resolve("toolchains/llvm/prebuilt/linux-x86_64/bin")
                val clang = bin.resolve("${triple}26-clang").path // 26 = minSdk
                val u = triple.replace('-', '_')
                environment("CC_$u", clang)
                environment("AR_$u", bin.resolve("llvm-ar").path)
                environment("CARGO_TARGET_${u.uppercase()}_LINKER", clang)
            }
            commandLine("cargo", "build", "-p", "mobcore", "--target", triple, "--profile", profile)
            outputs.upToDateWhen { false }
            doLast {
                if (!so.isFile) throw GradleException("cargo 说编完了，$so 却不在")
                val dest = jniDir.get().asFile.resolve(abi)
                dest.mkdirs()
                so.copyTo(dest.resolve("libmobcore.so"), overwrite = true)
            }
        }
    android.sourceSets
        .getByName(buildType)
        .jniLibs
        .srcDir(jniDir)
    tasks.configureEach {
        if (name == "merge${cap}JniLibFolders") dependsOn(cargo)
    }
}

dependencies {
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
