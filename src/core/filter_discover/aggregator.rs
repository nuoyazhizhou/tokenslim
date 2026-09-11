// filter_discover/aggregator.rs
// 聚合器 - 按命令分组并估算潜在节省

use super::types::{ClassifiedCommand, CommandClass, CommandGroup, DiscoverResult};
use crate::core::tracking::Tracker;
use std::collections::HashMap;

/// 聚合并估算
///
/// # 参数
/// - `classified`: 分类后的命令列表
/// - `tracker`: Token 追踪器（用于加载历史 savings_pct）
///
/// # 返回
/// - `DiscoverResult`: 聚合结果
#[tracing::instrument(level = "debug", skip_all)]
pub fn aggregate_and_estimate(
    classified: &[ClassifiedCommand],
    tracker: &Tracker,
) -> Result<DiscoverResult, String> {
    // 按分类分组
    let mut already_filtered_map: HashMap<String, Vec<&ClassifiedCommand>> = HashMap::new();
    let mut filterable_map: HashMap<String, Vec<&ClassifiedCommand>> = HashMap::new();
    let mut no_filter_map: HashMap<String, Vec<&ClassifiedCommand>> = HashMap::new();

    for cmd in classified {
        let key = extract_group_key(&cmd.command.command);

        match &cmd.class {
            CommandClass::AlreadyFiltered => {
                already_filtered_map.entry(key).or_default().push(cmd);
            }
            CommandClass::Filterable { filter_name } => {
                // 使用 filter_name 作为分组键
                filterable_map
                    .entry(filter_name.clone())
                    .or_default()
                    .push(cmd);
            }
            CommandClass::NoFilter => {
                no_filter_map.entry(key).or_default().push(cmd);
            }
        }
    }

    // 聚合各组
    let already_filtered = aggregate_groups(&already_filtered_map, tracker)?;
    let filterable = aggregate_groups(&filterable_map, tracker)?;
    let no_filter = aggregate_groups(&no_filter_map, tracker)?;

    // 计算总潜在节省
    let total_potential_savings = filterable
        .iter()
        .filter_map(|g| g.estimated_tokens_saved)
        .sum::<i64>()
        + no_filter
            .iter()
            .filter_map(|g| g.estimated_tokens_saved)
            .sum::<i64>();

    Ok(DiscoverResult {
        already_filtered,
        filterable,
        no_filter,
        total_commands: classified.len(),
        total_potential_savings,
    })
}

/// 聚合命令组
fn aggregate_groups(
    map: &HashMap<String, Vec<&ClassifiedCommand>>,
    tracker: &Tracker,
) -> Result<Vec<CommandGroup>, String> {
    let mut groups = Vec::new();

    for (key, commands) in map {
        let count = commands.len();
        let mut total_input_bytes = 0u64;
        let mut total_output_bytes = 0u64;
        let mut total_input_tokens = 0i64;
        let mut total_output_tokens = 0i64;

        for cmd in commands {
            total_input_bytes += cmd.command.input_bytes.unwrap_or(0);
            total_output_bytes += cmd.command.output_bytes.unwrap_or(0);
            total_input_tokens += cmd.command.input_tokens.unwrap_or(0);
            total_output_tokens += cmd.command.output_tokens.unwrap_or(0);
        }

        // 从历史数据加载 savings_pct
        let estimated_savings_pct = load_historical_savings_pct(tracker, key)?;

        // P2-45：估算诚实降级——当该组没有任何真实 token 元数据（total_output_tokens==0，
        // 如 DeepSeek/Claude session 解析器未补采到 usage）时，`estimated_tokens_saved` 返回
        // `None` 而非 `Some(0)`。旧实现恒算出 `Some((0 * pct) as i64) = Some(0)`，与
        // `total_potential_savings` 叠加后让 discover 核心卖点「估算潜在节省」系统性恒 0，
        // 误导用户以为无收益。诚实降级后：无元数据的组不产出估算值，仅保留命令频次发现；
        // 有真实 token 数据的组（generic 解析路径）仍按历史 pct 或 30% 兜底正常估算。
        let estimated_tokens_saved = if total_output_tokens > 0 {
            if let Some(pct) = estimated_savings_pct {
                Some((total_output_tokens as f64 * pct / 100.0) as i64)
            } else {
                // 如果没有历史数据，使用默认估算值 30%
                Some((total_output_tokens as f64 * 0.30) as i64)
            }
        } else {
            None
        };

        groups.push(CommandGroup {
            key: key.clone(),
            count,
            total_input_bytes,
            total_output_bytes,
            total_input_tokens,
            total_output_tokens,
            estimated_savings_pct,
            estimated_tokens_saved,
        });
    }

    // 按估算节省 Token 数降序排序
    groups.sort_by(|a, b| {
        b.estimated_tokens_saved
            .unwrap_or(0)
            .cmp(&a.estimated_tokens_saved.unwrap_or(0))
    });

    Ok(groups)
}

