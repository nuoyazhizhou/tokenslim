#!/usr/bin/env python3
r"""
重新模拟生成 21 个含敏感信息的测试/基准输入文件。

目的：替换主仓 benchmarks/ 和 tests/data/ 下含真实 Alibaba Cloud AccessKey
ID 与 Secret（已脱敏为 ***REDACTED***，具体值不写入仓库任何位置）以及
其他公司内部识别符（黑名单详见 BLACKLIST 常量，不在 docstring 列出以免反向索引）。
所有生成的内容完全模拟合成，**不**包含任何真实凭证、内部 URL、用户名、
Gerrit 仓库、芯片型号、产品代号等。

使用：
    python scripts/regen_benchmarks.py
    python scripts/regen_benchmarks.py --root /path/to/TokenSlim
    python scripts/regen_benchmarks.py --root C:/git_work/TokenSlim-publish2
    python scripts/regen_benchmarks.py --redact-config .tokenslim-redact.toml

安全模型：
    1. 黑名单外置：默认从仓库根 `.tokenslim-redact.toml` 加载 regex 模式，
       该文件在 .gitignore 中, 不进仓库；示例模板 `.tokenslim-redact.toml.example`
       提交进仓库, 让用户知道有这个机制。
    2. 内置兜底：找不到外置配置时使用一组最小硬编码模式（不显式列举公司
       内部字面），保证最基础防护。
    3. 大小写不敏感：所有匹配走 re.IGNORECASE。
    4. 域名通配：内部域名走 r'\bdomain_root\.[a-z.]+\b' 而非字面, 兼容
       .com/.co/.io/.ai 等 TLD 变体。
"""
from __future__ import annotations

import argparse
import random
import re
import string
import sys
import time
from pathlib import Path

# ---------- 安全字符串池（绝不使用真实凭据/URL/用户名/分支名/模块名） ----------
#
# 设计原则: 所有字符串为完全合成占位符, 不映射到任何真实公司产品/平台/组件。
# 用 "pkg_/proj_/user_/scm.example.com" 等中性前缀确保不撞名。

SAFE_USERS = ["user_alpha", "user_beta", "user_gamma", "user_delta", "user_epsilon"]
SAFE_HOSTS = ["build-cluster-01.test", "build-cluster-02.test", "ci-pool-01.test",
              "ci-pool-02.test", "ci-pool-03.test"]
SAFE_PROJECTS = ["project_p", "project_q", "project_r", "project_s", "project_t", "project_u"]
SAFE_TARGETS = ["pkg_a/mod1", "pkg_a/mod2", "pkg_b/mod3", "pkg_b/mod4",
                "pkg_c/mod5", "pkg_c/mod6", "pkg_d/mod7", "pkg_d/mod8",
                "pkg_e/mod9", "pkg_e/mod10"]
SAFE_FLAGS = ["-O2", "-Wall", "-Wextra", "-g", "-std=c++17",
              "-I./include_a", "-I./include_b", "-I./third_party/lib_x",
              "-L./lib_a", "-L./lib_b", "-L./lib_c",
              "-fPIC", "-DVERSION=4.2.0"]

# 分支（不直接列举被替代的旧字面以免反向索引）
SAFE_BRANCHES = ["dev", "main", "release/v1", "release/v2",
                 "feature/x", "feature/y", "fix/a", "fix/b"]
# 产品/平台/组件/仓库/分支 等命名空间合并在 SAFE_BRANCHES 一个池子, 避免命名空间膨胀。
# 当前生成器只需替换显式 `master` 分支名即可, 不需要为不存在的"产品/平台/组件/仓库"维护独立池子。
# UTF-8 字符池：保留 gcc_build_utf8.txt 的多字节字符测试能力
SAFE_UTF8_SAMPLES = [
    "测试中文字符", "日本語サンプル", "한국어 텍스트", "Пример текста",
    "Mañana", "Über", "Café", "naïve", "Ω∞π", "🌍🔥", "résumé", "Σ",
]


# 预定义合成 hash 池 (绝不包含禁用 token 子串, 避免随机碰撞)
SAFE_HASH_POOL = [
    "deadbeef", "cafebabe", "b00bcafe", "feedbead", "defec8ed", "baddcafe",
    "abcdef02", "fedcba02", "01fedcba", "98765432", "fedcba09", "abcdef09",
    "beef0002", "0002beef", "dec0de02", "c0de0002", "faded0c2", "b00bb00b",
    "f00df00d", "b00dcafe", "cafed00d", "fade2afe", "defecaced", "0b1ecafe",
    "ace0feba", "f00baa00", "beeface0", "d00dfeed", "b0cadb00", "d00db00b",
]

SAFE_HEX_CHARS = "0246789abcdef"  # 排除部分数字字符减少与 FORBIDDEN 列表的子串碰撞


def random_hex(n: int) -> str:
    return "".join(random.choices(SAFE_HEX_CHARS, k=n))


def random_hash(n: int = 8) -> str:
    """拼接合成 hash 直到达到 n 字符, 避免随机生成命中禁用 token 子串"""
    parts: list[str] = []
    total = 0
    while total < n:
        part = random.choice(SAFE_HASH_POOL)
        parts.append(part)
        total += len(part)
    return "".join(parts)[:n]


def random_rev() -> str:
    """40 字符 git revision"""
    return random_hash(40)


def random_token(n: int = 16) -> str:
    """模拟 AWS-style 假 token。**绝不**生成 LTAI 开头（避免和真实阿里云 Key 冲突）"""
    return "AKFAKE" + "".join(random.choices(string.ascii_letters + string.digits, k=n))


def fake_path(depth: int = 4) -> str:
    parts = [random.choice(SAFE_PROJECTS) for _ in range(depth)]
    return "/".join(parts)


def fake_iso_time(start: float, span: float) -> str:
    """生成 ISO8601 时间戳 (与原文件风格一致 [YYYY-MM-DDTHH:MM:SS.mmmZ])"""
    t = start + random.random() * span
    return time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime(t)) + f".{random.randint(0, 999):03d}Z"


# ---------- 流式写出器 ----------

