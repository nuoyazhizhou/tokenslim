use regex::Regex;
use std::path::Path;

pub fn detect_file(cwd: &Path, pattern: &str) -> bool {
    cwd.join(pattern).exists()
}

pub fn detect_args(args: &[String], pattern: &str) -> bool {
    Regex::new(pattern)
        .ok()
        .is_some_and(|re| re.is_match(&args.join(" ")))
}

pub fn detect_output(output: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .ok()
        .is_some_and(|re| re.is_match(output))
}
