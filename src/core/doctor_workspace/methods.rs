use super::types::*;
use crate::core::doctor_encoding::{
    collect_encoding_report, EncodingDoctorReport, EncodingRiskLevel,
};
use crate::core::plugin_config_loader;
use crate::core::sys_env::get_environment_info;
use crate::utils::i18n::t;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const E_DOCTOR_WORKSPACE_SERIALIZE: &str = "E_DOCTOR_WORKSPACE_SERIALIZE";
const E_DOCTOR_WORKSPACE_CURRENT_DIR: &str = "E_DOCTOR_WORKSPACE_CURRENT_DIR";
const E_DOCTOR_WORKSPACE_WRITE_CONTEXT: &str = "E_DOCTOR_WORKSPACE_WRITE_CONTEXT";
const E_DOCTOR_WORKSPACE_CREATE_AUDIT_DIR: &str = "E_DOCTOR_WORKSPACE_CREATE_AUDIT_DIR";
const E_DOCTOR_WORKSPACE_WRITE_AUDIT: &str = "E_DOCTOR_WORKSPACE_WRITE_AUDIT";

pub fn run_workspace_doctor(format: WorkspaceReportFormat, strict: bool) -> Result<String, String> {
    let mut report = collect_workspace_report();
    if strict && report.risk == WorkspaceRiskLevel::Ok {
        // In strict mode, upgrade Warn-level signals to overall risk
        if report.encoding_risk != WorkspaceRiskLevel::Ok {
            report.risk = report.encoding_risk.clone();
        }
        if report.project.primary == "unknown" {
            report.risk = WorkspaceRiskLevel::Warn;
        }
    }
    match format {
        WorkspaceReportFormat::Json => serde_json::to_string_pretty(&report)
            .map_err(|e| format!("{E_DOCTOR_WORKSPACE_SERIALIZE}:{e}")),
        WorkspaceReportFormat::Text => Ok(render_workspace_text(&report)),
        WorkspaceReportFormat::Llm | WorkspaceReportFormat::JsonMin => {
            let risk = match report.risk {
                WorkspaceRiskLevel::Ok => "ok",
                WorkspaceRiskLevel::Warn => "warn",
                WorkspaceRiskLevel::Fail => "fail",
            };
            let enc_risk = match report.encoding_risk {
                WorkspaceRiskLevel::Ok => "ok",
                WorkspaceRiskLevel::Warn => "warn",
                WorkspaceRiskLevel::Fail => "fail",
            };
            let compact = WorkspaceLlmCompact {
                r: risk,
                enc_risk,
                os: &report.os,
                sh: &report.shell,
                enc: &report.encoding,
                enc_mixed: true,
                proj: &report.project.primary,
                fwk: report.project.framework.as_deref(),
                pkg: report.project.package_manager.as_deref(),
                ide: {
                    let mut v = Vec::new();
                    if report.ide.vscode {
                        v.push("vscode");
                    }
                    if report.ide.idea {
                        v.push("idea");
                    }
                    if report.ide.visual_studio {
                        v.push("visual_studio");
                    }
                    if report.ide.xcode {
                        v.push("xcode");
                    }
                    if report.ide.cursor {
                        v.push("cursor");
                    }
                    if report.ide.neovim {
                        v.push("neovim");
                    }
                    if report.ide.eclipse {
                        v.push("eclipse");
                    }
                    if report.ide.sublime {
                        v.push("sublime");
                    }
                    if report.ide.android_studio {
                        v.push("android-studio");
                    }
                    if v.is_empty() {
                        v.push("unknown");
                    }
                    v
                },
                repo: WorkspaceLlmRepo {
                    v: if report.repo.git { "git" } else { "no-git" },
                    b: report.repo.git_branch.as_deref().unwrap_or("n/a"),
                    d: report.repo.git_dirty,
                    svn: report.repo.svn,
                    hg: report.repo.hg,
                    p4: report.repo.p4,
                    cvs: report.repo.cvs,
                    bzr: report.repo.bzr,
                    fossil: report.repo.fossil,
                    darcs: report.repo.darcs,
                },
                act: &report.actions,
                plugins: report.plugins.iter().map(|p| p.name.as_str()).collect(),
            };
            serde_json::to_string(&compact)
                .map_err(|e| format!("{E_DOCTOR_WORKSPACE_SERIALIZE}:{e}"))
        }
    }
}

pub fn collect_workspace_report() -> WorkspaceDoctorReport {
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let env = get_environment_info();
    let enc = collect_encoding_report();

    let project = detect_project(&cwd);
    let tools = detect_tools();
    let ide = detect_ide(&cwd);
    let repo = detect_repo(&cwd);

    let actions = build_workspace_actions(&cwd, &project, &tools, &enc.risk);
    let risk = resolve_workspace_risk(&project.primary, &enc.risk);
    let encoding_risk = map_encoding_risk(&enc.risk);
    let shell = enc
        .shell
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let encoding = resolve_workspace_encoding(&enc);

    let config_dir = std::path::Path::new("config").join("plugins");
    let plugins = plugin_config_loader::get_all_plugin_capabilities(if config_dir.exists() {
        Some(&config_dir)
    } else {
        None
    });

    WorkspaceDoctorReport {
        risk,
        encoding_risk,
        os: format!("{} {}", env.os, env.os_version).trim().to_string(),
        shell,
        encoding,
        project,
        tools,
        ide,
        repo,
        actions,
        plugins,
    }
}

fn map_encoding_risk(risk: &EncodingRiskLevel) -> WorkspaceRiskLevel {
    match risk {
        EncodingRiskLevel::Fail => WorkspaceRiskLevel::Fail,
        EncodingRiskLevel::Warn => WorkspaceRiskLevel::Warn,
        EncodingRiskLevel::Ok => WorkspaceRiskLevel::Ok,
    }
}

fn resolve_workspace_risk(
    project_primary: &str,
    enc_risk: &EncodingRiskLevel,
) -> WorkspaceRiskLevel {
    if project_primary == "unknown" {
        WorkspaceRiskLevel::Warn
    } else {
        map_encoding_risk(enc_risk)
    }
}