class StreamingWriter:
    """按目标字节数流式写入文件,避免大文件 OOM"""

    def __init__(self, path: Path, target_bytes: int):
        self.path = path
        self.target = target_bytes
        self.written = 0
        self.fp = open(path, "w", encoding="utf-8", newline="\n")
        self.buf: list[str] = []
        self.buf_bytes = 0
        self.BUF_FLUSH = 256 * 1024  # 256KB

    def write(self, line: str) -> None:
        if not line.endswith("\n"):
            line += "\n"
        b = len(line.encode("utf-8"))
        if self.written + b > self.target:
            self._filled = True
            return  # 已达到目标大小,静默丢弃
        self.buf.append(line)
        self.buf_bytes += b
        self.written += b
        if self.buf_bytes >= self.BUF_FLUSH:
            self.flush()

    def is_filled(self) -> bool:
        return getattr(self, "_filled", False) or self.written >= self.target

    def flush(self) -> None:
        if self.buf:
            self.fp.write("".join(self.buf))
            self.buf.clear()
            self.buf_bytes = 0

    def close(self) -> None:
        self.flush()
        # 如果还差几字节未达目标,补一行 padding
        if self.written < self.target:
            self.fp.write(f"[end-of-fixture-pad-{random_hex(8)}]\n")
        self.fp.close()


# ---------- 各种 fixture 生成器 ----------

def gen_jenkins_pipeline(out: StreamingWriter, *, fail: bool, utf8: bool = False) -> None:
    """生成 Jenkins pipeline 风格日志(与 gcc_build_*.txt 一致)

    时间戳格式: [YYYY-MM-DDTHH:MM:SS.mmmZ] message
    """
    start = time.mktime((2026, 6, 1, 0, 0, 0, 0, 0, 0))
    span = 7 * 86400  # 7 天窗口
    ts = start

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(ts, span)}] {s}")

    user = random.choice(SAFE_USERS)
    build_no = random.randint(1000, 50000)
    host = random.choice(SAFE_HOSTS)
    project = random.choice(SAFE_PROJECTS)
    rev = random_rev()

    emit(f"Started by user {user}")
    emit(f"Rebuilds build #{build_no}")
    emit(f"Obtained {project}/build.pipeline from git ssh://git@{host}:2222/{project}")
    emit("Loading library ci-helpers@master")
    emit("Attempting to resolve master from remote references...")
    emit(" > git --version # timeout=10")
    emit(" > git --version # 'git version 2.30.2'")
    emit(f" > git ls-remote -h -- ssh://git@{host}:2222/ci-helpers # timeout=10")
    emit(f"Found match: refs/heads/master revision {rev}")
    emit("The recommended git tool is: NONE")
    emit(f"Using credential {random_token(8)}")
    emit(f"> git rev-parse --resolve-git-dir {project}/.git # timeout=10")
    emit("Resetting working tree")
    emit(" > git config core.sparsecheckout # timeout=10")
    emit(" > git checkout -f e0aa947effce7cca17e9151f76bc4fe9291dcfac")
    emit("[Pipeline] Start of Pipeline")
    emit("Running on Jenkins in /var/lib/jenkins/workspace")
    emit(f"[Pipeline] node {{ (hide) }}")
    emit(f"[Pipeline] withEnv: TOOLCHAIN=gcc-12, PROJECT={project}")
    emit(f"[Pipeline] {{ (stage: Checkout)")
    emit(f" Cloning repository {project}")
    emit(f" HEAD is now at {rev[:12]} initial commit")
    emit("[Pipeline] }")
    emit("[Pipeline] { (Build)")
    i = 0
    while not out.is_filled():
        target = random.choice(SAFE_TARGETS)
        flags = " ".join(random.sample(SAFE_FLAGS, 5))
        emit(f" g++ {flags} -c {target}.cpp -o /tmp/build/{target}.o")
        if i % 17 == 0 and (fail or random.random() < 0.3):
            emit(f"{target}.cpp:{random.randint(10, 500)}:5: error: 'undeclared' in expression")
            emit("  expected primary-expression before ')' token")
            emit(f"  {random.randint(10000, 99999)} |     return ptr->value();")
            emit("      |                  ^")
            emit(f"make[3]: *** [/tmp/build/{target}.o] Error 1")
        elif i % 11 == 0:
            emit(f"{target}.cpp:{random.randint(10, 500)}:12: warning: unused variable 'tmp_buf' [-Wunused-variable]")
        if utf8 and i % 25 == 0:
            for s in SAFE_UTF8_SAMPLES:
                emit(f"  log-info: utf8 sample = '{s}'")
        i += 1
    if fail:
        emit("make[1]: *** Waiting for unfinished jobs....")
        emit("make: *** Error 2")
        emit("Build failed. See console for details.")
        emit("[Pipeline] }")
        emit("[Pipeline] // stage")
        emit("[Pipeline] End of Pipeline")
        emit("Finished: FAILURE")
    else:
        emit("[Pipeline] }")
        emit("[Pipeline] // stage")
        emit("[Pipeline] stage (Test)")
        emit("[Pipeline] { (Test)")
        emit(f" Running 12 test suites ... ok ({random.randint(30, 240)}.{random.randint(0, 999):03d}s)")
        emit(" All tests passed.")
        emit("[Pipeline] }")
        emit("[Pipeline] // stage")
        emit("[Pipeline] End of Pipeline")
        emit("Finished: SUCCESS")


