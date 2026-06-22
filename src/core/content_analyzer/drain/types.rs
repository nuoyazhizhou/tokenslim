//! Drain 算法类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 日志模板簇
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogCluster {
    pub id: usize,
    pub template: Vec<String>, // 词序列，<*> 表示通配符
    pub size: usize,           // 包含的消息数量
}

/// 树节点
#[derive(Debug, Clone)]
pub enum DrainNode {
    Internal(HashMap<String, DrainNode>), // 按照词内容分叉
    Leaf(Vec<usize>),                     // 存储 Cluster ID 列表
}

/// Drain 算法配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrainConfig {
    pub max_depth: usize,    // 树的最大深度（通常为 4-6）
    pub sim_threshold: f32,  // 相似度阈值（通常为 0.3-0.6）
    pub max_children: usize, // 每个内部节点的最大子节点数
    pub tokenize_delimiters: Vec<char>,
}

impl Default for DrainConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            sim_threshold: 0.5,
            max_children: 100,
            tokenize_delimiters: vec![' ', '=', ',', ':', ';', '[', ']', '(', ')'],
        }
    }
}

/// Drain 核心状态
pub struct DrainManager {
    pub config: DrainConfig,
    pub root: HashMap<usize, DrainNode>, // 第一层按消息长度（Token 数量）分类
    pub clusters: Vec<LogCluster>,
    pub next_cluster_id: usize,
}
