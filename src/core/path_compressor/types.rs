//! 路径压缩器类型定义

use std::collections::HashMap;

/// 路径压缩器
pub struct PathCompressor {
    /// 公共路径前缀映射
    pub(crate) common_prefixes: HashMap<String, String>,
    /// 下一个前缀 ID
    pub(crate) next_prefix_id: usize,
    /// 最小前缀长度（小于此长度的前缀不压缩）
    pub(crate) min_prefix_length: usize,
    /// 最小出现次数（小于此次数的不提取为公共前缀）
    pub(crate) min_occurrences: usize,
}

impl PathCompressor {
    /// 创建一个新的路径压缩器，使用默认配置。
    pub fn new() -> Self {
        Self {
            common_prefixes: HashMap::new(),
            next_prefix_id: 1,
            min_prefix_length: 20, // 默认至少 20 字符才值得压缩
            min_occurrences: 2,    // 默认至少出现 2 次
        }
    }

    /// 从路径列表中分析并提取公共前缀。
    pub fn extract_common_prefixes(&mut self, paths: &[&str]) {
        // 统计所有路径各级前缀的出现次数
        let mut prefix_counts: HashMap<String, usize> = HashMap::new();

        for path in paths {
            // 按路径分隔符切分
            let parts: Vec<&str> = path.split('/').collect();
            let mut current_prefix = String::new();

            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    current_prefix.push('/');
                }
                current_prefix.push_str(part);

                // 只有长度达标的前缀才计入统计
                if current_prefix.len() >= self.min_prefix_length {
                    *prefix_counts.entry(current_prefix.clone()).or_insert(0) += 1;
                }
            }
        }

        // 筛选出在数据集中出现次数达标的所有可能前缀
        let mut selected_prefixes: Vec<_> = prefix_counts
            .into_iter()
            .filter(|(_, count)| *count >= self.min_occurrences)
            .collect();

        // 优先处理长前缀，以获得更好的压缩效果
        selected_prefixes.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let mut used_paths = std::collections::HashSet::new();

        for (prefix, _) in selected_prefixes {
            // 防止重复压缩已经处理过的路径段
            let has_overlap = paths
                .iter()
                .filter(|p| p.starts_with(&prefix))
                .any(|p| used_paths.contains(*p));

            if !has_overlap {
                // 分配压缩标识符 Token
                let token = format!("$P{}", self.next_prefix_id);
                self.next_prefix_id += 1;

                self.common_prefixes.insert(token, prefix.clone());

                // 记录已被覆盖的路径
                for path in paths.iter() {
                    if path.starts_with(&prefix) {
                        used_paths.insert(path.to_string());
                    }
                }
            }
        }
    }

    /// 对单个路径字符串执行压缩（如果其前缀在字典中）。
    pub fn compress_path(&self, path: &str) -> String {
        let mut result = path.to_string();

        // 按长度降序检查，确保匹配最长的前缀条目
        let mut prefixes: Vec<_> = self.common_prefixes.iter().collect();
        prefixes.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        for (token, prefix) in prefixes {
            if result.starts_with(prefix) {
                result = result.replacen(prefix, token, 1);
                break; // 路径通常只有一个根前缀
            }
        }

        result
    }

    /// 获取当前所有已提取的前缀 Token 到原始路径的映射。
    pub fn get_prefix_map(&self) -> &HashMap<String, String> {
        &self.common_prefixes
    }

    /// 清空压缩状态，重置计数器。
    pub fn reset(&mut self) {
        self.common_prefixes.clear();
        self.next_prefix_id = 1;
    }

    /// 设置进行压缩转换的最小字符长度要求。
    pub fn set_min_prefix_length(&mut self, length: usize) {
        self.min_prefix_length = length;
    }

    /// 设置判定为“公共”前缀所需的最小出现频率。
    pub fn set_min_occurrences(&mut self, count: usize) {
        self.min_occurrences = count;
    }

    /// 获取当前的压缩效率统计。
    pub fn get_stats(&self) -> PathCompressorStats {
        let total_prefixes = self.common_prefixes.len();
        let total_chars_saved: usize = self
            .common_prefixes
            .values()
            .map(|p: &String| p.len().saturating_sub(3))
            .sum();

        PathCompressorStats {
            total_prefixes,
            total_chars_saved,
        }
    }
}

impl Default for PathCompressor {
    /// Creates a new PathCompressor with default settings
    fn default() -> Self {
        Self::new()
    }
}

/// 路径压缩器统计信息
pub struct PathCompressorStats {
    pub total_prefixes: usize,
    pub total_chars_saved: usize,
}