def gen_coverity_log(out: StreamingWriter) -> None:
    """生成 Coverity 扫描器 shell 风格日志(无 timestamp 前缀)"""
    out.write(f"Started by user {random.choice(SAFE_USERS)}")
    out.write("Running as SYSTEM")
    out.write("[EnvInject] - Loading node environment variables.")
    out.write("[EnvInject] - Preparing an environment for the build.")
    out.write("[EnvInject] - Keeping Jenkins system variables.")
    out.write("[EnvInject] - Keeping Jenkins build variables.")
    out.write(f"Building remotely on {random.choice(SAFE_HOSTS)} in workspace /jenkins2/workspace/coverity_{random.choice(SAFE_PROJECTS)}")
    out.write(f"The variable 'BUILD_DISPLAY_NAME' is explicitly set to '#{random.randint(1000, 9999)}-{random.choice(SAFE_PROJECTS)}'")
    out.write("Cov-config: /opt/cov/config/" + random_hash(8))
    out.write("Cov-build version: " + random_hash(16))
    out.write("> /opt/cov/bin/cov-build --dir /tmp/cov-int make -j4")
    i = 0
    while not out.is_filled():
        target = random.choice(SAFE_TARGETS)
        flags = " ".join(random.sample(SAFE_FLAGS, 5))
        out.write(f"+ g++ {flags} -c src/{target}.cpp -o obj/{target}.o")
        if i % 23 == 0:
            out.write(f"+ echo 'compiled {target}.o ({random.randint(100, 999)}KiB)'")
        if i % 41 == 0:
            out.write(f"+ set +x")
            out.write(f"+ '[' -f /tmp/filter.csv ']'")
        i += 1
    out.write("Coverity Scan analyze: 0 new defects")
    out.write("> /opt/cov/bin/cov-format-errors --dir /tmp/cov-int --json /tmp/results.json")
    out.write(f"Formatting {random.randint(800, 2000)} error records...")
    out.write(f"+ git push origin cov-{random_hash(8)}")
    out.write(f" * [new tag]         v{random.randint(1, 9)}.{random.randint(0, 30)}.{random.randint(0, 99)}    -> v{random.randint(1, 9)}.{random.randint(0, 30)}.{random.randint(0, 99)}")
    out.write("New run name is '#" + str(random.randint(1000, 9999)) + "-" + random.choice(SAFE_PROJECTS) + "-master-" + random_hash(8) + "'")
    out.write("Finished: SUCCESS")


def gen_firmware_build(out: StreamingWriter) -> None:
    """生成 Jenkins firmware build 流水日志(与 benchmarks/input_*.txt 风格一致)"""
    start = time.mktime((2026, 3, 1, 0, 0, 0, 0, 0, 0))
    span = 14 * 86400

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(start, span)}] {s}")

    user = random.choice(SAFE_USERS)
    host = random.choice(SAFE_HOSTS)
    project = random.choice(SAFE_PROJECTS)

    emit(f"Started by user {user}")
    emit(f"Rebuilds build #{random.randint(1000, 9000)}")
    emit(f"Obtained {project}/build.pipeline from git ssh://git@{host}:2222/{project}")
    emit("Loading library jenkinsci-unstashParam-library@master")
    emit("Attempting to resolve master from remote references...")
    emit(" > git --version # timeout=10")
    emit(" > git --version # 'git version 2.30.2'")
    emit(f" > git ls-remote -h -- ssh://git@{host}:2222/jenkins_lib # timeout=10")
    emit(f"Found match: refs/heads/master revision {random_rev()}")
    emit("The recommended git tool is: NONE")
    emit("Using credential " + random_token(8))
    emit(f"> git rev-parse --resolve-git-dir {project}/.git # timeout=10")
    emit("Resetting working tree")
    emit(" > git config core.sparsecheckout # timeout=10")
    emit(" > git checkout -f " + random_rev())
    emit(f"Obtained {project}/build-jobs/build-{project}.pipeline from git ssh://git@{host}:2222/{project}_jobs")
    emit(f"Obtained build-conf/firmware-{project}/release.config from git ssh://git@{host}:2222/conf")
    emit("Loading library jenkinsci-pipeline-unit@master")
    emit("Loading library jenkinsci-build-name-setter@master")
    emit("[Pipeline] Start of Pipeline")
    emit("Running on Jenkins in /var/lib/jenkins/workspace")
    emit(f"[Pipeline] node {{ (hide) }}")
    emit(f"[Pipeline] withEnv: BUILD_USER={user}, PROJECT={project}, BRANCH={random.choice(SAFE_BRANCHES)}")
    emit("[Pipeline] { (Checkout)")
    emit("+ set +x")
    emit("+ '[' -f /tmp/build-env.sh ']'")
    emit(f"+ export CI=true")
    emit(f"+ export store_key={random_token(24)}")
    emit(f"+ export store_secret={random_token(30)}")
    emit(f"+ export STORE_ENDPOINT=blob-store-{random.choice(['a','b','c'])}-{random.randint(1,9)}.example.com")
    emit(f"+ export BUILD_PLATFORM_DIR=/build_root/platform/{project}/")
    emit(f"+ export WORKSPACE_TMP=/var/jenkins_tmp/{project}_{random.randint(1000,9999)}")
    emit(f"+ export BUILD_YYYYMMDDHH=2026030{random.randint(1,9)}2{random.randint(0,5)}")
    emit(f"+ export EXECUTOR_NUMBER={random.randint(1,8)}")
    emit(f"+ '[' -f /var/jenkins_home/secrets/cloud-credentials.json ']'")
    # 大量 parallel gcc 编译行(循环到目标大小)
    i = 0
    while not out.is_filled():
        target = random.choice(SAFE_TARGETS)
        flags = " ".join(random.sample(SAFE_FLAGS, 6))
        emit(f" gcc {flags} -c src/{target}.cpp -o /tmp/build/{project}/{target}.o")
        if i % 23 == 0:
            emit(f"+ echo 'compiled {target}.o ({random.randint(100, 999)}KiB)'")
        if i % 41 == 0:
            emit(f"+ '[' -f /tmp/filter.csv ']'")
        if i % 73 == 0 and random.random() < 0.4:
            emit(f"src/{target}.cpp:{random.randint(10, 500)}:5: error: 'undeclared' in expression")
            emit(f"make[3]: *** [CMakeFiles/{project}.dir/build.make:{random.randint(100, 999)}: {target}.o] Error 1")
        if i % 91 == 0:
            emit(f"src/{target}.cpp:{random.randint(10, 500)}:12: warning: unused variable 'tmp_buf' [-Wunused-variable]")
        i += 1
    # make 链接阶段
    emit(f"+ make -j8 all")
    emit("Making all in .")
    emit("Making all in po")
    emit("Making all in tests")
    emit("Making all in docs")
    emit("Making all in fastwc")
    emit(f"gcc -O2 -shared -o lib{project}.so {project}-main.o {project}-util.o -lm")
    emit(f"gcc -O2 -o {project}-bin {project}-main.o {project}-util.o -lpthread -ldl")
    emit("[Pipeline] }")
    emit("[Pipeline] // withEnv")
    emit("[Pipeline] }")
    emit("[Pipeline] // node")
    emit("[Pipeline] End of Pipeline")
    emit(f"Finished: {random.choice(['SUCCESS', 'SUCCESS', 'SUCCESS', 'FAILURE'])}")