fn resolve_workspace_encoding(enc: &EncodingDoctorReport) -> String {
    if enc.codepage.as_ref().and_then(|c| c.is_utf8) == Some(true) {
        "utf8".to_string()
    } else {
        enc.codepage
            .as_ref()
            .and_then(|c| c.value.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

fn build_workspace_actions(
    cwd: &Path,
    project: &ProjectInfo,
    tools: &ToolVersions,
    enc_risk: &EncodingRiskLevel,
) -> Vec<String> {
    let mut actions = Vec::new();
    if project.primary == "unknown" {
        actions.push("detect-project-manually".to_string());
    }
    if tools.rust.is_none() && project.primary == "rust" {
        actions.push("install-rust-toolchain".to_string());
    }
    match enc_risk {
        EncodingRiskLevel::Warn => actions.push("encoding-review".to_string()),
        EncodingRiskLevel::Fail => actions.push("encoding-fix-required".to_string()),
        EncodingRiskLevel::Ok => {}
    }

    let has = |name: &str| cwd.join(name).exists();
    if has("docker-compose.yml") || has("docker-compose.yaml") {
        actions.push("docker-compose-present".to_string());
    }
    if glob_exists(cwd, "schema.prisma") {
        actions.push("prisma-schema-detected".to_string());
    }
    if actions.is_empty() {
        actions.push("none".to_string());
    }
    actions
}

fn collect_project_languages(cwd: &Path) -> Vec<String> {
    let has = |name: &str| cwd.join(name).exists();
    let mut langs = Vec::<String>::new();
    if has("Cargo.toml") {
        langs.push("rust".to_string());
    }
    if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
        langs.push("java".to_string());
    }
    if has("package.json") {
        langs.push("node".to_string());
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        langs.push("python".to_string());
    }
    if has("CMakeLists.txt") || has("Makefile") {
        langs.push("cpp".to_string());
    }
    if glob_exists(cwd, ".sln") || glob_exists(cwd, ".csproj") {
        langs.push("csharp".to_string());
    }
    if has("go.mod") {
        langs.push("go".to_string());
    }
    if has("gemfile") || has("Gemfile") {
        langs.push("ruby".to_string());
    }
    if has("composer.json") {
        langs.push("php".to_string());
    }
    if has("pubspec.yaml") {
        langs.push("dart".to_string());
    }
    if glob_exists(cwd, ".xcodeproj")
        || glob_exists(cwd, ".xcworkspace")
        || has("Package.swift")
        || has("Podfile")
    {
        langs.push("swift".to_string());
    }
    if glob_exists(cwd, "AndroidManifest.xml")
        || (has("app") && cwd.join("app").join("build.gradle").exists())
        || (has("app") && cwd.join("app").join("build.gradle.kts").exists())
    {
        langs.push("kotlin".to_string());
    }
    // Additional languages from spec
    if has("build.sbt") {
        langs.push("scala".to_string());
    }
    if has("mix.exs") {
        langs.push("elixir".to_string());
    }
    if has("stack.yaml") || glob_exists(cwd, ".cabal") {
        langs.push("haskell".to_string());
    }
    if glob_exists(cwd, ".rockspec") || glob_exists(cwd, ".lua") {
        langs.push("lua".to_string());
    }
    if has("build.zig") {
        langs.push("zig".to_string());
    }
    // Embedded/IoT detection
    if has("platformio.ini") {
        langs.push("embedded".to_string());
    }
    if has("Arduino.ino") || glob_exists(cwd, ".ino") {
        langs.push("arduino".to_string());
    }
    // R / Data Science
    if has("DESCRIPTION") || has("renv.lock") || glob_exists(cwd, ".R") {
        langs.push("r".to_string());
    }
    // MATLAB
    if glob_exists(cwd, ".m") || has("Project.prj") {
        langs.push("matlab".to_string());
    }
    // Perl
    if has("Makefile.PL") || has("cpanfile") || glob_exists(cwd, ".pl") {
        langs.push("perl".to_string());
    }
    // Shell scripting
    if glob_exists(cwd, ".sh") || has("Makefile") {
        langs.push("shell".to_string());
    }
    // C (independent from C++)
    if (has("CMakeLists.txt") || has("Makefile")) && !has("package.json") && !has("Cargo.toml") {
        // Check for .c files without .cpp
        let has_c = glob_exists(cwd, ".c");
        let has_cpp = glob_exists(cwd, ".cpp") || glob_exists(cwd, ".cc");
        if has_c && !has_cpp {
            langs.push("c".to_string());
        }
    }
    // SQL / DBA (via migration/ORM files)
    if has("flyway.conf") || glob_exists(cwd, "V*.sql") {
        langs.push("sql".to_string());
    }
    if has("alembic.ini") || has("knexfile.js") || has("prisma/schema.prisma") {
        langs.push("sql".to_string());
    }
    // Julia
    if has("Project.toml") || has("Manifest.toml") || glob_exists(cwd, ".jl") {
        langs.push("julia".to_string());
    }
    // Objective-C
    if glob_exists(cwd, ".m") && glob_exists(cwd, ".h") && !glob_exists(cwd, ".swift") {
        langs.push("objc".to_string());
    }
    // Groovy
    if has("Jenkinsfile") || glob_exists(cwd, ".groovy") || has("gradle.properties") {
        langs.push("groovy".to_string());
    }
    // Erlang
    if has("rebar.config") || glob_exists(cwd, ".erl") {
        langs.push("erlang".to_string());
    }
    // Fortran
    if glob_exists(cwd, ".f90") || glob_exists(cwd, ".f95") || glob_exists(cwd, ".f") {
        langs.push("fortran".to_string());
    }
    // COBOL
    if glob_exists(cwd, ".cob") || glob_exists(cwd, ".cbl") {
        langs.push("cobol".to_string());
    }
    langs
}

fn detect_project(cwd: &Path) -> ProjectInfo {
    let has = |name: &str| cwd.join(name).exists();
    let langs = collect_project_languages(cwd);
    let primary = langs
        .first()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let secondary = if langs.len() > 1 {
        langs[1..].to_vec()
    } else {
        Vec::new()
    };

    let mut framework = None;
    let mut package_manager = None;

    if primary == "node" || secondary.iter().any(|s| s == "node") {
        if has("pnpm-lock.yaml") {
            package_manager = Some("pnpm".to_string());
        } else if has("yarn.lock") {
            package_manager = Some("yarn".to_string());
        } else if has("bun.lockb") || has("bun.lock") {
            package_manager = Some("bun".to_string());
        } else {
            package_manager = Some("npm".to_string());
        }
    } else if primary == "python" || secondary.iter().any(|s| s == "python") {
        if has("Pipfile") {
            package_manager = Some("pipenv".to_string());
        } else if has("poetry.lock") {
            package_manager = Some("poetry".to_string());
        } else if has("uv.lock") {
            package_manager = Some("uv".to_string());
        } else if has("conda-lock.yml") || has("environment.yml") {
            package_manager = Some("conda".to_string());
        } else {
            package_manager = Some("pip".to_string());
        }
    } else if primary == "java" || secondary.iter().any(|s| s == "java") {
        if has("pom.xml") {
            package_manager = Some("maven".to_string());
        } else if has("build.gradle") || has("build.gradle.kts") {
            package_manager = Some("gradle".to_string());
        }
    } else if primary == "csharp" || secondary.iter().any(|s| s == "csharp") {
        package_manager = Some("nuget".to_string());
    } else if primary == "swift" || secondary.iter().any(|s| s == "swift") {
        package_manager = Some("spm".to_string());
    } else if primary == "ruby" || secondary.iter().any(|s| s == "ruby") {
        package_manager = Some("bundler".to_string());
    } else if primary == "php" || secondary.iter().any(|s| s == "php") {
        package_manager = Some("composer".to_string());
    } else if primary == "go" || secondary.iter().any(|s| s == "go") {
        package_manager = Some("go modules".to_string());
    } else if primary == "rust" || secondary.iter().any(|s| s == "rust") {
        package_manager = Some("cargo".to_string());
    } else if primary == "dart" || secondary.iter().any(|s| s == "dart") {
        package_manager = Some("pub".to_string());
    } else if primary == "elixir" || secondary.iter().any(|s| s == "elixir") {
        package_manager = Some("hex".to_string());
    } else if primary == "haskell" || secondary.iter().any(|s| s == "haskell") {
        package_manager = Some("cabal".to_string());
    } else if primary == "scala" || secondary.iter().any(|s| s == "scala") {
        package_manager = Some("sbt".to_string());
    } else if primary == "julia" || secondary.iter().any(|s| s == "julia") {
        package_manager = Some("julia pkg".to_string());
    }

    if primary == "node" || secondary.iter().any(|s| s == "node") {
        let pkg_content = std::fs::read_to_string(cwd.join("package.json")).unwrap_or_default();

        if pkg_content.contains("\"react-native\"")
            || has("app.json") && pkg_content.contains("\"expo\"")
        {
            framework = Some("react-native".to_string());
        } else if pkg_content.contains("\"electron\"") || has("electron-builder.json") {
            framework = Some("electron".to_string());
        } else if has("next.config.js") || has("next.config.mjs") || has("next.config.ts") {
            framework = Some("nextjs".to_string());
        } else if has("nuxt.config.js") || has("nuxt.config.ts") {
            framework = Some("nuxt".to_string());
        } else if has("vue.config.js") || has("vite.config.js") || has("vite.config.ts") {
            // Check if vue is in package.json to be sure it's vue
            if pkg_content.contains("\"vue\"") || has("vue.config.js") {
                framework = Some("vue".to_string());
            } else if has("vite.config.js") || has("vite.config.ts") {
                framework = Some("vite".to_string());
            }
        } else if has("svelte.config.js") {
            framework = Some("svelte".to_string());
        } else if has("angular.json") {
            framework = Some("angular".to_string());
        } else if pkg_content.contains("\"express\"") {
            framework = Some("express".to_string());
        } else if has("astro.config.mjs") || has("astro.config.ts") {
            framework = Some("astro".to_string());
        } else if has("remix.config.js") || has("remix.config.ts") {
            framework = Some("remix".to_string());
        } else if pkg_content.contains("\"solid-js\"") {
            framework = Some("solid".to_string());
        }

        if has("turbo.json") {
            framework = Some(
                format!("{}/turbo", framework.unwrap_or_default())
                    .trim_start_matches('/')
                    .to_string(),
            );
        } else if has("nx.json") {
            framework = Some(
                format!("{}/nx", framework.unwrap_or_default())
                    .trim_start_matches('/')
                    .to_string(),
            );
        }
    } else if primary == "rust" || secondary.iter().any(|s| s == "rust") {
        if has("src-tauri/tauri.conf.json") || has("src-tauri/tauri.conf.json5") {
            framework = Some("tauri".to_string());
        }
    } else if primary == "dart" || secondary.iter().any(|s| s == "dart") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("pubspec.yaml")) {
            if content.contains("sdk: flutter") || content.contains("flutter:") {
                framework = Some("flutter".to_string());
            }
        }
    } else if primary == "kotlin" || secondary.iter().any(|s| s == "kotlin") {
        framework = Some("android".to_string());
    } else if primary == "swift" || secondary.iter().any(|s| s == "swift") {
        framework = Some("ios/macos".to_string());
    } else if primary == "python" || secondary.iter().any(|s| s == "python") {
        let reqs = std::fs::read_to_string(cwd.join("requirements.txt")).unwrap_or_default();
        let pyproj = std::fs::read_to_string(cwd.join("pyproject.toml")).unwrap_or_default();
        if has("manage.py") {
            framework = Some("django".to_string());
        } else if reqs.contains("fastapi") || pyproj.contains("fastapi") {
            framework = Some("fastapi".to_string());
        } else if reqs.contains("flask") || pyproj.contains("flask") {
            framework = Some("flask".to_string());
        }
    } else if primary == "java" || secondary.iter().any(|s| s == "java") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("pom.xml")) {
            if content.contains("spring-boot") {
                // Detect Spring Boot 2.x vs 3.x
                let version = if content.contains("spring-boot-starter-parent")
                    && content.contains("3.")
                {
                    "spring-boot-3".to_string()
                } else if content.contains("spring-boot-starter-parent") && content.contains("2.") {
                    "spring-boot-2".to_string()
                } else {
                    "spring-boot".to_string()
                };
                framework = Some(version);
            }
        }
        // Also check build.gradle
        if framework.is_none() {
            if let Ok(content) = std::fs::read_to_string(cwd.join("build.gradle")) {
                if content.contains("spring-boot") {
                    framework = Some("spring-boot".to_string());
                }
            }
        }
    } else if primary == "csharp" || secondary.iter().any(|s| s == "csharp") {
        framework = Some("dotnet".to_string());
    } else if primary == "php" || secondary.iter().any(|s| s == "php") {
        if has("artisan") {
            framework = Some("laravel".to_string());
        }
    } else if primary == "ruby" || secondary.iter().any(|s| s == "ruby") {
        if has("bin/rails") || has("config/application.rb") {
            framework = Some("rails".to_string());
        }
    } else if primary == "node" || secondary.iter().any(|s| s == "node") {
        if let Ok(pkg) = std::fs::read_to_string(cwd.join("package.json")) {
            if pkg.contains("\"@nestjs/core\"") || has("nest-cli.json") {
                framework = Some("nestjs".to_string());
            }
        }
    } else if primary == "csharp" || secondary.iter().any(|s| s == "csharp") {
        framework = Some("aspnetcore".to_string());
    } else if primary == "elixir" || secondary.iter().any(|s| s == "elixir") {
        if has("mix.exs") {
            if let Ok(content) = std::fs::read_to_string(cwd.join("mix.exs")) {
                if content.contains("phoenix") {
                    framework = Some("phoenix".to_string());
                }
            }
        }
    } else if primary == "go" || secondary.iter().any(|s| s == "go") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("go.mod")) {
            if content.contains("gin-gonic") {
                framework = Some("gin".to_string());
            } else if content.contains("labstack/echo") {
                framework = Some("echo".to_string());
            } else if content.contains("gofiber") {
                framework = Some("fiber".to_string());
            }
        }
    } else if primary == "cpp" || secondary.iter().any(|s| s == "cpp") {
        if has("CMakeLists.txt") {
            if let Ok(content) = std::fs::read_to_string(cwd.join("CMakeLists.txt")) {
                if content.contains("Qt") || content.contains("QT") {
                    framework = Some("qt".to_string());
                } else if content.contains("GTK") || content.contains("gtk") {
                    framework = Some("gtk".to_string());
                }
            }
        }
    } else if primary == "csharp" || secondary.iter().any(|s| s == "csharp") {
        if glob_exists(cwd, ".xaml") || has("App.xaml") {
            framework = Some("wpf".to_string());
        } else {
            framework = Some("winforms".to_string());
        }
    }

    let (mut build, mut test) = match primary.as_str() {
        "rust" => ("cargo build".to_string(), "cargo test".to_string()),
        "java" => (
            "mvn -q test | ./gradlew test".to_string(),
            "mvn -q test".to_string(),
        ),
        "node" => {
            let pm = package_manager.as_deref().unwrap_or("npm");
            (format!("{} run build", pm), format!("{} test", pm))
        }
        "python" => (
            "python -m pip install -r requirements.txt".to_string(),
            "pytest".to_string(),
        ),
        "cpp" => ("cmake --build .".to_string(), "ctest".to_string()),
        "csharp" => ("dotnet build".to_string(), "dotnet test".to_string()),
        "go" => ("go build ./...".to_string(), "go test ./...".to_string()),
        "ruby" => (
            "bundle install".to_string(),
            "bundle exec rspec".to_string(),
        ),
        "php" => ("composer install".to_string(), "composer test".to_string()),
        "dart" => {
            if framework.as_deref() == Some("flutter") {
                ("flutter build apk".to_string(), "flutter test".to_string())
            } else {
                ("dart pub get".to_string(), "dart test".to_string())
            }
        }
        "swift" => {
            if has("Package.swift") {
                ("swift build".to_string(), "swift test".to_string())
            } else {
                ("xcodebuild".to_string(), "xcodebuild test".to_string())
            }
        }
        "kotlin" => (
            "./gradlew assembleDebug".to_string(),
            "./gradlew test".to_string(),
        ),
        _ => ("unknown".to_string(), "unknown".to_string()),
    };

    if framework.as_deref() == Some("tauri") {
        let pm = package_manager.as_deref().unwrap_or("cargo");
        if pm == "cargo" {
            build = "cargo tauri build".to_string();
        } else {
            build = format!("{} run tauri build", pm);
        }
    } else if framework.as_deref() == Some("react-native") || framework.as_deref() == Some("expo") {
        let pm = package_manager.as_deref().unwrap_or("npm");
        build = format!("{} run android", pm); // Defaulting to one of them
    } else if let Some(fwk) = framework.as_deref() {
        if fwk.ends_with("/turbo") {
            let pm = package_manager.as_deref().unwrap_or("npx");
            build = format!("{} turbo run build", pm);
            test = format!("{} turbo run test", pm);
        } else if fwk.ends_with("/nx") {
            let pm = package_manager.as_deref().unwrap_or("npx");
            build = format!("{} nx build", pm);
            test = format!("{} nx test", pm);
        }
    }

    // Detect version dialect
    let dialect = detect_dialect(cwd, &primary, &framework);
    // Detect database from ORM/migration files
    let database = detect_database(cwd);
    // Detect module system (ESM vs CJS)
    let module_system = detect_module_system(cwd, &primary);

    ProjectInfo {
        primary,
        secondary,
        framework,
        package_manager,
        build,
        test,
        dialect,
        database,
        module_system,
    }
}

