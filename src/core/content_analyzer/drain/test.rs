//! Drain 核心逻辑单元测试

#[cfg(test)]
mod tests {
    use crate::core::content_analyzer::drain::{DrainConfig, DrainManager};

    #[test]
    fn test_drain_basic_mining() {
        let config = DrainConfig {
            sim_threshold: 0.5,
            max_depth: 4,
            ..DrainConfig::default()
        };
        let mut manager = DrainManager::new(config);

        let logs = vec![
            "User 123 logged in from 192.168.1.1",
            "User 456 logged in from 192.168.1.2",
            "Connection refused from 10.0.0.1",
            "User 789 logged in from 1.1.1.1",
        ];

        for log in logs {
            manager.add_log_message(log);
        }

        let templates = manager.get_templates();
        for t in &templates {
            println!("Template {}: {}", t.id, t.template.join(" "));
        }
        // 应该有两个模板：User <*> logged in from <*> 和 Connection refused from <*>
        assert!(templates.len() >= 2);

        let user_template = templates
            .iter()
            .find(|c| c.template.contains(&"User".to_string()))
            .unwrap();
        assert!(user_template.template.contains(&"<*>".to_string()));
    }
}