# ---------- Android/Gradle 构建日志生成器 ----------

def gen_android_build(out: StreamingWriter, *, fail: bool) -> None:
    """生成 Android/Gradle 构建日志 (android_build_success.txt / android_build_failure.txt)"""
    start = time.mktime((2026, 6, 1, 0, 0, 0, 0, 0, 0))
    span = 7 * 86400

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(start, span)}] {s}")

    user = random.choice(SAFE_USERS)
    gradle_ver = random.choice(["8.4", "8.5", "8.6", "8.7"])
    agp_ver = random.choice(["8.2.0", "8.3.0", "8.4.0"])
    kotlin_ver = random.choice(["1.9.22", "1.9.23", "2.0.0"])
    project = random.choice(SAFE_PROJECTS)

    emit(f"Starting a Gradle Daemon (subsequent builds will be faster)")
    emit(f"> Task :app:preBuild")
    emit(f"> Task :app:preDebugBuild")
    emit(f"> Configure project :app")
    emit('AGPBI: "build" type "project"')
    emit(f"Reading effective POM for project ':app'")
    emit(f"Resolved plugin [id: 'com.android.application', version: '{agp_ver}']")
    emit(f"Resolved plugin [id: 'org.jetbrains.kotlin.android', version: '{kotlin_ver}']")
    emit(f"> Task :app:dataBindingMergeDependencyComponentsDebug")
    emit(f"> Task :app:generateDebugBuildConfig")
    emit(f"> Task :app:processDebugResources")
    emit(f"> Task :app:mergeDebugResources")
    emit(f"> Task :app:processDebugMainManifest")
    emit(f"> Task :app:mergeDebugJavaResource")
    emit(f"> Task :app:compileDebugKotlin ({kotlin_ver})")
    emit(f"w: file:///{project}/app/src/main/kotlin/com/{project}/MainActivity.kt:42:5")
    emit(f"   Unused import statement")
    emit(f"> Task :app:compileDebugJavaWithJavac (Java {random.choice(['17','21'])})")
    emit(f"Note: Recompile with -Xlint:deprecation for details.")
    emit(f"> Task :app:dexBuilderDebug")
    emit(f"> Task :app:mergeDebugGlobalSynthetics")
    emit(f"> Task :app:mergeProjectDexDebug")
    emit(f"> Task :app:packageDebug")
    emit(f"> Task :app:assembleDebug")
    if fail:
        emit(f"FAILURE: Build failed with an exception.")
        emit(f"* What went wrong:")
        emit(f"Execution failed for task ':app:processDebugResources'.")
        emit(f"> A failure occurred while executing com.android.build.gradle.tasks.ProcessAndroidResources")
        emit(f"   > Could not resolve all files for configuration ':app:debugRuntimeClasspath'.")
        emit(f"     > Could not find com.android.tools.build:gradle:{agp_ver}.")
        emit(f"     > Required by:")
        emit(f"         project :app")
        emit(f"* Try:")
        emit(f"> Run with --stacktrace option to get the stack trace.")
        emit(f"> Run with --info or --debug option to get more log output.")
        emit(f"> Run with --scan to get full insights.")
        emit(f"BUILD FAILED in {random.randint(20, 120)}s")
        emit(f"{random.randint(20, 50)} actionable tasks: {random.randint(10, 30)} executed, {random.randint(2, 8)} failed")
    else:
        emit(f"BUILD SUCCESSFUL in {random.randint(30, 180)}s")
        emit(f"{random.randint(40, 80)} actionable tasks: {random.randint(20, 50)} executed, {random.randint(2, 10)} from cache")
    i = 0
    while not out.is_filled():
        task = random.choice([
            "compileDebugKotlin", "compileDebugJavaWithJavac", "processDebugResources",
            "mergeDebugResources", "mergeDexDebug", "mergeDebugJavaResource",
            "packageDebug", "dexBuilderDebug", "transformClassesWithDexBuilder",
        ])
        elapsed = f"{random.randint(0, 30)}.{random.randint(0, 999):03d}s"
        emit(f"> Task :app:{task}")
        if i % 5 == 0:
            emit(f"   [Thread-{random.randint(1, 16)}] batch operation completed ({elapsed})")
        if i % 11 == 0 and fail and random.random() < 0.5:
            emit(f"   error: cannot find symbol class BuildConfig")
            emit(f"     location: package com.{project}.debug")
        if i % 13 == 0:
            emit(f"   class file transform {random_hash(8)} completed ({elapsed})")
        i += 1


# ---------- iOS/Xcode 构建日志生成器 ----------