/// Detect version dialect (Spring 2/3, C++ standard, Python version, etc.)
fn detect_dialect(cwd: &Path, primary: &str, _framework: &Option<String>) -> Option<String> {
    let has = |name: &str| cwd.join(name).exists();
    match primary {
        "python" => {
            // Check pyproject.toml for requires-python
            if let Ok(content) = std::fs::read_to_string(cwd.join("pyproject.toml")) {
                if content.contains("requires-python") {
                    // Check specific version ranges
                    if content.contains(">=3.12") || content.contains(">= 3.12") {
                        return Some("python-3.12+".to_string());
                    }
                    if content.contains(">=3.11") || content.contains(">= 3.11") {
                        return Some("python-3.11+".to_string());
                    }
                    if content.contains(">=3.10") || content.contains(">= 3.10") {
                        return Some("python-3.10+".to_string());
                    }
                    if content.contains(">=3.9") || content.contains(">= 3.9") {
                        return Some("python-3.9+".to_string());
                    }
                    if content.contains(">=3.8") || content.contains(">= 3.8") {
                        return Some("python-3.8+".to_string());
                    }
                    if content.contains(">=3.7") || content.contains(">= 3.7") {
                        return Some("python-3.7+".to_string());
                    }
                    if content.contains(">=3.6") || content.contains(">= 3.6") {
                        return Some("python-3.6+".to_string());
                    }
                    if content.contains(">=3.5") || content.contains(">= 3.5") {
                        return Some("python-3.5+".to_string());
                    }
                    if content.contains(">=2.7") || content.contains(">= 2.7") {
                        return Some("python-2.7".to_string());
                    }
                }
            }
            // Check .python-version file (pyenv)
            if let Ok(content) = std::fs::read_to_string(cwd.join(".python-version")) {
                let v = content.trim();
                if !v.is_empty() {
                    return Some(format!("python-{}", v));
                }
            }
            // Check runtime version (python3 first, then python)
            for cmd in &["python3", "python"] {
                if let Ok(output) = std::process::Command::new(cmd).args(["--version"]).output() {
                    let v = String::from_utf8_lossy(&output.stdout);
                    let v = if v.is_empty() {
                        String::from_utf8_lossy(&output.stderr)
                    } else {
                        v
                    };
                    // Extract version like "Python 3.8.10"
                    if let Some(ver) = v.lines().next() {
                        let ver = ver.trim();
                        if ver.contains("Python 2.7") {
                            return Some("python-2.7".to_string());
                        }
                        if ver.contains("Python 3.6") {
                            return Some("python-3.6".to_string());
                        }
                        if ver.contains("Python 3.7") {
                            return Some("python-3.7".to_string());
                        }
                        if ver.contains("Python 3.8") {
                            return Some("python-3.8".to_string());
                        }
                        if ver.contains("Python 3.9") {
                            return Some("python-3.9".to_string());
                        }
                        if ver.contains("Python 3.10") {
                            return Some("python-3.10".to_string());
                        }
                        if ver.contains("Python 3.11") {
                            return Some("python-3.11".to_string());
                        }
                        if ver.contains("Python 3.12") {
                            return Some("python-3.12".to_string());
                        }
                        if ver.contains("Python 3.13") {
                            return Some("python-3.13".to_string());
                        }
                        if ver.contains("Python 3.") {
                            // Generic 3.x
                            return Some("python-3.x".to_string());
                        }
                        if ver.contains("Python 2.") {
                            return Some("python-2.x".to_string());
                        }
                    }
                }
            }
            // Check for Python 2.7 legacy markers
            if has("requirements.txt") {
                if let Ok(content) = std::fs::read_to_string(cwd.join("requirements.txt")) {
                    if content.contains("# Python 2.7") || content.contains("# python2") {
                        return Some("python-2.7".to_string());
                    }
                }
            }
            None
        }
        "cpp" | "c" => {
            // Check CMakeLists.txt for CXX_STANDARD
            if let Ok(content) = std::fs::read_to_string(cwd.join("CMakeLists.txt")) {
                for line in content.lines() {
                    if line.contains("CMAKE_CXX_STANDARD") {
                        if line.contains("20") {
                            return Some("c++20".to_string());
                        }
                        if line.contains("17") {
                            return Some("c++17".to_string());
                        }
                        if line.contains("14") {
                            return Some("c++14".to_string());
                        }
                        if line.contains("11") {
                            return Some("c++11".to_string());
                        }
                    }
                    if line.contains("CMAKE_C_STANDARD") {
                        if line.contains("11") {
                            return Some("c11".to_string());
                        }
                        if line.contains("99") {
                            return Some("c99".to_string());
                        }
                    }
                }
            }
            // Check Makefile for -std flag
            if let Ok(content) = std::fs::read_to_string(cwd.join("Makefile")) {
                if let Some(line) = content.lines().find(|l| l.contains("-std=c++")) {
                    if line.contains("c++20") {
                        return Some("c++20".to_string());
                    }
                    if line.contains("c++17") {
                        return Some("c++17".to_string());
                    }
                    if line.contains("c++14") {
                        return Some("c++14".to_string());
                    }
                    if line.contains("c++11") {
                        return Some("c++11".to_string());
                    }
                }
            }
            None
        }
        "rust" => {
            // Check Cargo.toml for edition
            if let Ok(content) = std::fs::read_to_string(cwd.join("Cargo.toml")) {
                for line in content.lines() {
                    if line.starts_with("edition") {
                        if line.contains("2024") {
                            return Some("rust-2024".to_string());
                        }
                        if line.contains("2021") {
                            return Some("rust-2021".to_string());
                        }
                        if line.contains("2018") {
                            return Some("rust-2018".to_string());
                        }
                        if line.contains("2015") {
                            return Some("rust-2015".to_string());
                        }
                    }
                }
            }
            None
        }
        "go" => {
            // Check go.mod for go version
            if let Ok(content) = std::fs::read_to_string(cwd.join("go.mod")) {
                for line in content.lines() {
                    if line.starts_with("go ") {
                        return Some(format!("go-{}", line.trim_start_matches("go ").trim()));
                    }
                }
            }
            None
        }
        "node" => {
            // Check package.json for type (esm/cjs) and engines
            if let Ok(content) = std::fs::read_to_string(cwd.join("package.json")) {
                let mut module = None;
                if content.contains("\"type\": \"module\"") {
                    module = Some("esm".to_string());
                } else if content.contains("\"type\": \"commonjs\"") {
                    module = Some("cjs".to_string());
                }
                // Check engines.node for version constraint
                if content.contains("\"node\"") {
                    if content.contains(">=18") || content.contains(">= 18") {
                        return Some(format!("node-18+ ({})", module.unwrap_or_default()));
                    }
                    if content.contains(">=16") || content.contains(">= 16") {
                        return Some(format!("node-16+ ({})", module.unwrap_or_default()));
                    }
                    if content.contains(">=14") || content.contains(">= 14") {
                        return Some(format!("node-14+ ({})", module.unwrap_or_default()));
                    }
                    if content.contains(">=12") || content.contains(">= 12") {
                        return Some(format!("node-12+ ({})", module.unwrap_or_default()));
                    }
                    if content.contains(">=10") || content.contains(">= 10") {
                        return Some(format!("node-10+ ({})", module.unwrap_or_default()));
                    }
                }
                if let Some(m) = module {
                    return Some(m);
                }
            }
            // Check .nvmrc or .node-version
            for file in &[".nvmrc", ".node-version"] {
                if let Ok(content) = std::fs::read_to_string(cwd.join(file)) {
                    let v = content.trim().trim_start_matches('v');
                    if !v.is_empty() && v != "lts/*" {
                        return Some(format!("node-{}", v));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Detect database type from ORM/migration files
fn detect_database(cwd: &Path) -> Option<String> {
    let has = |name: &str| cwd.join(name).exists();
    // Prisma
    if has("prisma/schema.prisma") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("prisma/schema.prisma")) {
            if content.contains("provider = \"postgresql\"") {
                return Some("postgresql".to_string());
            }
            if content.contains("provider = \"mysql\"") {
                return Some("mysql".to_string());
            }
            if content.contains("provider = \"sqlite\"") {
                return Some("sqlite".to_string());
            }
            if content.contains("provider = \"sqlserver\"") {
                return Some("sqlserver".to_string());
            }
            if content.contains("provider = \"mongodb\"") {
                return Some("mongodb".to_string());
            }
        }
    }
    // Flyway
    if has("flyway.conf") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("flyway.conf")) {
            if content.contains("jdbc:postgresql") {
                return Some("postgresql".to_string());
            }
            if content.contains("jdbc:mysql") {
                return Some("mysql".to_string());
            }
            if content.contains("jdbc:oracle") {
                return Some("oracle".to_string());
            }
            if content.contains("jdbc:sqlserver") {
                return Some("sqlserver".to_string());
            }
            if content.contains("jdbc:db2") {
                return Some("db2".to_string());
            }
        }
    }
    // Alembic (Python/SQLAlchemy)
    if has("alembic.ini") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("alembic.ini")) {
            if content.contains("postgresql") {
                return Some("postgresql".to_string());
            }
            if content.contains("mysql") {
                return Some("mysql".to_string());
            }
            if content.contains("sqlite") {
                return Some("sqlite".to_string());
            }
            if content.contains("oracle") {
                return Some("oracle".to_string());
            }
        }
    }
    // Knex (Node.js)
    if has("knexfile.js") || has("knexfile.ts") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("knexfile.js"))
            .or_else(|_| std::fs::read_to_string(cwd.join("knexfile.ts")))
        {
            if content.contains("client: 'pg'") || content.contains("client:\"pg\"") {
                return Some("postgresql".to_string());
            }
            if content.contains("client: 'mysql'") || content.contains("client:\"mysql\"") {
                return Some("mysql".to_string());
            }
            if content.contains("client: 'sqlite3'") {
                return Some("sqlite".to_string());
            }
            if content.contains("client: 'oracledb'") {
                return Some("oracle".to_string());
            }
        }
    }
    // Django settings
    if has("manage.py") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("settings.py")) {
            if content.contains("django.db.backends.postgresql") {
                return Some("postgresql".to_string());
            }
            if content.contains("django.db.backends.mysql") {
                return Some("mysql".to_string());
            }
            if content.contains("django.db.backends.sqlite3") {
                return Some("sqlite".to_string());
            }
            if content.contains("django.db.backends.oracle") {
                return Some("oracle".to_string());
            }
        }
    }
    // Liquibase
    if has("liquibase.properties") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("liquibase.properties")) {
            if content.contains("jdbc:postgresql") {
                return Some("postgresql".to_string());
            }
            if content.contains("jdbc:mysql") {
                return Some("mysql".to_string());
            }
            if content.contains("jdbc:oracle") {
                return Some("oracle".to_string());
            }
            if content.contains("jdbc:mariadb") {
                return Some("mariadb".to_string());
            }
        }
    }
    // Redis
    if has("redis.conf") || has("docker-compose.yml") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("docker-compose.yml")) {
            if content.contains("redis:") || content.contains("redis:") {
                return Some("redis".to_string());
            }
        }
    }
    // Elasticsearch
    if has("elasticsearch.yml") {
        return Some("elasticsearch".to_string());
    }
    // ClickHouse
    if has("clickhouse-config.xml") || glob_exists(cwd, ".clickhouse") {
        return Some("clickhouse".to_string());
    }
    // Cassandra
    if has("cassandra.yaml") || has("cassandra.yml") {
        return Some("cassandra".to_string());
    }
    // MariaDB (via docker-compose or config)
    if has("docker-compose.yml") {
        if let Ok(content) = std::fs::read_to_string(cwd.join("docker-compose.yml")) {
            if content.contains("mariadb:") {
                return Some("mariadb".to_string());
            }
        }
    }
    None
}

/// Detect module system (ESM vs CJS for Node.js)
fn detect_module_system(cwd: &Path, primary: &str) -> Option<String> {
    if primary != "node" {
        return None;
    }
    if let Ok(content) = std::fs::read_to_string(cwd.join("package.json")) {
        if content.contains("\"type\": \"module\"") {
            return Some("esm".to_string());
        }
        if content.contains("\"type\": \"commonjs\"") {
            return Some("cjs".to_string());
        }
    }
    // .mjs files = ESM, .cjs files = CJS
    if glob_exists(cwd, ".mjs") && !glob_exists(cwd, ".cjs") {
        return Some("esm".to_string());
    }
    if glob_exists(cwd, ".cjs") && !glob_exists(cwd, ".mjs") {
        return Some("cjs".to_string());
    }
    None
}

fn glob_exists(cwd: &Path, suffix: &str) -> bool {
    std::fs::read_dir(cwd)
        .ok()
        .into_iter()
        .flat_map(|it| it.flatten())
        .any(|e| e.path().to_string_lossy().ends_with(suffix))
}

