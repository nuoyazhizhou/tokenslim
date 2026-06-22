use std::path::Path;

use super::detector::{detect_args, detect_file, detect_output};
use super::types::{VariantConfig, VariantDetect, VariantFilter};

pub fn resolve_variant(
    configs: &[VariantConfig],
    cwd: &Path,
    args: &[String],
    output: &str,
) -> Option<String> {
    for cfg in configs {
        let matched = match &cfg.detect {
            VariantDetect::File { exists } => detect_file(cwd, exists),
            VariantDetect::ArgsPattern { pattern } => detect_args(args, pattern),
            VariantDetect::OutputPattern { pattern } => detect_output(output, pattern),
        };
        if matched {
            return Some(cfg.filter.clone());
        }
    }
    None
}

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