def gen_ios_build(out: StreamingWriter, *, fail: bool) -> None:
    """生成 iOS/Xcode 构建日志 (ios_build_success.txt / ios_build_failure.txt)"""
    start = time.mktime((2026, 6, 1, 0, 0, 0, 0, 0, 0))
    span = 7 * 86400

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(start, span)}] {s}")

    project = random.choice(SAFE_PROJECTS)
    xcode_ver = random.choice(["15.2", "15.3", "15.4", "16.0"])
    ios_target = random.choice(["15.0", "16.0", "17.0", "17.2"])

    emit(f"Build target {project.capitalize()}App of project {project.capitalize()}App with configuration Debug")
    emit(f"Resolve Package Graph")
    emit(f"Fetching {project}.swift")
    emit(f"Computing version for {project}.swift")
    emit(f"Computed {project}.swift at {random_hash(40)} (13.40s)")
    emit(f"Resolved source packages to {project}.swift, {random.choice(SAFE_PROJECTS)}.swift, ... ({random.randint(2, 10)} total)")
    emit(f"/Users/buildbot/Library/Developer/Xcode/DerivedData/{project.capitalize()}App-{random_hash(8)}/Build/Intermediates.noindex")
    emit(f"SwiftDriver Compilation {project}App normal arm64 com.apple.xcode.tools.swift.compiler")
    emit(f"  /Users/buildbot/src/{project}/Sources/AppDelegate.swift:32:5: warning: variable was written to, but never read")
    emit(f"  /Users/buildbot/src/{project}/Sources/ViewController.swift:18:9: note: add 'let' to make it 'let'")
    emit(f"EmitSwiftModule normal arm64 (in target {project.capitalize()}App from project {project.capitalize()}App)")
    emit(f"CompileC {project}/Sources/NetworkModule.c normal arm64 c com.apple.compilers.llvm.clang.assembler")
    emit(f"CompileSwiftSources normal arm64 com.apple.xcode.tools.swift.compiler")
    emit(f"CompileSwift normal arm64 /Users/buildbot/src/{project}/Sources/DatabaseManager.swift (in target {project.capitalize()}App from project {project.capitalize()}App)")
    emit(f"LinkStoryboards {project}/Resources/Main.storyboard (in target {project.capitalize()}App from project {project.capitalize()}App)")
    emit(f"CodeSign {project}/build/{project}.app (in target {project.capitalize()}App from project {project.capitalize()}App)")
    emit(f"Touch /Users/buildbot/Library/Developer/Xcode/DerivedData/{project.capitalize()}App-{random_hash(8)}/Build/Products/Debug-iphoneos/{project}.app")
    if fail:
        emit(f"error: Build input file cannot be found: '/Users/buildbot/src/{project}/Sources/MissingModule.swift'")
        emit(f"warning: Skipping duplicate build file in Compile Sources build phase: {project}/Sources/ViewController.swift")
        emit(f"Undefined symbols for architecture arm64:")
        emit(f"  \"_OBJC_CLASS_$_RNKeychain\", referenced from:")
        emit(f"      objc-class-ref in {project}/build/lib{random.choice(SAFE_TARGETS).split('/')[-1]}.o")
        emit(f"ld: symbol(s) not found for architecture arm64")
        emit(f"clang: error: linker command failed with exit code 1 (use -v to see invocation)")
        emit(f"** BUILD FAILED **")
    else:
        emit(f"** BUILD SUCCEEDED **")
    i = 0
    while not out.is_filled():
        sub = random.choice([
            "Network", "Database", "UI", "Audio", "Renderer", "Cache",
            "Session", "Parser", "Queue", "Worker", "View", "Controller",
        ])
        op = random.choice(["compile", "emit-module", "merge-module", "link", "code-sign"])
        emit(f"{op} {project}/Sources/{sub}Module.swift normal arm64 (elapsed: {random.randint(0, 5)}.{random.randint(0, 999):03d}s)")
        if i % 7 == 0 and fail and random.random() < 0.4:
            emit(f"  error: type '{sub}Delegate' does not conform to protocol 'NSObjectProtocol'")
        if i % 9 == 0:
            emit(f"  /Users/buildbot/src/{project}/Sources/{sub}Module.swift:{random.randint(10, 500)}:9: warning: result of call to function '{sub}Handler' is unused")
        i += 1


# ---------- Maven/Java 构建日志生成器 ----------

def gen_maven_java_build(out: StreamingWriter, *, fail: bool) -> None:
    """生成 Maven Java 构建日志 (maven_java_build_success.txt / maven_java_build_failure.txt)"""
    start = time.mktime((2026, 6, 1, 0, 0, 0, 0, 0, 0))
    span = 7 * 86400

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(start, span)}] {s}")

    user = random.choice(SAFE_USERS)
    project = random.choice(SAFE_PROJECTS)
    java_ver = random.choice(["17.0.9", "17.0.10", "21.0.1", "21.0.2"])
    maven_ver = random.choice(["3.9.6", "3.9.7", "3.9.8"])

    emit(f"[INFO] Scanning for projects...")
    emit(f"[INFO] --< {project}.{project}:core >-----------------------------")
    emit(f"[INFO] Building {project}.{project} 4.2.0")
    emit(f"[INFO] --------------------------------[ jar ]---------------------------------")
    emit(f"[INFO] --- maven-clean-plugin:3.3.2:clean (default-clean) @ core ---")
    emit(f"[INFO] --- maven-resources-plugin:3.3.1:resources (default-resources) @ core ---")
    emit(f"[INFO] --- maven-compiler-plugin:3.13.0:compile (default-compile) @ core ---")
    emit(f"[INFO] Changes detected - recompiling the module!")
    emit(f"[INFO] Compiling {random.randint(80, 250)} source files with javac [{java_ver}] to target {java_ver}")
    emit(f"[INFO] --- maven-resources-plugin:3.3.1:testResources (default-testResources) @ core ---")
    emit(f"[INFO] --- maven-compiler-plugin:3.13.0:testCompile (default-testCompile) @ core ---")
    emit(f"[INFO] Compiling {random.randint(40, 120)} source files with javac [{java_ver}]")
    emit(f"[INFO] --- maven-surefire-plugin:3.2.5:test (default-test) @ core ---")
    emit(f"[INFO] Tests run: {random.randint(200, 800)}, Failures: 0, Errors: 0, Skipped: 0")
    emit(f"[INFO] --- maven-jar-plugin:3.4.1:jar (default-jar) @ core ---")
    emit(f"[INFO] Building jar: /var/maven/repo/{project}/{project}/4.2.0/{project}-core-4.2.0.jar")
    emit(f"[INFO] --- maven-install-plugin:3.1.2:install (default-install) @ core ---")
    emit(f"[INFO] Installing /var/maven/repo/{project}/{project}/4.2.0/{project}-core-4.2.0.jar to /var/maven/.m2/repository/{project}/{project}/4.2.0/{project}-core-4.2.0.jar")
    if fail:
        emit(f"[INFO] --< {project}.{project}:web >-----------------------------")
        emit(f"[INFO] Building {project}.{project} 4.2.0")
        emit(f"[ERROR] COMPILATION ERROR :")
        emit(f"[ERROR] /var/jenkins/src/{project}/web/src/main/java/com/{project}/web/ApiController.java:[{random.randint(20,80)},{random.randint(10,40)}] cannot find symbol")
        emit(f"[ERROR]   symbol:   class {project.capitalize()}Service")
        emit(f"[ERROR]   location: package com.{project}.service")
        emit(f"[ERROR] /var/jenkins/src/{project}/web/src/test/java/com/{project}/web/ApiTest.java:[{random.randint(20,80)},{random.randint(10,40)}] method does not override or implement a method from a supertype")
        emit(f"[ERROR] Failed to execute goal org.apache.maven.plugins:maven-compiler-plugin:3.13.0:compile (default-compile) on project {project}-web: COMPILATION ERROR")
        emit(f"[INFO] BUILD FAILURE")
    else:
        emit(f"[INFO] BUILD SUCCESS")
    emit(f"[INFO] Total time:  {random.randint(20, 300)}.{random.randint(0, 999):03d} s")
    emit(f"[INFO] Finished at: {fake_iso_time(start, span)}")
    emit(f"[INFO] ------------------------------------------------------------------------")
    i = 0
    while not out.is_filled():
        phase = random.choice([
            "compile", "test-compile", "test", "package", "install",
            "verify", "compile-tests", "process-resources", "process-test-resources",
        ])
        module = random.choice(["core", "api", "web", "common", "util"])
        emit(f"[INFO] --- maven-{random.choice(['compile','surefire','jar','install'])}-plugin:{random.choice(['3.13.0','3.2.5','3.4.1','3.1.2'])}:{phase} (default-{phase}) @ {module} ---")
        if i % 7 == 0:
            emit(f"[INFO] No sources to compile")
        if i % 13 == 0 and fail and random.random() < 0.4:
            emit(f"[ERROR] {module}/{module}-utils/src/main/java/com/{project}/util/{module.capitalize()}Helper.java:[{random.randint(10,80)},{random.randint(5,40)}] incompatible types: int cannot be converted to String")
        if i % 17 == 0:
            emit(f"[INFO] Tests run: {random.randint(50, 200)}, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: {random.randint(1, 30)}.{random.randint(0, 999):03d} s")
        i += 1