fn detect_tools() -> ToolVersions {
    ToolVersions {
        rust: detect_cmd_version("rustc", &["--version"]),
        node: detect_cmd_version("node", &["-v"]),
        python: detect_cmd_version("python", &["--version"])
            .or_else(|| detect_cmd_version("python3", &["--version"])),
        java: detect_cmd_version("java", &["-version"]),
        gcc: detect_cmd_version("gcc", &["--version"])
            .or_else(|| detect_cmd_version("cc", &["--version"])),
        clang: detect_cmd_version("clang", &["--version"]),
        deno: detect_cmd_version("deno", &["--version"]),
        msvc: detect_msvc_version(),
        ninja: detect_cmd_version("ninja", &["--version"]),
        bazel: detect_cmd_version("bazel", &["--version"]),
        make: detect_cmd_version("make", &["--version"])
            .or_else(|| detect_cmd_version("gmake", &["--version"])),
        cmake: detect_cmd_version("cmake", &["--version"]),
        meson: detect_cmd_version("meson", &["--version"]),
        julia: detect_cmd_version("julia", &["--version"]),
        dotnet: detect_cmd_version("dotnet", &["--version"]),
        go: detect_cmd_version("go", &["version"]),
        ruby: detect_cmd_version("ruby", &["--version"]),
        php: detect_cmd_version("php", &["--version"]),
        swift: detect_cmd_version("swift", &["--version"]),
        erlang: detect_cmd_version(
            "erl",
            &[
                "-eval",
                "erlang:display(erlang:system_info(otp_release)), halt().",
                "-noshell",
            ],
        ),
        fortran: detect_cmd_version("gfortran", &["--version"])
            .or_else(|| detect_cmd_version("ifort", &["--version"])),
        r_lang: detect_cmd_version("R", &["--version"]),
        perl: detect_cmd_version("perl", &["--version"]),
        lua: detect_cmd_version("lua", &["-v"]),
        elixir: detect_cmd_version("elixir", &["--version"]),
        haskell: detect_cmd_version("ghc", &["--version"]),
        dart: detect_cmd_version("dart", &["--version"]),
        scala: detect_cmd_version("scala", &["-version"]),
        zig: detect_cmd_version("zig", &["version"]),
        groovy: detect_cmd_version("groovy", &["--version"]),
        cobol: detect_cmd_version("cobc", &["--version"]),
    }
}

/// Detect MSVC compiler version via cl.exe or vswhere
fn detect_msvc_version() -> Option<String> {
    // Try cl.exe first
    if let Some(v) = detect_cmd_version("cl", &["/?"]) {
        // cl.exe outputs version to stderr
        return Some(v);
    }
    // Try vswhere on Windows
    if cfg!(target_os = "windows") {
        if let Ok(output) = Command::new("vswhere")
            .args([
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property",
                "installationPath",
            ])
            .output()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                let cl_path = format!("{}\\VC\\Tools\\MSVC", path);
                if let Ok(entries) = std::fs::read_dir(&cl_path) {
                    if let Some(entry) = entries.filter_map(|e| e.ok()).last() {
                        return Some(format!("MSVC {}", entry.file_name().to_string_lossy()));
                    }
                }
            }
        }
    }
    None
}

fn detect_cmd_version(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| {
            let bytes = if o.stdout.is_empty() {
                o.stderr
            } else {
                o.stdout
            };
            decode_tool_version_output(&bytes)
        })
        .filter(|s| !s.is_empty())
}

fn decode_tool_version_output(bytes: &[u8]) -> String {
    let (decoded, _, _) = crate::core::encoding_fallback::decode_and_repair_for_display(bytes);
    decoded.lines().next().unwrap_or("").trim().to_string()
}

fn detect_ide(cwd: &Path) -> IdeInfo {
    let has_idea = cwd.join(".idea").exists() || glob_exists(cwd, ".iml");
    IdeInfo {
        vscode: cwd.join(".vscode").exists(),
        idea: has_idea,
        visual_studio: cwd.join(".vs").exists() || glob_exists(cwd, ".sln"),
        xcode: glob_exists(cwd, ".xcodeproj") || glob_exists(cwd, ".xcworkspace"),
        cursor: cwd.join(".cursor").exists() || cwd.join(".cursorrules").exists(),
        neovim: cwd.join(".nvim").exists() || cwd.join("init.lua").exists(),
        eclipse: cwd.join(".project").exists() || cwd.join(".classpath").exists(),
        sublime: glob_exists(cwd, ".sublime-project"),
        android_studio: has_idea
            && (glob_exists(cwd, "AndroidManifest.xml")
                || (cwd.join("app").exists() && cwd.join("app").join("build.gradle").exists())),
        pycharm: has_idea
            && (cwd.join("requirements.txt").exists()
                || cwd.join("pyproject.toml").exists()
                || cwd.join("setup.py").exists()),
        webstorm: has_idea && cwd.join("package.json").exists(),
        clion: has_idea && (cwd.join("CMakeLists.txt").exists() || cwd.join("Makefile").exists()),
        goland: has_idea && cwd.join("go.mod").exists(),
        rider: has_idea && (glob_exists(cwd, ".csproj") || glob_exists(cwd, ".sln")),
        jupyter: glob_exists(cwd, ".ipynb") || cwd.join("jupyter_notebook_config.py").exists(),
        rstudio: cwd.join(".Rproj.user").exists() || glob_exists(cwd, ".Rproj"),
        emacs: cwd.join(".emacs").exists()
            || cwd.join(".emacs.d").exists()
            || cwd.join("init.el").exists(),
        vim: cwd.join(".vimrc").exists()
            || cwd.join(".vim").exists()
            || cwd.join("init.vim").exists(),
    }
}

fn detect_repo(cwd: &Path) -> RepoInfo {
    let git = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let svn = Command::new("svn")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let hg = Command::new("hg")
        .args(["root"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let p4 = Command::new("p4")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let cvs = cwd.join("CVS").exists();
    let bzr = cwd.join(".bzr").exists();
    let fossil = cwd.join(".fslckout").exists() || cwd.join("_FOSSIL_").exists();
    let darcs = cwd.join("_darcs").exists();

    if !git {
        return RepoInfo {
            git: false,
            git_branch: None,
            git_dirty: None,
            svn,
            hg,
            p4,
            cvs,
            bzr,
            fossil,
            darcs,
        };
    }

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty());

    RepoInfo {
        git: true,
        git_branch: branch,
        git_dirty: dirty,
        svn,
        hg,
        p4,
        cvs,
        bzr,
        fossil,
        darcs,
    }
}

fn translate_action(action: &str) -> String {
    let _key = format!("doctor_action_{}", action.replace("-", "_"));
    let trans = match action {
        "detect-project-manually" => t("doctor_action_detect_project_manually"),
        "install-rust-toolchain" => t("doctor_action_install_rust_toolchain"),
        "encoding-review" => t("doctor_action_encoding_review"),
        "encoding-fix-required" => t("doctor_action_encoding_fix_required"),
        "docker-compose-present" => t("doctor_action_docker_compose_present"),
        "prisma-schema-detected" => t("doctor_action_prisma_schema_detected"),
        "none" => t("doctor_action_none"),
        _ => return action.to_string(),
    };
    if trans.starts_with("doctor_action_") {
        action.to_string()
    } else {
        trans.to_string()
    }
}

fn render_workspace_text(report: &WorkspaceDoctorReport) -> String {
    fn push_field(out: &mut String, label_key: &'static str, value: String) {
        out.push_str(&format!("  {}: {}\n", t(label_key), value));
    }

    let na = t("doctor_workspace_na");
    let none_text = t("doctor_workspace_none");
    let not_found = t("doctor_workspace_not_found");
    let risk = match report.risk {
        WorkspaceRiskLevel::Ok => t("doctor_workspace_risk_ok"),
        WorkspaceRiskLevel::Warn => t("doctor_workspace_risk_warn"),
        WorkspaceRiskLevel::Fail => t("doctor_workspace_risk_fail"),
    };
    let mut out = String::new();
    let secondary = if report.project.secondary.is_empty() {
        none_text.to_string()
    } else {
        report.project.secondary.join(",")
    };

    out.push_str(t("doctor_workspace_report_title"));
    out.push_str("\n================================\n\n");
    out.push_str(&format!(
        "{}: {}\n\n",
        t("doctor_workspace_overall_risk"),
        risk
    ));

    out.push_str(t("doctor_workspace_section_project"));
    out.push('\n');
    push_field(
        &mut out,
        "doctor_workspace_field_primary",
        report.project.primary.clone(),
    );
    push_field(&mut out, "doctor_workspace_field_secondary", secondary);
    push_field(
        &mut out,
        "doctor_workspace_field_framework",
        report
            .project
            .framework
            .as_deref()
            .unwrap_or(na)
            .to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_package_manager",
        report
            .project
            .package_manager
            .as_deref()
            .unwrap_or(na)
            .to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_dialect",
        report.project.dialect.as_deref().unwrap_or(na).to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_database",
        report.project.database.as_deref().unwrap_or(na).to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_module_system",
        report
            .project
            .module_system
            .as_deref()
            .unwrap_or(na)
            .to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_build",
        report.project.build.clone(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_test",
        report.project.test.clone(),
    );
    out.push('\n');

    out.push_str(t("doctor_workspace_section_environment"));
    out.push('\n');
    push_field(&mut out, "doctor_workspace_field_os", report.os.clone());
    push_field(
        &mut out,
        "doctor_workspace_field_shell",
        report.shell.clone(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_encoding",
        report.encoding.clone(),
    );
    out.push('\n');

    out.push_str(t("doctor_workspace_section_tools"));
    out.push('\n');
    let tools = [
        ("doctor_workspace_field_rust", report.tools.rust.as_deref()),
        ("doctor_workspace_field_node", report.tools.node.as_deref()),
        (
            "doctor_workspace_field_python",
            report.tools.python.as_deref(),
        ),
        ("doctor_workspace_field_java", report.tools.java.as_deref()),
        ("doctor_workspace_field_gcc", report.tools.gcc.as_deref()),
        (
            "doctor_workspace_field_clang",
            report.tools.clang.as_deref(),
        ),
        ("doctor_workspace_field_deno", report.tools.deno.as_deref()),
        ("doctor_workspace_field_msvc", report.tools.msvc.as_deref()),
        (
            "doctor_workspace_field_ninja",
            report.tools.ninja.as_deref(),
        ),
        (
            "doctor_workspace_field_bazel",
            report.tools.bazel.as_deref(),
        ),
        ("doctor_workspace_field_make", report.tools.make.as_deref()),
        (
            "doctor_workspace_field_cmake",
            report.tools.cmake.as_deref(),
        ),
        (
            "doctor_workspace_field_meson",
            report.tools.meson.as_deref(),
        ),
        (
            "doctor_workspace_field_julia",
            report.tools.julia.as_deref(),
        ),
        (
            "doctor_workspace_field_dotnet",
            report.tools.dotnet.as_deref(),
        ),
        ("doctor_workspace_field_go", report.tools.go.as_deref()),
        ("doctor_workspace_field_ruby", report.tools.ruby.as_deref()),
        ("doctor_workspace_field_php", report.tools.php.as_deref()),
        (
            "doctor_workspace_field_swift",
            report.tools.swift.as_deref(),
        ),
        (
            "doctor_workspace_field_erlang",
            report.tools.erlang.as_deref(),
        ),
        (
            "doctor_workspace_field_fortran",
            report.tools.fortran.as_deref(),
        ),
        (
            "doctor_workspace_field_r_lang",
            report.tools.r_lang.as_deref(),
        ),
        ("doctor_workspace_field_perl", report.tools.perl.as_deref()),
        ("doctor_workspace_field_lua", report.tools.lua.as_deref()),
        (
            "doctor_workspace_field_elixir",
            report.tools.elixir.as_deref(),
        ),
        (
            "doctor_workspace_field_haskell",
            report.tools.haskell.as_deref(),
        ),
        ("doctor_workspace_field_dart", report.tools.dart.as_deref()),
        (
            "doctor_workspace_field_scala",
            report.tools.scala.as_deref(),
        ),
        ("doctor_workspace_field_zig", report.tools.zig.as_deref()),
        (
            "doctor_workspace_field_groovy",
            report.tools.groovy.as_deref(),
        ),
        (
            "doctor_workspace_field_cobol",
            report.tools.cobol.as_deref(),
        ),
    ];
    for (k, v) in tools {
        push_field(&mut out, k, v.unwrap_or(not_found).to_string());
    }
    out.push('\n');

    out.push_str(t("doctor_workspace_section_ide"));
    out.push('\n');
    let ide_fields = [
        ("doctor_workspace_field_vscode", report.ide.vscode),
        ("doctor_workspace_field_idea", report.ide.idea),
        (
            "doctor_workspace_field_visual_studio",
            report.ide.visual_studio,
        ),
        ("doctor_workspace_field_xcode", report.ide.xcode),
        ("doctor_workspace_field_cursor", report.ide.cursor),
        ("doctor_workspace_field_neovim", report.ide.neovim),
        ("doctor_workspace_field_eclipse", report.ide.eclipse),
        ("doctor_workspace_field_sublime", report.ide.sublime),
        (
            "doctor_workspace_field_android_studio",
            report.ide.android_studio,
        ),
        ("doctor_workspace_field_pycharm", report.ide.pycharm),
        ("doctor_workspace_field_webstorm", report.ide.webstorm),
        ("doctor_workspace_field_clion", report.ide.clion),
        ("doctor_workspace_field_goland", report.ide.goland),
        ("doctor_workspace_field_rider", report.ide.rider),
        ("doctor_workspace_field_jupyter", report.ide.jupyter),
        ("doctor_workspace_field_rstudio", report.ide.rstudio),
        ("doctor_workspace_field_emacs", report.ide.emacs),
        ("doctor_workspace_field_vim", report.ide.vim),
    ];
    for (k, v) in ide_fields {
        push_field(&mut out, k, v.to_string());
    }
    out.push('\n');

    out.push_str(t("doctor_workspace_section_repo"));
    out.push('\n');
    push_field(
        &mut out,
        "doctor_workspace_field_git",
        report.repo.git.to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_branch",
        report.repo.git_branch.as_deref().unwrap_or(na).to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_dirty",
        report
            .repo
            .git_dirty
            .map(|v| v.to_string())
            .unwrap_or_else(|| na.to_string()),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_svn",
        report.repo.svn.to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_hg",
        report.repo.hg.to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_p4",
        report.repo.p4.to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_cvs",
        report.repo.cvs.to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_bzr",
        report.repo.bzr.to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_fossil",
        report.repo.fossil.to_string(),
    );
    push_field(
        &mut out,
        "doctor_workspace_field_darcs",
        report.repo.darcs.to_string(),
    );
    out.push('\n');

    out.push_str(t("doctor_workspace_section_actions"));
    out.push('\n');
    for (i, action) in report.actions.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, translate_action(action)));
    }
    out.push('\n');

    out.push_str("## Plugin Capabilities\n");
    out.push_str("  (Run `tokenslim plugins` to view detailed plugin capabilities)\n\n");

    out.push_str(t("doctor_workspace_section_ai_context"));
    out.push('\n');
    out.push_str(&format!("  {}\n", t("doctor_workspace_ai_context_line1")));
    out.push_str(&format!("  {}\n", t("doctor_workspace_ai_context_line2")));
    out.push_str(&format!("  {}\n", t("doctor_workspace_ai_context_line3")));
    append_raw_ai_command_findings(&mut out, report, Path::new("."));

    out
}

