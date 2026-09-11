use std::path::Path;

use super::detector::detect_file;
use super::types::VariantFilter;

/// 解析 npm test 调用的测试框架变体：检测 vitest/jest/mocha 配置文件存在性并返回对应过滤器。
pub fn resolve_npm_test_variant(cwd: &Path, prog: &str, args: &[String]) -> Option<VariantFilter> {
    let prog_lc = prog.to_ascii_lowercase();
    if prog_lc != "npm" && prog_lc != "npm.cmd" {
        return None;
    }
    if args.first().map(|s| s.as_str()) != Some("test") {
        return None;
    }

    if detect_file(cwd, "vitest.config.ts")
        || detect_file(cwd, "vitest.config.js")
        || detect_file(cwd, "vitest.config.mts")
        || detect_file(cwd, "vitest.config.cjs")
    {
        return Some(VariantFilter::Vitest);
    }
    if detect_file(cwd, "jest.config.js")
        || detect_file(cwd, "jest.config.ts")
        || detect_file(cwd, "jest.config.cjs")
        || detect_file(cwd, "jest.config.mjs")
    {
        return Some(VariantFilter::Jest);
    }
    if detect_file(cwd, ".mocharc.js")
        || detect_file(cwd, ".mocharc.json")
        || detect_file(cwd, ".mocharc.yml")
    {
        return Some(VariantFilter::Mocha);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 `resolve_npm_test_variant` 在临时目录写入 vitest 配置后能正确识别 Vitest 变体。
    #[test]
    fn detects_vitest_variant_by_file() {
        let unique = format!(
            "tokenslim_variant_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("vitest.config.ts"), "export default {}").expect("write config");

        let args = vec!["test".to_string()];
        let variant = resolve_npm_test_variant(&dir, "npm", &args);
        assert_eq!(variant, Some(VariantFilter::Vitest));

        let _ = std::fs::remove_file(dir.join("vitest.config.ts"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