# ---------- Node.js/npm 构建日志生成器 ----------

def gen_nodejs_build(out: StreamingWriter) -> None:
    """生成 Node.js/npm 构建日志 (nodejs_build_failure.txt)"""
    start = time.mktime((2026, 6, 1, 0, 0, 0, 0, 0, 0))
    span = 7 * 86400

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(start, span)}] {s}")

    user = random.choice(SAFE_USERS)
    project = random.choice(SAFE_PROJECTS)
    node_ver = random.choice(["v18.20.0", "v20.11.0", "v20.12.0", "v21.5.0"])
    npm_ver = random.choice(["10.2.4", "10.3.0", "10.4.0"])

    emit(f"npm WARN using --force Recommended protections disabled.")
    emit(f"npm WARN deprecated [email protected]: this package is deprecated")
    emit(f"npm WARN deprecated [email protected]: use Date.now() instead")
    emit(f"npm WARN deprecated [email protected]: This package is no longer supported")
    emit(f"> {project}-service@4.2.0 build")
    emit(f"> tsc -p tsconfig.json && node scripts/postbuild.js")
    emit(f"src/server.ts:42:5 - error TS2304: Cannot find name 'DatabaseConfig'.")
    emit(f"src/api/users.ts:18:9 - error TS2345: Argument of type 'string | undefined' is not assignable to parameter of type 'string'.")
    emit(f"src/middleware/auth.ts:55:3 - error TS2531: Object is possibly 'null'.")
    emit(f"src/lib/storage.ts:91:7 - error TS2322: Type 'Buffer' is not assignable to type 'string'.")
    emit(f"src/lib/storage.ts:103:12 - error TS2349: This expression is not callable.")
    emit(f"Found 5 errors in 4 files.")
    emit(f"Errors  Files")
    emit(f"     2  src/api/users.ts:18")
    emit(f"     1  src/server.ts:42")
    emit(f"     1  src/middleware/auth.ts:55")
    emit(f"     2  src/lib/storage.ts:91")
    emit(f"npm ERR! code ELIFECYCLE")
    emit(f"npm ERR! errno 1")
    emit(f"npm ERR! {project}-service@4.2.0 build: `tsc -p tsconfig.json && node scripts/postbuild.js`")
    emit(f"npm ERR! Exit status 1")
    emit(f"npm ERR! Failed at the {project}-service@4.2.0 build script.")
    emit(f"npm ERR! This is probably not a problem with npm. There is likely additional logging output above.")
    i = 0
    while not out.is_filled():
        pkg = random.choice([
            f"@{project}/core", f"@{project}/api", f"@{project}/web",
            "lodash", "axios", "express", "typescript", "jest", "ts-node",
        ])
        emit(f"npm WARN deprecated {pkg}@1.{random.randint(0, 9)}.{random.randint(0, 9)}: this package is deprecated")
        if i % 7 == 0 and random.random() < 0.5:
            emit(f"src/{random.choice(['server','api','lib','middleware'])}/{random.choice(SAFE_TARGETS).split('/')[-1]}.ts:{random.randint(10, 500)}:5 - error TS2304: Cannot find name '{project.capitalize()}Service'.")
        if i % 11 == 0:
            emit(f"npm WARN tarball tarball data for {pkg}@1.{random.randint(0, 9)}.{random.randint(0, 9)} (sha512:{random_hash(64)}) seems to be corrupted. Trying again.")
        i += 1


# ---------- Jenkins 通用构建日志生成器(短) ----------