/// Generate a persistent context file (.tokenslim-context.md) for LLM injection.
/// This file should be committed to the repo and read by LLMs before generating code.
pub fn generate_context_file() -> Result<String, String> {
    let report = collect_workspace_report();
    let mut out = String::new();
    out.push_str("# TokenSlim Workspace Context (AUTO-GENERATED)\n");
    out.push_str("# DO NOT EDIT MANUALLY - run `tokenslim workspace --inject` to update\n\n");

    out.push_str("## Environment\n");
    out.push_str(&format!("- OS: {}\n", report.os));
    out.push_str(&format!("- Shell: {}\n", report.shell));
    out.push_str(&format!("- Encoding: {}\n", report.encoding));
    out.push_str(&format!("- Encoding Risk: {:?}\n", report.encoding_risk));
    out.push_str("\n");

    out.push_str("## Project\n");
    out.push_str(&format!("- Primary: {}\n", report.project.primary));
    if !report.project.secondary.is_empty() {
        out.push_str(&format!(
            "- Secondary: {}\n",
            report.project.secondary.join(", ")
        ));
    }
    if let Some(fw) = &report.project.framework {
        out.push_str(&format!("- Framework: {}\n", fw));
    }
    if let Some(pm) = &report.project.package_manager {
        out.push_str(&format!("- Package Manager: {}\n", pm));
    }
    if let Some(d) = &report.project.dialect {
        out.push_str(&format!("- Version Dialect: {}\n", d));
    }
    if let Some(db) = &report.project.database {
        out.push_str(&format!("- Database: {}\n", db));
    }
    if let Some(ms) = &report.project.module_system {
        out.push_str(&format!("- Module System: {}\n", ms));
    }
    out.push_str("\n");

    out.push_str("## Detected Project Commands\n");
    for line in detected_project_command_lines(&report) {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&format!("- Raw Build: {}\n", report.project.build));
    out.push_str(&format!("- Raw Test: {}\n", report.project.test));
    out.push_str("\n");

    out.push_str("## Plugin Capabilities\n");
    for line in workspace_plugin_capability_lines(&report) {
        out.push_str(&line);
        out.push('\n');
    }
    out.push('\n');

    out.push_str("## IDE\n");
    let mut ides = Vec::new();
    if report.ide.vscode {
        ides.push("VS Code");
    }
    if report.ide.idea {
        ides.push("IntelliJ IDEA");
    }
    if report.ide.visual_studio {
        ides.push("Visual Studio");
    }
    if report.ide.xcode {
        ides.push("Xcode");
    }
    if report.ide.cursor {
        ides.push("Cursor");
    }
    if report.ide.neovim {
        ides.push("Neovim");
    }
    if report.ide.eclipse {
        ides.push("Eclipse");
    }
    if report.ide.sublime {
        ides.push("Sublime Text");
    }
    if report.ide.android_studio {
        ides.push("Android Studio");
    }
    if report.ide.pycharm {
        ides.push("PyCharm");
    }
    if report.ide.webstorm {
        ides.push("WebStorm");
    }
    if report.ide.clion {
        ides.push("CLion");
    }
    if report.ide.goland {
        ides.push("GoLand");
    }
    if report.ide.rider {
        ides.push("Rider");
    }
    if report.ide.jupyter {
        ides.push("Jupyter");
    }
    if report.ide.rstudio {
        ides.push("RStudio");
    }
    if report.ide.emacs {
        ides.push("Emacs");
    }
    if report.ide.vim {
        ides.push("Vim");
    }
    if ides.is_empty() {
        ides.push("Unknown");
    }
    out.push_str(&format!("- Detected: {}\n", ides.join(", ")));
    out.push_str("\n");

    out.push_str("## Repository\n");
    out.push_str(&format!("- Git: {}\n", report.repo.git));
    if let Some(b) = &report.repo.git_branch {
        out.push_str(&format!("- Branch: {}\n", b));
    }
    if let Some(d) = report.repo.git_dirty {
        out.push_str(&format!("- Dirty: {}\n", d));
    }
    out.push_str("\n");

    out.push_str("## Constraints\n");
    out.push_str("- Use the detected project type, framework, and version dialect for all code generation.\n");
    out.push_str("- Use the detected database type for SQL queries.\n");
    out.push_str("- Use the detected module system (ESM/CJS) for import/export syntax.\n");
    out.push_str("- Use the detected build/test commands through `tokenslim run`.\n");
    out.push_str("- Match the detected IDE for configuration files.\n");
    out.push_str("\n");

    out.push_str("## TokenSlim Command Policy\n");
    out.push_str("- Run build, test, package-manager, VCS, and infrastructure commands through `tokenslim run <command>`.\n");
    out.push_str("- Only use commands detected for this project; do not inject unrelated toolchains just because they exist on the OS.\n");
    out.push_str("- Use raw commands only when explicitly debugging TokenSlim itself or when the wrapper would change the experiment.\n");
    out.push_str("\n");

    out.push_str("## ⚠️ AI SYSTEM DIRECTIVE (CRITICAL)\n");
    out.push_str("The environment (OS, Shell, Encoding, Tool Versions) varies per developer.\n");
    out.push_str("DO NOT assume the environment based on previous conversations.\n");
    out.push_str(
        "Before generating any terminal commands or debugging local issues, YOU MUST run:\n",
    );
    out.push_str("`tokenslim workspace --format llm`\n");
    out.push_str("to get the current developer's real-time local environment.\n");
    out.push_str("\n");

    out.push_str("## VCS Plugin Config Guidance\n");
    out.push_str("- Supported VCS in this repo policy: git/svn/hg/p4/cvs/bzr/fossil/darcs.\n");
    out.push_str("- Preferred shared config: `config/vcs_plugin.json` (project-level).\n");
    out.push_str(
        "- Optional local override: `.tokenslim/vcs_plugin.json` or env `TOKENSLIM_VCS_CONFIG`.\n",
    );
    out.push_str("- Generate a starter config from logs:\n");
    out.push_str("  `python scripts/generate_vcs_config.py --input <log1> --input <log2>`\n");
    out.push_str("- If you need strict policy replacement, use `replace_command_whitelists` / `replace_signatures` in config.\n");
    out.push_str("\n");

    out.push_str("## Actions\n");
    for (i, a) in report.actions.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, translate_action(a)));
    }

    Ok(out)
}

fn wrap_workspace_command_for_tokenslim_run(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        return trimmed.to_string();
    }
    if trimmed.to_ascii_lowercase().starts_with("tokenslim run ") {
        return trimmed.to_string();
    }
    format!("tokenslim run {trimmed}")
}

fn push_unique_command(
    lines: &mut Vec<String>,
    seen_commands: &mut BTreeSet<String>,
    label: &str,
    command: String,
) {
    if command.trim().is_empty() || command.trim().eq_ignore_ascii_case("unknown") {
        return;
    }
    if !seen_commands.insert(command.clone()) {
        return;
    }
    let line = format!("- {label}: `{command}`");
    if !lines.iter().any(|existing| existing == &line) {
        lines.push(line);
    }
}

fn workspace_scope_candidates(report: &WorkspaceDoctorReport) -> Vec<String> {
    let mut keys = Vec::new();
    let primary = report.project.primary.to_ascii_lowercase();
    let package_manager = report
        .project
        .package_manager
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();

    if !primary.is_empty() && primary != "unknown" {
        keys.push(primary.clone());
    }

    if !primary.is_empty() && !package_manager.is_empty() {
        keys.push(format!("{primary}:{package_manager}"));
    }

    if let Some(fw) = report.project.framework.as_deref() {
        let fw = fw.to_ascii_lowercase();
        if !fw.is_empty() && fw != "n/a" && fw != "unknown" {
            keys.push(fw);
        }
    }
    if let Some(dialect) = report.project.dialect.as_deref() {
        let dialect = dialect.to_ascii_lowercase();
        if !dialect.is_empty() && dialect != "n/a" && dialect != "unknown" {
            keys.push(dialect);
        }
    }

    let has = |name: &str| Path::new(name).exists();

    if primary == "java" {
        if has("gradlew") || has("gradlew.bat") {
            keys.push("java:gradlew".to_string());
        } else if package_manager == "gradle" {
            keys.push("java:gradle".to_string());
        } else if package_manager == "maven" {
            keys.push("java:maven".to_string());
        }
    }

    if primary == "node" {
        match package_manager.as_str() {
            "yarn" => keys.push("node:yarn".to_string()),
            "pnpm" => keys.push("node:pnpm".to_string()),
            "bun" => keys.push("node:bun".to_string()),
            _ => {
                keys.push("node:npm".to_string());
                keys.push("node:npx".to_string());
                keys.push("node:node".to_string());
            }
        }
    }

    if primary == "python" {
        match package_manager.as_str() {
            "uv" => keys.push("python:uv".to_string()),
            "poetry" => keys.push("python:poetry".to_string()),
            "conda" => keys.push("python:conda".to_string()),
            _ => keys.push("python:pytest".to_string()),
        }
    }

    if report.project.database.as_deref() == Some("postgresql") {
        keys.push("db:postgres".to_string());
    } else if report.project.database.as_deref() == Some("mongodb") {
        keys.push("db:mongodb".to_string());
    } else if report.project.database.as_deref() == Some("redis") {
        keys.push("db:redis".to_string());
    }

    if has("Dockerfile") || has("docker-compose.yml") || has("docker-compose.yaml") {
        keys.push("docker:compose".to_string());
    }
    if has("Chart.yaml") || has("chart.yaml") {
        keys.push("helm".to_string());
    }
    if glob_exists(Path::new("."), ".tf") || has("main.tf") || has("providers.tf") {
        keys.push("terraform".to_string());
    }
    if has("playbook.yml") || has("playbook.yaml") || has("site.yml") || has("site.yaml") {
        keys.push("ansible".to_string());
    }
    if has("WORKSPACE") || has("BUILD.bazel") || glob_exists(Path::new("."), ".bzl") {
        keys.push("bazel".to_string());
    }
    if has("buf.yaml") || has("buf.gen.yaml") || glob_exists(Path::new("."), ".proto") {
        keys.push("proto".to_string());
    }

    if primary == "cpp" || primary == "c++" || primary == "c" {
        if has("CMakeLists.txt") {
            keys.push("cpp:cmake".to_string());
        }
        if has("build.ninja") {
            keys.push("cpp:ninja".to_string());
        }
        if has("Makefile") {
            keys.push("cpp:make".to_string());
        }
    }

    if report.repo.git {
        keys.push("vcs:git".to_string());
    } else if report.repo.svn {
        keys.push("vcs:svn".to_string());
    } else if report.repo.hg {
        keys.push("vcs:hg".to_string());
    } else if report.repo.p4 {
        keys.push("vcs:p4".to_string());
    } else if report.repo.cvs {
        keys.push("vcs:cvs".to_string());
    } else if report.repo.bzr {
        keys.push("vcs:bzr".to_string());
    } else if report.repo.fossil {
        keys.push("vcs:fossil".to_string());
    } else if report.repo.darcs {
        keys.push("vcs:darcs".to_string());
    }

    keys.sort();
    keys.dedup();
    keys
}

