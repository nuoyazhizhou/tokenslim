use std::path::Path;

/// 判断当前工作目录下是否存在与给定文件名匹配的文件（用于按文件探测构建变体）。
pub fn detect_file(cwd: &Path, pattern: &str) -> bool {
    cwd.join(pattern).exists()
}