def gen_jenkins_build(out: StreamingWriter) -> None:
    """生成 Jenkins 短日志 (jenkins_build_failure.txt, 8-9KB)"""
    start = time.mktime((2026, 6, 1, 0, 0, 0, 0, 0, 0))
    span = 7 * 86400

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(start, span)}] {s}")

    user = random.choice(SAFE_USERS)
    project = random.choice(SAFE_PROJECTS)
    build_num = random.randint(100, 999)

    emit(f"Started by user {user}")
    emit(f"Building in workspace /var/jenkins_home/workspace/{project}-build")
    emit(f"[Pipeline] Start of Pipeline")
    emit(f"[Pipeline] node {{")
    emit(f"Running on Jenkins in /var/jenkins_home")
    emit(f"[Pipeline] {{ (Checkout)")
    emit(f"> git rev-parse --resolve-git-dir {project}/.git # timeout=10")
    emit(f"Resetting working tree")
    emit(f"> git checkout -f {random_rev()}")
    emit(f"[Pipeline] {{ (Build)")
    emit(f"+ export store_key={random_token(24)}")
    emit(f"+ export store_secret={random_token(30)}")
    emit(f"+ export STORE_ENDPOINT=blob-store-{random.choice(['a','b','c'])}-1.example.com")
    emit(f" g++ -O2 -Wall -Wextra -c src/{random.choice(SAFE_TARGETS)}.cpp -o /tmp/build/{project}.o")
    emit(f"src/{random.choice(SAFE_TARGETS)}.cpp:{random.randint(10, 200)}:5: error: 'undeclared' in this function")
    emit(f"  return new_value();")
    emit(f"  ^~~~~~~~~~~")
    emit(f"make[2]: *** [/tmp/build/{project}.o] Error 1")
    emit(f"make[1]: *** [CMakeFiles/{project}.dir/all] Error 2")
    emit(f"make: *** [all] Error 2")
    emit(f"Build step 'Execute shell' marked build as failure")
    emit(f"[Pipeline] // withEnv")
    emit("[Pipeline] }")
    emit(f"[Pipeline] // node")
    emit(f"[Pipeline] End of Pipeline")
    emit(f"Finished: FAILURE")
    emit(f"")
    emit(f"ERROR: Failed to build. See console output for details.")
    i = 0
    while not out.is_filled():
        emit(f" g++ -O2 -c src/{random.choice(SAFE_TARGETS)}.cpp -o /tmp/build/{project}-{i}.o")
        if i % 5 == 0:
            emit(f"src/{random.choice(SAFE_TARGETS)}.cpp:{random.randint(10, 200)}:{random.randint(1, 80)}: error: undeclared identifier")
            emit(f"make[2]: *** [/tmp/build/{project}-{i}.o] Error 1")
        if i % 7 == 0:
            emit(f"+ '[' -f /tmp/cache.bin ']'")
        i += 1


# ---------- gcc_build_failure 短失败日志(3 个变种) ----------

def gen_gcc_build_failure(out: StreamingWriter, variant: int) -> None:
    """生成 gcc 短失败日志(51KB / 17KB / 18KB),3 个变种对应 -1/-2/-3"""
    start = time.mktime((2026, 6, 1, 0, 0, 0, 0, 0, 0))
    span = 7 * 86400

    def emit(s: str) -> None:
        out.write(f"[{fake_iso_time(start, span)}] {s}")

    user = random.choice(SAFE_USERS)
    project = random.choice(SAFE_PROJECTS)
    # 3 个变种不同错误模式
    error_templates = {
        1: [
            "undefined reference to `vtable for {Cls}'",
            "use of deleted function 'std::unique_ptr<{Cls}>::unique_ptr(...)'",
            "no matching function for call to '{Func}({Args})'",
        ],
        2: [
            "expected unqualified-id before '{{' token",
            "redefinition of 'class {Cls}'",
            "previous definition of 'class {Cls}'",
        ],
        3: [
            "cannot bind non-const lvalue reference of type '{T}&' to an rvalue of type '{T}'",
            "static assertion failed: {Msg}",
            "void value not ignored as it ought to be",
        ],
    }
    templates = error_templates.get(variant, error_templates[1])

    emit(f"Started by user {user}")
    emit(f"Building remotely on {random.choice(SAFE_HOSTS)} in workspace /var/jenkins/workspace/{project}-build")
    emit(f"[Pipeline] Start of Pipeline")
    emit(f"[Pipeline] node {{")
    emit(f"[Pipeline] {{ (Build)")
    i = 0
    while not out.is_filled():
        target = random.choice(SAFE_TARGETS)
        emit(f" g++ -O2 -Wall -Wextra -c src/{target}.cpp -o /tmp/build/{target}.o")
        if i % 3 == 0:
            tmpl = random.choice(templates)
            line_no = random.randint(10, 500)
            tmpl_filled = tmpl.format(
                Cls=project.capitalize() + "Handler",
                Func="process",
                Args="int, char*",
                T="int",
                Msg="invalid size",
            )
            emit(f"src/{target}.cpp:{line_no}:5: error: {tmpl_filled}")
        if i % 5 == 0:
            emit(f"src/{target}.cpp:{random.randint(10, 500)}:12: warning: unused variable 'tmp_buf' [-Wunused-variable]")
        if i % 9 == 0:
            emit(f"make[3]: *** [/tmp/build/{target}.o] Error 1")
        if i % 13 == 0:
            emit(f"make[2]: *** [CMakeFiles/{project}.dir/all] Error 2")
        i += 1
    emit(f"make: *** [all] Error 2")
    emit(f"Build step 'Execute shell' marked build as failure")
    emit("[Pipeline] }")
    emit(f"[Pipeline] End of Pipeline")
    emit(f"Finished: FAILURE")


# ---------- benchmarks/input_128kb.txt (短小 firmware build) ----------

def gen_input_128kb(out: StreamingWriter) -> None:
    """生成 128KB 短固件构建日志 (benchmarks/input_128kb.txt)"""
    gen_firmware_build(out)


# ---------- 入口 ----------