/// 提取分组键
///
/// 例如:
/// - "git status" -> "git status"
/// - "git log --oneline" -> "git log"
/// - "cargo test --all" -> "cargo test"
fn extract_group_key(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();

    if parts.is_empty() {
        return command.to_string();
    }

    // 提取前两个词作为分组键
    if parts.len() >= 2 {
        format!("{} {}", parts[0], parts[1])
    } else {
        parts[0].to_string()
    }
}

/// 从历史数据加载 savings_pct
fn load_historical_savings_pct(
    tracker: &Tracker,
    filter_name: &str,
) -> Result<Option<f64>, String> {
    // 从 tracking.db 查询该过滤器的平均 savings_pct
    let filter_gains = tracker.get_by_filter()?;

    for gain in filter_gains {
        if gain.filter_name == filter_name {
            return Ok(Some(gain.savings_pct));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::super::types::SessionCommand;
    use super::*;

    /// 验证 `extract_group_key` 的分组键提取规则：单/双词命令分别返回首词与"前两个词"，空串原样返回。
    #[test]
    fn test_extract_group_key() {
        assert_eq!(extract_group_key("git status"), "git status");
        assert_eq!(extract_group_key("git log --oneline"), "git log");
        assert_eq!(extract_group_key("cargo test --all"), "cargo test");
        assert_eq!(extract_group_key("npm"), "npm");
        assert_eq!(extract_group_key(""), "");
    }

    /// 验证 `aggregate_groups` 对单过滤器分组（vcs_git）的聚合：命令数、字节/Token 汇总正确，且无历史数据时按默认 30% 估算节省（2 命令 300 output_token → 90 saved）。
    #[test]
    fn test_aggregate_groups() {
        let commands = vec![
            ClassifiedCommand {
                command: SessionCommand {
                    command: "git status".to_string(),
                    input_bytes: Some(100),
                    output_bytes: Some(1000),
                    input_tokens: Some(10),
                    output_tokens: Some(100),
                    timestamp: None,
                },
                class: CommandClass::Filterable {
                    filter_name: "vcs_git".to_string(),
                },
            },
            ClassifiedCommand {
                command: SessionCommand {
                    command: "git log".to_string(),
                    input_bytes: Some(200),
                    output_bytes: Some(2000),
                    input_tokens: Some(20),
                    output_tokens: Some(200),
                    timestamp: None,
                },
                class: CommandClass::Filterable {
                    filter_name: "vcs_git".to_string(),
                },
            },
        ];

        let mut map: HashMap<String, Vec<&ClassifiedCommand>> = HashMap::new();
        map.insert("vcs_git".to_string(), vec![&commands[0], &commands[1]]);

        // 创建临时 tracker
        let tracker = Tracker::open_in_memory().unwrap();

        let groups = aggregate_groups(&map, &tracker).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "vcs_git");
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[0].total_input_bytes, 300);
        assert_eq!(groups[0].total_output_bytes, 3000);
        assert_eq!(groups[0].total_input_tokens, 30);
        assert_eq!(groups[0].total_output_tokens, 300);
        // 默认估算 30%
        assert_eq!(groups[0].estimated_tokens_saved, Some(90));
    }

    /// 验证 `aggregate_and_estimate` 端到端：混合 Filterable/AlreadyFiltered/NoFilter 三类命令后，total_commands 与 total_potential_savings（仅累加 filterable+no_filter 的 30%）计算正确。
    #[test]
    fn test_aggregate_and_estimate() {
        let classified = vec![
            ClassifiedCommand {
                command: SessionCommand {
                    command: "git status".to_string(),
                    input_bytes: Some(100),
                    output_bytes: Some(1000),
                    input_tokens: Some(10),
                    output_tokens: Some(100),
                    timestamp: None,
                },
                class: CommandClass::Filterable {
                    filter_name: "vcs_git".to_string(),
                },
            },
            ClassifiedCommand {
                command: SessionCommand {
                    command: "tokenslim run cargo test".to_string(),
                    input_bytes: Some(200),
                    output_bytes: Some(2000),
                    input_tokens: Some(20),
                    output_tokens: Some(200),
                    timestamp: None,
                },
                class: CommandClass::AlreadyFiltered,
            },
            ClassifiedCommand {
                command: SessionCommand {
                    command: "unknown-command".to_string(),
                    input_bytes: Some(50),
                    output_bytes: Some(500),
                    input_tokens: Some(5),
                    output_tokens: Some(50),
                    timestamp: None,
                },
                class: CommandClass::NoFilter,
            },
        ];

        let tracker = Tracker::open_in_memory().unwrap();
        let result = aggregate_and_estimate(&classified, &tracker).unwrap();

        assert_eq!(result.total_commands, 3);
        assert_eq!(result.filterable.len(), 1);
        assert_eq!(result.already_filtered.len(), 1);
        assert_eq!(result.no_filter.len(), 1);
        // 100 * 0.3 + 50 * 0.3 = 30 + 15 = 45
        assert_eq!(result.total_potential_savings, 45);
    }

    /// P2-45 负路径：验证 `aggregate_and_estimate` 对无 token 元数据的命令（DeepSeek/Claude
    /// session 解析路径的 output_tokens=None 恒 0 场景）诚实降级——`estimated_tokens_saved`
    /// 返回 `None`（不产出误导性估算），且 `total_potential_savings` 只累加有真实元数据的分组。
    #[test]
    fn test_aggregate_no_token_metadata_honest_degrade() {
        let classified = vec![
            // DeepSeek 路径：硬编码 metrics=None，output_tokens 恒 0。
            ClassifiedCommand {
                command: SessionCommand {
                    command: "git status".to_string(),
                    input_bytes: None,
                    output_bytes: None,
                    input_tokens: None,
                    output_tokens: None,
                    timestamp: None,
                },
                class: CommandClass::Filterable {
                    filter_name: "vcs_git".to_string(),
                },
            },
            // generic 路径：携带真实 output_tokens，估算应正常。
            ClassifiedCommand {
                command: SessionCommand {
                    command: "npm test".to_string(),
                    input_bytes: None,
                    output_bytes: None,
                    input_tokens: None,
                    output_tokens: Some(200),
                    timestamp: None,
                },
                class: CommandClass::Filterable {
                    filter_name: "vcs_git".to_string(),
                },
            },
        ];

        let tracker = Tracker::open_in_memory().unwrap();
        let result = aggregate_and_estimate(&classified, &tracker).unwrap();

        // 两组同 filter_name 会合并为一个分组，合计 output_tokens=200。
        assert_eq!(result.filterable.len(), 1);
        let group = &result.filterable[0];
        assert_eq!(group.total_output_tokens, 200);
        // 有真实元数据：按 30% 兜底估算 200 * 0.3 = 60。
        assert_eq!(group.estimated_tokens_saved, Some(60));
        assert_eq!(result.total_potential_savings, 60);
    }

    /// P2-45 负路径：验证 `aggregate_and_estimate` 对**全部**命令均无 token 元数据时，
    /// `total_potential_savings` 为 0 而非误导性的正数——DiscoverResult 序列化面保持诚实。
    #[test]
    fn test_aggregate_all_missing_metadata_yields_zero_savings() {
        let classified = vec![ClassifiedCommand {
            command: SessionCommand {
                command: "git log".to_string(),
                input_bytes: None,
                output_bytes: None,
                input_tokens: None,
                output_tokens: None,
                timestamp: None,
            },
            class: CommandClass::Filterable {
                filter_name: "vcs_git".to_string(),
            },
        }];

        let tracker = Tracker::open_in_memory().unwrap();
        let result = aggregate_and_estimate(&classified, &tracker).unwrap();

        assert_eq!(result.filterable.len(), 1);
        assert_eq!(result.filterable[0].estimated_tokens_saved, None);
        assert_eq!(result.total_potential_savings, 0);
    }
}