fn configured_workspace_command_pairs(report: &WorkspaceDoctorReport) -> Vec<(String, String)> {
    let config_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("plugins");
    let capabilities = plugin_config_loader::load_workspace_command_capabilities(Some(&config_dir));
    let scope_keys = workspace_scope_candidates(report);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();

    for cap in capabilities {
        for scope_key in &scope_keys {
            let Some(commands) = cap.workspace_commands.get(scope_key) else {
                continue;
            };
            for command in commands {
                let wrapped = wrap_workspace_command_for_tokenslim_run(command);
                let key = format!("{}|{}", command, wrapped);
                if seen.insert(key) {
                    out.push((command.clone(), wrapped));
                }
            }
        }
    }

    out
}

fn detected_project_command_lines(report: &WorkspaceDoctorReport) -> Vec<String> {
    let mut lines = Vec::new();
    let mut seen_commands = BTreeSet::new();

    push_unique_command(
        &mut lines,
        &mut seen_commands,
        "Build",
        wrap_workspace_command_for_tokenslim_run(&report.project.build),
    );
    push_unique_command(
        &mut lines,
        &mut seen_commands,
        "Test",
        wrap_workspace_command_for_tokenslim_run(&report.project.test),
    );

    for (_, wrapped) in configured_workspace_command_pairs(report) {
        push_unique_command(&mut lines, &mut seen_commands, "Command", wrapped);
    }

    lines
}

fn wrap_text(text: &str, indent: usize, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_len = 0;
    let allowed_width = if max_width > indent {
        max_width - indent
    } else {
        20
    };

    for word in text.split(' ') {
        let word_len = word.chars().count();
        if current_len == 0 {
            current_line.push_str(word);
            current_len += word_len;
        } else if current_len + 1 + word_len > allowed_width {
            lines.push(current_line.clone());
            current_line = word.to_string();
            current_len = word_len;
        } else {
            current_line.push(' ');
            current_line.push_str(word);
            current_len += 1 + word_len;
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}

fn workspace_plugin_capability_lines(report: &WorkspaceDoctorReport) -> Vec<String> {
    use crate::utils::i18n::t;
    let mut lines = Vec::new();

    let mut run_routes = Vec::new();
    let mut filters = Vec::new();
    let mut others = Vec::new();

    for cap in &report.plugins {
        if cap.category == "run_route" {
            run_routes.push(cap);
        } else if cap.category == "filter" {
            filters.push(cap);
        } else {
            others.push(cap);
        }
    }

    let indent_spaces = " ".repeat(25);

    if !run_routes.is_empty() {
        lines.push(format!("  {}", t("plugin_category_run_route")));
        for cap in run_routes {
            if !cap.skills.is_empty() {
                let skills_text = format!("{}: {}", t("plugin_skills"), cap.skills.join(", "));
                let wrapped = wrap_text(&skills_text, 25, 100);
                for (i, line) in wrapped.iter().enumerate() {
                    if i == 0 {
                        lines.push(format!("    - {:<16} : {}", cap.name, line));
                    } else {
                        lines.push(format!("{}{}", indent_spaces, line));
                    }
                }
            } else {
                lines.push(format!("    - {:<16} : -", cap.name));
            }
        }
        lines.push("".to_string());
    }

    if !filters.is_empty() {
        lines.push(format!("  {}", t("plugin_category_filter")));
        for cap in filters {
            let desc_key = format!("plugin_desc_{}", cap.name);
            let desc =
                crate::utils::i18n::t_dynamic(&desc_key).unwrap_or(if cap.description.is_empty() {
                    "-"
                } else {
                    &cap.description
                });
            let wrapped = wrap_text(&desc, 25, 100);
            for (i, line) in wrapped.iter().enumerate() {
                if i == 0 {
                    lines.push(format!("    - {:<16} : {}", cap.name, line));
                } else {
                    lines.push(format!("{}{}", indent_spaces, line));
                }
            }
        }
        lines.push("".to_string());
    }

    if !others.is_empty() {
        lines.push(format!("  {}", t("plugin_category_other")));
        for cap in others {
            let desc_key = format!("plugin_desc_{}", cap.name);
            let desc =
                crate::utils::i18n::t_dynamic(&desc_key).unwrap_or(if cap.description.is_empty() {
                    "-"
                } else {
                    &cap.description
                });
            let wrapped = wrap_text(&desc, 25, 100);
            for (i, line) in wrapped.iter().enumerate() {
                if i == 0 {
                    lines.push(format!("    - {:<16} : {}", cap.name, line));
                } else {
                    lines.push(format!("{}{}", indent_spaces, line));
                }
            }
        }
        lines.push("".to_string());
    }

    if lines.is_empty() {
        lines.push(format!("  {}", t("plugin_no_plugins")));
    }

    lines
}

#[derive(Debug, Clone)]
enum InjectAction {
    Created,
    Appended,
    ReplacedBlock,
    Unchanged,
    ReadFailed(String),
    WriteFailed(String),
}

#[derive(Debug, Clone)]
struct InjectAuditEntry {
    file: String,
    action: InjectAction,
    changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawAiCommandFinding {
    file: String,
    line: usize,
    command: String,
    replacement: String,
}

fn action_label(action: &InjectAction) -> &'static str {
    match action {
        InjectAction::Created => "created",
        InjectAction::Appended => "appended",
        InjectAction::ReplacedBlock => "replaced_block",
        InjectAction::Unchanged => "unchanged",
        InjectAction::ReadFailed(_) => "read_failed",
        InjectAction::WriteFailed(_) => "write_failed",
    }
}

fn detected_ide_names(ide: &IdeInfo) -> Vec<&'static str> {
    let mut names = Vec::new();
    if ide.vscode {
        names.push("vscode");
    }
    if ide.idea {
        names.push("idea");
    }
    if ide.visual_studio {
        names.push("visual_studio");
    }
    if ide.xcode {
        names.push("xcode");
    }
    if ide.cursor {
        names.push("cursor");
    }
    if ide.neovim {
        names.push("neovim");
    }
    if ide.eclipse {
        names.push("eclipse");
    }
    if ide.sublime {
        names.push("sublime");
    }
    if ide.android_studio {
        names.push("android_studio");
    }
    if ide.pycharm {
        names.push("pycharm");
    }
    if ide.webstorm {
        names.push("webstorm");
    }
    if ide.clion {
        names.push("clion");
    }
    if ide.goland {
        names.push("goland");
    }
    if ide.rider {
        names.push("rider");
    }
    if ide.jupyter {
        names.push("jupyter");
    }
    if ide.rstudio {
        names.push("rstudio");
    }
    if ide.emacs {
        names.push("emacs");
    }
    if ide.vim {
        names.push("vim");
    }
    if names.is_empty() {
        names.push("unknown");
    }
    names
}

fn ai_instruction_targets() -> &'static [&'static str] {
    &[
        "AGENTS.md",
        "CLAUDE.md",
        "CODEX.md",
        ".github/copilot-instructions.md",
        ".cursor/rules/tokenslim.mdc",
        ".kiro/steering/tokenslim.md",
        ".cursorrules",
        ".windsurfrules",
        ".clinerules",
        ".roorules",
        "AGENT.md",
        ".junie/guidelines.md",
    ]
}

fn detected_project_command_pairs(report: &WorkspaceDoctorReport) -> Vec<(String, String)> {
    detected_project_command_lines(report)
        .into_iter()
        .filter_map(|line| {
            let wrapped = line.split('`').nth(1)?.trim().to_string();
            let raw = wrapped
                .strip_prefix("tokenslim run ")
                .unwrap_or(&wrapped)
                .trim()
                .to_string();
            if raw.is_empty() || raw == wrapped {
                None
            } else {
                Some((raw, wrapped))
            }
        })
        .collect()
}

fn normalize_ai_command_line(line: &str) -> String {
    let trimmed = line.trim();
    let trimmed = trimmed
        .trim_start_matches(|c: char| {
            matches!(
                c,
                '-' | '*' | '>' | '#' | '`' | '\'' | '"' | ':' | ';' | ' '
            ) || c.is_ascii_digit()
                || c == '.'
        })
        .trim();
    trimmed
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn line_starts_with_raw_command(line: &str, raw_command: &str) -> bool {
    let normalized = normalize_ai_command_line(line);
    if normalized.contains("tokenslim run ") {
        return false;
    }
    if normalized == raw_command {
        return true;
    }
    normalized
        .strip_prefix(raw_command)
        .map(|tail| {
            tail.starts_with(" &&")
                || tail.starts_with(" ||")
                || tail.starts_with(" #")
                || tail.starts_with(" --")
                || tail.starts_with(" -")
        })
        .unwrap_or(false)
}

fn scan_text_for_raw_ai_commands(
    target: &str,
    content: &str,
    command_pairs: &[(String, String)],
) -> Vec<RawAiCommandFinding> {
    let content = remove_existing_context_block(content).unwrap_or_else(|| content.to_string());
    let mut findings = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        for (raw, wrapped) in command_pairs {
            if line_starts_with_raw_command(line, raw) {
                findings.push(RawAiCommandFinding {
                    file: target.to_string(),
                    line: line_idx + 1,
                    command: raw.clone(),
                    replacement: wrapped.clone(),
                });
                break;
            }
        }
    }
    findings
}

fn scan_ai_instruction_raw_commands(
    cwd: &Path,
    report: &WorkspaceDoctorReport,
) -> Vec<RawAiCommandFinding> {
    let command_pairs = detected_project_command_pairs(report);
    if command_pairs.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for target in ai_instruction_targets() {
        let path = cwd.join(target);
        if !path.exists() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            findings.extend(scan_text_for_raw_ai_commands(
                target,
                &content,
                &command_pairs,
            ));
        }
    }
    findings
}

fn append_raw_ai_command_findings(out: &mut String, report: &WorkspaceDoctorReport, cwd: &Path) {
    let findings = scan_ai_instruction_raw_commands(cwd, report);
    if findings.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str(t("doctor_workspace_section_ai_command_hygiene"));
    out.push('\n');
    out.push_str(&format!(
        "  {}\n",
        t("doctor_workspace_ai_command_hygiene_line1")
    ));
    out.push_str(&format!(
        "  {}\n",
        t("doctor_workspace_ai_command_hygiene_line2")
    ));
    for finding in findings.iter().take(12) {
        out.push_str(&format!(
            "  - {}:{} `{}` -> `{}`\n",
            finding.file, finding.line, finding.command, finding.replacement
        ));
    }
    if findings.len() > 12 {
        out.push_str(&format!("  - ... {} more\n", findings.len() - 12));
    }
}