# (relative_path, target_bytes, generator)
FIXTURES = [
    # ---------- 第一批:含 LTAI/B47 阿里云 AccessKey 的 11 个文件 ----------
    ("benchmarks/input_10mb.txt",                 10 * 1024 * 1024, gen_firmware_build),
    ("benchmarks/input_20mb.txt",                 20 * 1024 * 1024, gen_firmware_build),
    ("benchmarks/input_100mb.txt",               100 * 1024 * 1024, gen_firmware_build),
    ("tests/data/gcc_build_failure-4.txt",         1 * 1024 * 1024, lambda w: gen_jenkins_pipeline(w, fail=True)),
    ("tests/data/gcc_build_success.txt",          30 * 1024 * 1024, lambda w: gen_jenkins_pipeline(w, fail=False)),
    ("tests/data/gcc_build_utf8.txt",             30 * 1024 * 1024, lambda w: gen_jenkins_pipeline(w, fail=False, utf8=True)),
    ("tests/data/gcc_coverity_success-1.txt",     18 * 1024 * 1024, gen_coverity_log),
    ("tests/data/gcc_coverity_success-2.txt",     17 * 1024 * 1024, gen_coverity_log),
    # ---------- 第二批:含 BLACKLIST 黑名单关键字的 13 个文件 ----------
    ("benchmarks/input_128kb.txt",                128 * 1024,      gen_input_128kb),
    ("tests/data/android_build_success.txt",      2950 * 1024,     lambda w: gen_android_build(w, fail=False)),
    ("tests/data/android_build_failure.txt",      2984 * 1024,     lambda w: gen_android_build(w, fail=True)),
    ("tests/data/ios_build_success.txt",         28983 * 1024,     lambda w: gen_ios_build(w, fail=False)),
    ("tests/data/ios_build_failure.txt",          4068 * 1024,     lambda w: gen_ios_build(w, fail=True)),
    ("tests/data/maven_java_build_success.txt",    397 * 1024,     lambda w: gen_maven_java_build(w, fail=False)),
    ("tests/data/maven_java_build_failure.txt",    343 * 1024,     lambda w: gen_maven_java_build(w, fail=True)),
    ("tests/data/nodejs_build_failure.txt",        165 * 1024,     gen_nodejs_build),
    ("tests/data/jenkins_build_failure.txt",         9 * 1024,     gen_jenkins_build),
    ("tests/data/gcc_build_failure-1.txt",          51 * 1024,     lambda w: gen_gcc_build_failure(w, variant=1)),
    ("tests/data/gcc_build_failure-2.txt",          17 * 1024,     lambda w: gen_gcc_build_failure(w, variant=2)),
    ("tests/data/gcc_build_failure-3.txt",          18 * 1024,     lambda w: gen_gcc_build_failure(w, variant=3)),
]


def load_redact_patterns(config_path: Path | None) -> list[re.Pattern]:
    """加载黑名单 regex 模式。

    优先级:
    1. --redact-config 指定路径
    2. 仓库根 .tokenslim-redact.toml (gitignore, 本地存在)
    3. 内置 BUILTIN_MINIMAL 兜底

    配置文件格式: 每行一条 regex 模式, 注释以 # 开头, 空行忽略。
    所有匹配走 re.IGNORECASE + re.MULTILINE。
    """
    candidates = []
    if config_path is not None:
        candidates.append(config_path)
    candidates.append(Path(".tokenslim-redact.toml"))

    for path in candidates:
        if path.is_file():
            patterns: list[re.Pattern] = []
            for raw in path.read_text(encoding="utf-8").splitlines():
                line = raw.strip()
                if not line or line.startswith("#"):
                    continue
                try:
                    patterns.append(re.compile(line, re.IGNORECASE | re.MULTILINE))
                except re.error as e:
                    print(f"[redact] WARN: bad regex in {path}: {line!r} ({e})", file=sys.stderr)
            if patterns:
                print(f"[redact] loaded {len(patterns)} patterns from {path}")
                return patterns
            else:
                print(f"[redact] WARN: {path} exists but empty, falling through")

    # 兜底: 不显式列举公司内部字面, 只匹配最通用模式。
    # 外置配置文件才是黑名单主战场, 内置只防"配置文件丢失时不小心泄露"
    return [re.compile(p, re.IGNORECASE | re.MULTILINE) for p in BUILTIN_MINIMAL]


# 最小兜底集: 仅匹配"看起来像阿里云 AccessKey"等通用模式。
# 公司内部关键字 (A97/Firmware_build/linkplay 等) 必须由外置配置提供。
BUILTIN_MINIMAL = [
    r"\bLTAI[A-Za-z0-9]{12,}\b",          # 阿里云 AccessKey ID (16+ 字符)
    r"\bAKID[A-Za-z0-9]{16,}\b",          # AWS AccessKey ID
    r"\bSECRET_[A-Za-z0-9]{32,}\b",       # 通用 Secret 前缀
    r"\bghp_[A-Za-z0-9]{30,}\b",          # GitHub Personal Access Token
    r"\bxoxb-[A-Za-z0-9-]{20,}\b",        # Slack bot token
]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".", help="主仓根目录(默认当前目录)")
    parser.add_argument("--seed", type=int, default=20260624, help="随机种子(可复现)")
    parser.add_argument("--redact-config", type=Path, default=None,
                        help="外置黑名单配置文件路径 (每行一条 regex, 默认 .tokenslim-redact.toml)")
    parser.add_argument("--skip-redact-check", action="store_true",
                        help="跳过黑名单扫描 (仅调试, 不推荐)")
    args = parser.parse_args()

    random.seed(args.seed)
    root = Path(args.root).resolve()
    print(f"[regen] root={root} seed={args.seed}")

    forbid_patterns = load_redact_patterns(args.redact_config)
    print(f"[redact] active patterns: {len(forbid_patterns)}")

    for rel, size, gen in FIXTURES:
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        print(f"[regen] {rel} target={size / 1024 / 1024:.1f}MB ...", flush=True)
        w = StreamingWriter(path, size)
        gen(w)
        w.close()
        actual = path.stat().st_size
        if not args.skip_redact_check:
            # 检查禁用模式 (大小写不敏感)
            with open(path, "rb") as f:
                head = f.read(1024 * 1024).decode("utf-8", errors="replace")
            for pat in forbid_patterns:
                m = pat.search(head)
                if m:
                    print(f"  [FATAL] {rel} matches pattern {pat.pattern!r} -> {m.group(0)!r}")
                    return 1
        print(f"  -> {actual / 1024 / 1024:.2f}MB ok")

    print(f"[regen] all 21 fixtures regenerated, no real-keyword/credential/secret leakage detected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
