//! Drain 算法方法实现

use super::types::*;
use std::collections::HashMap;

impl DrainManager {
    /// 创建一个新的 DrainManager 实例。
    ///
    /// # 参数
    /// - `config`: Drain 算法配置（包含深度、相似度阈值等）。
    pub fn new(config: DrainConfig) -> Self {
        DrainManager {
            config,
            root: HashMap::new(),
            clusters: Vec::new(),
            next_cluster_id: 1,
        }
    }

    /// 将一条日志消息添加到 Drain 树中，并返回所属聚类（Cluster）的 ID。
    ///
    /// # 执行流程
    /// 1. 分词（Tokenize）。
    /// 2. 遍历树结构：按长度分支 -> 按前缀分支 -> 匹配/创建叶子节点。
    /// 3. 模板更新：如果匹配到现有聚类，则根据新消息演化（Evolve）模板；否则创建新聚类。
    pub fn add_log_message(&mut self, content: &str) -> usize {
        let tokens = Self::tokenize_static(content, &self.config.tokenize_delimiters);
        if tokens.is_empty() {
            return 0;
        }

        let seq_len = tokens.len();

        // 1. 第一层：按长度
        if !self.root.contains_key(&seq_len) {
            self.root
                .insert(seq_len, DrainNode::Internal(HashMap::new()));
        }

        let mut current_node = self.root.get_mut(&seq_len).unwrap();
        let max_depth = self.config.max_depth;
        let max_children = self.config.max_children;

        // 2. 逐层向下
        for depth in 2..(max_depth + 1) {
            if depth > seq_len {
                break;
            }

            let token = tokens[depth - 1];
            let has_digit = token.chars().any(|c| c.is_ascii_digit());
            let branch_key = if has_digit { "<*>" } else { token }.to_string();

            // 提前取出 children 以避开借用检查冲突
            let is_leaf_layer = depth == max_depth || depth == seq_len;

            if let DrainNode::Internal(children) = current_node {
                if !children.contains_key(&branch_key) {
                    if children.len() >= max_children {
                        children.entry("<*>".to_string()).or_insert_with(|| {
                            if is_leaf_layer {
                                DrainNode::Leaf(Vec::new())
                            } else {
                                DrainNode::Internal(HashMap::new())
                            }
                        });
                        current_node = children.get_mut("<*>").unwrap();
                    } else {
                        children.insert(
                            branch_key.clone(),
                            if is_leaf_layer {
                                DrainNode::Leaf(Vec::new())
                            } else {
                                DrainNode::Internal(HashMap::new())
                            },
                        );
                        current_node = children.get_mut(&branch_key).unwrap();
                    }
                } else {
                    current_node = children.get_mut(&branch_key).unwrap();
                }
            } else {
                break;
            }
        }

        // 3. 叶子节点匹配
        if let DrainNode::Leaf(cluster_ids) = current_node {
            let mut best_cluster_id = None;
            let mut max_sim = -1.0;

            for &cid in cluster_ids.iter() {
                let cluster = &self.clusters[cid - 1];
                let sim = Self::calculate_similarity_static(&tokens, &cluster.template);
                if sim >= self.config.sim_threshold && sim > max_sim {
                    max_sim = sim;
                    best_cluster_id = Some(cid);
                }
            }

            if let Some(cid) = best_cluster_id {
                let cluster = &mut self.clusters[cid - 1];
                Self::evolve_template_static(&mut cluster.template, &tokens);
                cluster.size += 1;
                cid
            } else {
                let new_id = self.next_cluster_id;
                self.next_cluster_id += 1;
                let new_cluster = LogCluster {
                    id: new_id,
                    template: tokens.iter().map(|s| s.to_string()).collect(),
                    size: 1,
                };
                self.clusters.push(new_cluster);
                cluster_ids.push(new_id);
                new_id
            }
        } else {
            0
        }
    }

    /// 静态分词方法。基于给定的分隔符集将日志行拆分为标记。
    fn tokenize_static<'a>(content: &'a str, delimiters: &[char]) -> Vec<&'a str> {
        content
            .split(|c: char| delimiters.contains(&c))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// 计算日志消息标记与聚类模板之间的相似度。基于共同标记的比例。
    fn calculate_similarity_static(tokens: &[&str], template: &[String]) -> f32 {
        if tokens.len() != template.len() {
            return 0.0;
        }
        let mut sim_tokens = 0;
        for (t1, t2) in tokens.iter().zip(template.iter()) {
            if t1 == t2 {
                sim_tokens += 1;
            }
        }
        sim_tokens as f32 / tokens.len() as f32
    }

    /// 演化聚类模板。如果在相同位置发现不同标记，则将该位置标记为占位符 `<*>`。
    fn evolve_template_static(template: &mut Vec<String>, tokens: &[&str]) {
        for (t_part, msg_token) in template.iter_mut().zip(tokens.iter()) {
            if t_part != *msg_token {
                *t_part = "<*>".to_string();
            }
        }
    }

    /// 获取当前所有已识别的日志聚类模板。
    pub fn get_templates(&self) -> Vec<LogCluster> {
        self.clusters.clone()
    }
}