fn render_inject_audit_report(
    workspace_report: &WorkspaceDoctorReport,
    entries: &[InjectAuditEntry],
    dry_run: bool,
) -> String {
    let mut out = String::new();
    out.push_str("# Context Injection Audit\n\n");
    out.push_str(&format!(
        "- mode: `{}`\n",
        if dry_run { "dry-run" } else { "apply" }
    ));
    out.push_str(&format!("- os: `{}`\n", workspace_report.os));
    out.push_str(&format!(
        "- project_primary: `{}`\n",
        workspace_report.project.primary
    ));
    out.push_str(&format!(
        "- framework: `{}`\n",
        workspace_report
            .project
            .framework
            .as_deref()
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- package_manager: `{}`\n",
        workspace_report
            .project
            .package_manager
            .as_deref()
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- repo: git=`{}` branch=`{}` dirty=`{}`\n",
        workspace_report.repo.git,
        workspace_report.repo.git_branch.as_deref().unwrap_or("n/a"),
        workspace_report
            .repo
            .git_dirty
            .map(|d| d.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    ));
    out.push_str(&format!(
        "- ide: `{}`\n\n",
        detected_ide_names(&workspace_report.ide).join(", ")
    ));

    let max_file = entries
        .iter()
        .map(|e| e.file.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let max_action = entries
        .iter()
        .map(|e| action_label(&e.action).len())
        .max()
        .unwrap_or(6)
        .max(6);
    let max_note = entries
        .iter()
        .map(|e| match &e.action {
            InjectAction::ReadFailed(msg) | InjectAction::WriteFailed(msg) => msg.len(),
            _ => 0,
        })
        .max()
        .unwrap_or(4)
        .max(4);

    out.push_str(&format!(
        "| {:<w_f$} | {:<w_a$} | {:<7} | {:<w_n$} |\n",
        "file",
        "action",
        "changed",
        "note",
        w_f = max_file,
        w_a = max_action,
        w_n = max_note
    ));
    out.push_str(&format!(
        "| {:-<w_f$} | {:-<w_a$} | {:-<7} | {:-<w_n$} |\n",
        "",
        "",
        "",
        "",
        w_f = max_file,
        w_a = max_action,
        w_n = max_note
    ));

    for entry in entries {
        let note = match &entry.action {
            InjectAction::ReadFailed(msg) | InjectAction::WriteFailed(msg) => msg.as_str(),
            _ => "",
        };
        out.push_str(&format!(
            "| {:<w_f$} | {:<w_a$} | {:<7} | {:<w_n$} |\n",
            entry.file,
            action_label(&entry.action),
            entry.changed,
            note,
            w_f = max_file,
            w_a = max_action,
            w_n = max_note
        ));
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let raw_findings = scan_ai_instruction_raw_commands(&cwd, workspace_report);
    if !raw_findings.is_empty() {
        out.push_str("\n## AI Command Hygiene\n\n");
        out.push_str(&format!(
            "  {}\n",
            t("doctor_workspace_ai_command_hygiene_line1")
        ));
        out.push_str(&format!(
            "  {}\n\n",
            t("doctor_workspace_ai_command_hygiene_line2")
        ));
        let max_f = raw_findings
            .iter()
            .map(|f| f.file.len())
            .max()
            .unwrap_or(4)
            .max(4);
        let max_c = raw_findings
            .iter()
            .map(|f| f.command.len())
            .max()
            .unwrap_or(11)
            .max(11);
        let max_r = raw_findings
            .iter()
            .map(|f| f.replacement.len())
            .max()
            .unwrap_or(17)
            .max(17);

        out.push_str(&format!(
            "| {:<w_f$} | {:<5} | {:<w_c$} | {:<w_r$} |\n",
            "file",
            "line",
            "raw command",
            "suggested command",
            w_f = max_f,
            w_c = max_c,
            w_r = max_r
        ));
        out.push_str(&format!(
            "| {:-<w_f$} | {:-<5} | {:-<w_c$} | {:-<w_r$} |\n",
            "",
            "",
            "",
            "",
            w_f = max_f,
            w_c = max_c,
            w_r = max_r
        ));

        for finding in raw_findings {
            out.push_str(&format!(
                "| {:<w_f$} | {:<5} | {:<w_c$} | {:<w_r$} |\n",
                finding.file,
                finding.line,
                finding.command,
                finding.replacement,
                w_f = max_f,
                w_c = max_c,
                w_r = max_r
            ));
        }
    }

    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextBlockPlacement {
    Top,
}

fn remove_existing_context_block(existing: &str) -> Option<String> {
    if let (Some(start), Some(end)) = (
        existing.find("<!-- tokenslim-context-start -->"),
        existing.find("<!-- tokenslim-context-end -->"),
    ) {
        if end > start {
            let end_idx = end + "<!-- tokenslim-context-end -->".len();
            let mut without_block = existing.to_string();
            without_block.replace_range(start..end_idx, "");
            return Some(without_block.trim_start_matches(['\r', '\n']).to_string());
        }
    }
    None
}

fn insert_context_block(
    existing_without_block: &str,
    block: &str,
    placement: ContextBlockPlacement,
) -> String {
    let block = block.trim_end();
    match placement {
        ContextBlockPlacement::Top => {
            let rest = existing_without_block.trim_start_matches(['\r', '\n']);
            if rest.is_empty() {
                format!("{block}\n")
            } else {
                format!("{block}\n\n{rest}")
            }
        }
    }
}

fn patch_or_insert_context_block(
    existing: &str,
    block: &str,
    placement: ContextBlockPlacement,
) -> (String, InjectAction, bool) {
    if let Some(existing_without_block) = remove_existing_context_block(existing) {
        let patched = insert_context_block(&existing_without_block, block, placement);
        let changed = patched != existing;
        let action = if changed {
            InjectAction::ReplacedBlock
        } else {
            InjectAction::Unchanged
        };
        return (patched, action, changed);
    }

    let patched = insert_context_block(existing, block, placement);
    let changed = patched != existing;
    let action = if changed {
        InjectAction::Appended
    } else {
        InjectAction::Unchanged
    };
    (patched, action, changed)
}

fn apply_context_to_target(
    path: &Path,
    target: &str,
    block: &str,
    dry_run: bool,
    create_if_missing: bool,
    placement: ContextBlockPlacement,
) -> InjectAuditEntry {
    if path.exists() {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let (patched, action, changed) =
                    patch_or_insert_context_block(&content, block, placement);
                if changed && !dry_run {
                    if let Err(e) = std::fs::write(path, patched) {
                        return InjectAuditEntry {
                            file: target.to_string(),
                            action: InjectAction::WriteFailed(e.to_string()),
                            changed: true,
                        };
                    }
                }
                InjectAuditEntry {
                    file: target.to_string(),
                    action,
                    changed,
                }
            }
            Err(e) => InjectAuditEntry {
                file: target.to_string(),
                action: InjectAction::ReadFailed(e.to_string()),
                changed: false,
            },
        }
    } else if create_if_missing {
        if !dry_run {
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return InjectAuditEntry {
                        file: target.to_string(),
                        action: InjectAction::WriteFailed(e.to_string()),
                        changed: true,
                    };
                }
            }
            if let Err(e) = std::fs::write(path, block) {
                return InjectAuditEntry {
                    file: target.to_string(),
                    action: InjectAction::WriteFailed(e.to_string()),
                    changed: true,
                };
            }
        }
        InjectAuditEntry {
            file: target.to_string(),
            action: InjectAction::Created,
            changed: true,
        }
    } else {
        InjectAuditEntry {
            file: target.to_string(),
            action: InjectAction::Unchanged,
            changed: false,
        }
    }
}

fn generate_ai_adapter_context_file(_report: &WorkspaceDoctorReport) -> String {
    let mut out = String::new();
    out.push_str("# TokenSlim Project AI Context Pointer (AUTO-GENERATED)\n");
    out.push_str(
        "# DO NOT EDIT THIS BLOCK MANUALLY - run `tokenslim workspace --inject` to update\n\n",
    );
    out.push_str("Full TokenSlim workspace context lives in `.tokenslim-context.md`.\n");
    out.push_str("Read that file before local command generation, environment debugging, or build/test/VCS work.\n\n");
    out.push_str("Command policy:\n");
    out.push_str("- Run `tokenslim workspace --format llm` before diagnosing this project on a new machine/session.\n");
    out.push_str(
        "- Use the `Detected Project Commands` section in `.tokenslim-context.md` as the source of truth.\n",
    );
    out.push_str("- If raw build/test/VCS commands appear elsewhere in this file, execute their `tokenslim run <command>` equivalent from `.tokenslim-context.md`.\n");
    out.push_str("- Keep this pointer small to avoid duplicate context when multiple AI instruction files are read together.\n");
    out
}

fn update_standalone_context_entry(
    standalone_path: &Path,
    raw_content: &str,
    dry_run: bool,
) -> Result<InjectAuditEntry, String> {
    let standalone_changed = match std::fs::read_to_string(standalone_path) {
        Ok(existing) => existing != raw_content,
        Err(_) => true,
    };
    if standalone_changed && !dry_run {
        std::fs::write(standalone_path, raw_content)
            .map_err(|e| format!("{E_DOCTOR_WORKSPACE_WRITE_CONTEXT}:{e}"))?;
    }
    Ok(InjectAuditEntry {
        file: ".tokenslim-context.md".to_string(),
        action: if standalone_changed {
            if standalone_path.exists() {
                InjectAction::ReplacedBlock
            } else {
                InjectAction::Created
            }
        } else {
            InjectAction::Unchanged
        },
        changed: standalone_changed,
    })
}

