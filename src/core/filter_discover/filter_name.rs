// filter_discover/filter_name.rs
// P2-44 单一 filter 命名权威——discover 读侧（classifier）与 run 写侧
// （resolve_run_filter_name）共用的命名规则模块。
//
// 背景（P2-44 命名体系断裂）：classifier 曾硬编码 13 个家族名
// （`vcs_git`/`rust`/`nodejs`/…），与 tracking 写入侧实际记录的 filter 名
// （`vcs_plugin`/`vitest`/首个子命令参数/程序名兜底）零交集，导致
// discover 的历史 savings_pct 查询（tracker.get_by_filter）永远 miss、
// 恒走 30% 兜底。修复方向：读侧向写侧对齐（写侧命名已落库、不可变更），
// 双方引用本模块的单一推导规则。

use crate::core::plugin_config_loader::{self, RunRouteCapability};

/// 追踪过滤器名的统一推导规则（与写侧 `resolve_run_filter_name` 第 2~4 层一致）。
///
/// 优先级自高到低：
/// 1. VCS 路由命中（`vcs_routed == true`）→ 固定 `"vcs_plugin"`；
/// 2. 首个子命令参数（如 `cargo build` → `"build"`）；
/// 3. 兜底用程序名本身。
///
/// 已知残差（诚实登记，P2-44）：写侧在上述规则之前还有一层 **cwd 相关的
/// npm test 变体解析**（`resolve_npm_test_variant` → `vitest`/`jest`/`mocha`），
/// 读侧（session 历史命令）无 cwd 信息无法复现——对 `npm test` 读侧产出
/// `"test"` 而写侧可能写 `"vitest"`。该残差由 discover 报告的 30% 兜底吸收，
/// 不影响其余命令族的精确命中。
pub fn derive_tracking_filter_name(prog: &str, cmd_args: &[String], vcs_routed: bool) -> String {
    if vcs_routed {
        return "vcs_plugin".to_string();
    }
    if let Some(first) = cmd_args.first() {
        return first.clone();
    }
    prog.to_string()
}

/// 判定命令是否命中 VCS run 路由（`route_group == "vcs"`）。
///
/// 与写侧 `get_vcs_intent` 的 VCS 判定**同源**——均经
/// `plugin_config_loader::resolve_run_route` 按 run routes 配置解析，
/// 不再维护第二份程序名硬编码列表。
pub fn is_vcs_routed(caps: &[RunRouteCapability], prog: &str, cmd_args: &[String]) -> bool {
    let route = plugin_config_loader::resolve_run_route(caps, prog, cmd_args);
    route.route_group.eq_ignore_ascii_case("vcs")
}

/// 加载 run 路由能力（discover 读侧专用；与写侧 `load_run_routes` 同一配置源，
/// 走 `find_config_dir` 的多路径探测，配置缺失时落到内置 generic 兜底）。
///
/// 调用方应在批量分类前调用一次并传引用复用，避免逐命令重复读盘（P3-47 N+1 同族）。
pub fn load_route_caps() -> Vec<RunRouteCapability> {
    plugin_config_loader::load_run_route_capabilities(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约：VCS 路由命中时固定返回 "vcs_plugin"（与写侧第 2 层一致）。
    #[test]
    fn test_derive_vcs_routed_returns_vcs_plugin() {
        assert_eq!(
            derive_tracking_filter_name("git", &["status".to_string()], true),
            "vcs_plugin"
        );
    }

    /// 契约：非 VCS 命令优先取首个子命令参数（`cargo build` → "build"，与写侧第 3 层一致）。
    #[test]
    fn test_derive_prefers_first_subcommand_arg() {
        assert_eq!(
            derive_tracking_filter_name("cargo", &["build".to_string(), "--release".to_string()], false),
            "build"
        );
    }

    /// 契约：无子命令参数时兜底用程序名（与写侧第 4 层一致）。
    #[test]
    fn test_derive_falls_back_to_program_name() {
        assert_eq!(derive_tracking_filter_name("pytest", &[], false), "pytest");
    }

    /// 契约：is_vcs_routed 与写侧 resolve_run_route 同源——`git status` 命中
    /// vcs 路由（config/plugins/vcs_plugin.route.json 存在时），非 VCS 命令不命中。
    #[test]
    fn test_is_vcs_routed_matches_run_route_config() {
        let caps = load_route_caps();
        // 命中与否取决于路由配置；本仓库 config/plugins 含 vcs_plugin.route.json，
        // cargo test 的工作目录为仓库根，故 git 应命中、cargo 不应命中。
        if caps.iter().any(|c| c.route.route_group == "vcs") {
            assert!(is_vcs_routed(&caps, "git", &["status".to_string()]));
            assert!(!is_vcs_routed(&caps, "cargo", &["build".to_string()]));
        }
    }
}