/// Write the context file to the current directory and inject into AI tool configs.
/// When `dry_run` is true, it only computes changes and writes no files.
pub fn inject_context_file(dry_run: bool) -> Result<String, String> {
    let workspace_report = collect_workspace_report();
    let raw_content = generate_context_file()?;
    let ai_adapter_content = generate_ai_adapter_context_file(&workspace_report);
    let ai_adapter_block = format!(
        "<!-- tokenslim-context-start -->\n{}\n<!-- tokenslim-context-end -->\n",
        ai_adapter_content
    );

    let cwd =
        std::env::current_dir().map_err(|e| format!("{E_DOCTOR_WORKSPACE_CURRENT_DIR}:{e}"))?;

    // Canonical targets always managed by this command.
    let canonical_targets = [
        "AGENTS.md",
        "CLAUDE.md",
        "CODEX.md",
        ".github/copilot-instructions.md",
        ".cursor/rules/tokenslim.mdc",
        ".kiro/steering/tokenslim.md",
    ];
    // Optional ecosystem files: update only if they already exist.
    let optional_targets = [
        ".cursorrules",
        ".windsurfrules",
        ".clinerules",
        ".roorules",
        "AGENT.md",
        ".junie/guidelines.md",
    ];

    let mut entries: Vec<InjectAuditEntry> = Vec::new();

    for target in canonical_targets {
        let path = cwd.join(target);
        entries.push(apply_context_to_target(
            &path,
            target,
            &ai_adapter_block,
            dry_run,
            true,
            ContextBlockPlacement::Top,
        ));
    }

    for target in optional_targets {
        let path = cwd.join(target);
        if !path.exists() {
            continue;
        }
        entries.push(apply_context_to_target(
            &path,
            target,
            &ai_adapter_block,
            dry_run,
            false,
            ContextBlockPlacement::Top,
        ));
    }

    let standalone_path = cwd.join(".tokenslim-context.md");
    entries.push(update_standalone_context_entry(
        &standalone_path,
        &raw_content,
        dry_run,
    )?);

    let audit_md = render_inject_audit_report(&workspace_report, &entries, dry_run);
    let audit_path_rel = "docs/reports/CONTEXT_INJECTION_AUDIT.md";
    let audit_path = cwd.join(audit_path_rel);
    if !dry_run {
        if let Some(parent) = audit_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("{E_DOCTOR_WORKSPACE_CREATE_AUDIT_DIR}:{e}"))?;
        }
        std::fs::write(&audit_path, &audit_md)
            .map_err(|e| format!("{E_DOCTOR_WORKSPACE_WRITE_AUDIT}:{e}"))?;
    }

    let changed_count = entries.iter().filter(|e| e.changed).count();
    let mode = if dry_run { "dry-run" } else { "apply" };
    Ok(format!(
        "Context injection ({mode}) completed. changed_files={changed_count}\nAudit report{}\n\n{}",
        if dry_run {
            format!(" (preview only, not written): {audit_path_rel}")
        } else {
            format!(": {audit_path_rel}")
        },
        audit_md
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::doctor_encoding::{
        CodepageSignal, EncodingDoctorReport, OsSignal, RuntimeSignal,
    };
    use crate::utils::i18n::t;

    fn sample_report() -> WorkspaceDoctorReport {
        WorkspaceDoctorReport {
            plugins: Vec::new(),
            risk: WorkspaceRiskLevel::Warn,
            encoding_risk: WorkspaceRiskLevel::Warn,
            os: "Windows 11".to_string(),
            shell: "powershell".to_string(),
            encoding: "936".to_string(),
            project: ProjectInfo {
                primary: "rust".to_string(),
                secondary: vec!["python".to_string()],
                framework: Some("axum".to_string()),
                package_manager: Some("cargo".to_string()),
                build: "cargo build".to_string(),
                test: "cargo test".to_string(),
                dialect: Some("rust-2021".to_string()),
                database: Some("sqlite".to_string()),
                module_system: Some("esm".to_string()),
            },
            tools: ToolVersions {
                rust: Some("1.89".to_string()),
                node: None,
                python: Some("3.12".to_string()),
                java: None,
                gcc: None,
                clang: None,
                deno: None,
                msvc: None,
                ninja: None,
                bazel: None,
                make: None,
                cmake: None,
                meson: None,
                julia: None,
                dotnet: None,
                go: None,
                ruby: None,
                php: None,
                swift: None,
                erlang: None,
                fortran: None,
                r_lang: None,
                perl: None,
                lua: None,
                elixir: None,
                haskell: None,
                dart: None,
                scala: None,
                zig: None,
                groovy: None,
                cobol: None,
            },
            ide: IdeInfo {
                vscode: true,
                idea: false,
                visual_studio: true,
                xcode: false,
                cursor: true,
                neovim: false,
                eclipse: false,
                sublime: false,
                android_studio: false,
                pycharm: false,
                webstorm: false,
                clion: false,
                goland: false,
                rider: false,
                jupyter: false,
                rstudio: false,
                emacs: false,
                vim: false,
            },
            repo: RepoInfo {
                git: true,
                git_branch: Some("feature/i18n".to_string()),
                git_dirty: Some(true),
                svn: false,
                hg: false,
                p4: false,
                cvs: false,
                bzr: false,
                fossil: false,
                darcs: false,
            },
            actions: vec!["encoding-review".to_string(), "toolchain-pin".to_string()],
        }
    }

    #[test]
    fn render_workspace_text_uses_i18n_labels() {
        let report = sample_report();
        let text = render_workspace_text(&report);

        assert!(text.contains(t("doctor_workspace_report_title")));
        assert!(text.contains(t("doctor_workspace_section_project")));
        assert!(text.contains(t("doctor_workspace_section_environment")));
        assert!(text.contains(t("doctor_workspace_section_tools")));
        assert!(text.contains(t("doctor_workspace_section_actions")));
        assert!(text.contains(t("doctor_workspace_section_ai_context")));
        assert!(text.contains("tokenslim --dry-run workspace --inject"));
        assert!(text.contains("tokenslim workspace --inject"));
        assert!(text.contains(t("doctor_workspace_field_primary")));
        assert!(text.contains("feature/i18n"));
        assert!(text.contains(crate::utils::i18n::t("doctor_action_encoding_review")));
    }

    #[test]
    fn generate_context_file_uses_new_workspace_command() {
        let context = generate_context_file().expect("context generation should succeed");
        assert!(context.contains("`tokenslim workspace --format llm`"));
        assert!(!context.contains("`tokenslim doctor workspace --format llm`"));
        assert!(context.contains("`tokenslim workspace --inject`"));
        assert!(context.contains("## TokenSlim Command Policy"));
        assert!(context.contains("## Detected Project Commands"));
        assert!(context.contains("## Plugin Capabilities"));
        assert!(context.contains("- Use the detected build/test commands through `tokenslim run`."));
        assert!(context.contains("- Raw Build:"));
        assert!(context.contains("- Raw Test:"));
        assert!(!context.contains("tokenslim run npm test`, `tokenslim run mvn test"));
        assert!(context.contains("do not inject unrelated toolchains"));
    }

    #[test]
    fn generate_ai_adapter_context_file_is_lightweight_and_points_to_full_context() {
        let report = sample_report();
        let adapter = generate_ai_adapter_context_file(&report);

        assert!(adapter.contains(".tokenslim-context.md"));
        assert!(adapter.contains("tokenslim workspace --format llm"));
        assert!(adapter.contains("Detected Project Commands"));
        assert!(adapter.contains("Keep this pointer small"));
        assert!(!adapter.contains("tokenslim run cargo build"));
        assert!(!adapter.contains("tokenslim run cargo test"));
        assert!(!adapter.contains("## Detected TokenSlim Commands"));
        assert!(!adapter.contains("## Environment"));
        assert!(!adapter.contains("## Tools"));
        assert!(!adapter.contains("Rust: 1.89"));
    }

    #[test]
    fn patch_or_insert_context_block_moves_managed_block_to_top() {
        let existing = "# 开发命令\n\n```bash\ncargo test\n```\n\n<!-- tokenslim-context-start -->\nold\n<!-- tokenslim-context-end -->\n";
        let block =
            "<!-- tokenslim-context-start -->\nnew policy\n<!-- tokenslim-context-end -->\n";

        let (patched, action, changed) =
            patch_or_insert_context_block(existing, block, ContextBlockPlacement::Top);

        assert!(changed);
        assert!(matches!(action, InjectAction::ReplacedBlock));
        assert!(patched.starts_with("<!-- tokenslim-context-start -->\nnew policy"));
        assert!(patched.contains("# 开发命令"));
        assert!(patched.contains("cargo test"));
        assert_eq!(patched.matches("tokenslim-context-start").count(), 1);
    }

    #[test]
    fn scan_text_for_raw_ai_commands_reports_unwrapped_project_commands() {
        let report = sample_report();
        let command_pairs = detected_project_command_pairs(&report);
        let content = r#"# 开发命令

```bash
cargo check
tokenslim run cargo test
cargo run
cargo bench
git status --short
```
"#;

        let findings = scan_text_for_raw_ai_commands("CLAUDE.md", content, &command_pairs);

        assert_eq!(findings.len(), 4);
        assert!(findings.iter().any(|finding| {
            finding.command == "cargo check" && finding.replacement == "tokenslim run cargo check"
        }));
        assert!(findings.iter().any(|finding| {
            finding.command == "cargo run" && finding.replacement == "tokenslim run cargo run"
        }));
        assert!(findings.iter().any(|finding| {
            finding.command == "cargo bench" && finding.replacement == "tokenslim run cargo bench"
        }));
        assert!(findings.iter().any(|finding| {
            finding.command == "git status" && finding.replacement == "tokenslim run git status"
        }));
        assert!(!findings
            .iter()
            .any(|finding| finding.command == "cargo test"));
    }

    #[test]
    fn detected_project_commands_use_only_current_project_toolchain() {
        let report = sample_report();
        let lines = detected_project_command_lines(&report).join("\n");

        assert!(lines.contains("tokenslim run cargo build"));
        assert!(lines.contains("tokenslim run cargo test"));
        assert!(lines.contains("tokenslim run cargo check"));
        assert!(lines.contains("tokenslim run cargo run"));
        assert!(lines.contains("tokenslim run cargo bench"));
        assert!(lines.contains("tokenslim run git status"));
        assert!(!lines.contains("tokenslim run mvn"));
        assert!(!lines.contains("tokenslim run npm"));
        assert!(!lines.contains("tokenslim run gradle"));
    }

    #[test]
    fn detected_project_commands_select_maven_for_maven_project() {
        let mut report = sample_report();
        report.project.primary = "java".to_string();
        report.project.package_manager = Some("maven".to_string());
        report.project.build = "mvn package".to_string();
        report.project.test = "mvn test".to_string();

        let lines = detected_project_command_lines(&report).join("\n");

        assert!(lines.contains("tokenslim run mvn test"));
        assert!(lines.contains("tokenslim run mvn package"));
        assert!(lines.contains("tokenslim run git status"));
        assert!(!lines.contains("tokenslim run cargo"));
        assert!(!lines.contains("tokenslim run gradle"));
        assert!(!lines.contains("tokenslim run npm"));
    }

    #[test]
    fn decode_tool_version_output_repairs_gbk_msvc_text() {
        let (bytes, _, _) = encoding_rs::GBK.encode("C/C++ 编译器选项\n");
        let decoded = decode_tool_version_output(bytes.as_ref());
        assert_eq!(decoded, "C/C++ 编译器选项");
    }

    #[test]
    fn build_workspace_actions_collects_expected_signals() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-workspace-actions-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        std::fs::write(temp_dir.join("docker-compose.yml"), "services: {}")
            .expect("write compose file");
        std::fs::write(temp_dir.join("schema.prisma"), "model A { id Int @id }")
            .expect("write prisma file");

        let project = ProjectInfo {
            primary: "rust".to_string(),
            secondary: vec![],
            framework: None,
            package_manager: None,
            build: "cargo build".to_string(),
            test: "cargo test".to_string(),
            dialect: None,
            database: None,
            module_system: None,
        };
        let tools = ToolVersions {
            rust: None,
            node: None,
            python: None,
            java: None,
            gcc: None,
            clang: None,
            deno: None,
            msvc: None,
            ninja: None,
            bazel: None,
            make: None,
            cmake: None,
            meson: None,
            julia: None,
            dotnet: None,
            go: None,
            ruby: None,
            php: None,
            swift: None,
            erlang: None,
            fortran: None,
            r_lang: None,
            perl: None,
            lua: None,
            elixir: None,
            haskell: None,
            dart: None,
            scala: None,
            zig: None,
            groovy: None,
            cobol: None,
        };

        let actions =
            build_workspace_actions(&temp_dir, &project, &tools, &EncodingRiskLevel::Warn);
        assert!(actions.contains(&"install-rust-toolchain".to_string()));
        assert!(actions.contains(&"encoding-review".to_string()));
        assert!(actions.contains(&"docker-compose-present".to_string()));
        assert!(actions.contains(&"prisma-schema-detected".to_string()));
        assert!(!actions.contains(&"none".to_string()));

        let _ = std::fs::remove_file(temp_dir.join("docker-compose.yml"));
        let _ = std::fs::remove_file(temp_dir.join("schema.prisma"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_workspace_encoding_prefers_utf8_flag() {
        let report = EncodingDoctorReport {
            risk: EncodingRiskLevel::Ok,
            os: OsSignal {
                name: "Windows".to_string(),
                version: "11".to_string(),
                locale: Some("zh-CN".to_string()),
            },
            shell: None,
            codepage: Some(CodepageSignal {
                value: Some("65001".to_string()),
                is_utf8: Some(true),
            }),
            powershell: RuntimeSignal {
                detected: true,
                version: Some("7.5".to_string()),
                note: None,
            },
            python: RuntimeSignal {
                detected: true,
                version: Some("3.12 utf-8".to_string()),
                note: None,
            },
            node: RuntimeSignal {
                detected: true,
                version: Some("v20".to_string()),
                note: None,
            },
            jdk: RuntimeSignal {
                detected: true,
                version: Some("17".to_string()),
                note: None,
            },
            supported_decoders: vec![],
            recommended_expansions: vec![],
            repair_strategy_profile: vec![],
            repair_confidence_profile: vec![],
            recommendations: vec![],
        };
        assert_eq!(resolve_workspace_encoding(&report), "utf8");
    }

    #[test]
    fn collect_project_languages_detects_multi_stack_markers() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-project-langs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        std::fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .expect("write cargo file");
        std::fs::write(temp_dir.join("package.json"), "{\"name\":\"demo\"}")
            .expect("write package file");
        std::fs::write(temp_dir.join("pyproject.toml"), "[project]\nname='demo'\n")
            .expect("write pyproject file");

        let langs = collect_project_languages(&temp_dir);
        assert!(langs.contains(&"rust".to_string()));
        assert!(langs.contains(&"node".to_string()));
        assert!(langs.contains(&"python".to_string()));

        let _ = std::fs::remove_file(temp_dir.join("Cargo.toml"));
        let _ = std::fs::remove_file(temp_dir.join("package.json"));
        let _ = std::fs::remove_file(temp_dir.join("pyproject.toml"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn apply_context_to_target_dry_run_marks_created_without_writing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-context-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let file = temp_dir.join("CLAUDE.md");

        let entry = apply_context_to_target(
            &file,
            "CLAUDE.md",
            "<!-- tokenslim-context-start -->\nctx\n<!-- tokenslim-context-end -->\n",
            true,
            true,
            ContextBlockPlacement::Top,
        );
        assert!(matches!(entry.action, InjectAction::Created));
        assert!(entry.changed);
        assert!(!file.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn apply_context_to_target_creates_nested_parent_directories() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-context-nested-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let file = temp_dir.join(".cursor/rules/tokenslim.mdc");

        let entry = apply_context_to_target(
            &file,
            ".cursor/rules/tokenslim.mdc",
            "<!-- tokenslim-context-start -->\nctx\n<!-- tokenslim-context-end -->\n",
            false,
            true,
            ContextBlockPlacement::Top,
        );
        assert!(matches!(entry.action, InjectAction::Created));
        assert!(entry.changed);
        assert!(file.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

pub fn run_plugins_mode() {
    let report = collect_workspace_report();
    let header = crate::utils::i18n::t("cli_help_plugin_capabilities")
        .replace("## ", "")
        .replace(":", "");
    println!(
        "{}
====================================
",
        header
    );
    for line in workspace_plugin_capability_lines(&report) {
        println!("{}", line);
    }
    println!();
}
