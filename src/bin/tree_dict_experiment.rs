use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

#[derive(Debug, Default)]
struct TrieNode {
    children: HashMap<String, TrieNode>,
    count: usize,
    is_end: bool,
}

impl TrieNode {
    fn insert(&mut self, segments: &[&str]) {
        self.count += 1;
        if segments.is_empty() {
            self.is_end = true;
            return;
        }
        let child = self.children.entry(segments[0].to_string()).or_default();
        child.insert(&segments[1..]);
    }
}

struct DictionaryBuilder {
    next_d_id: usize,
    directories: HashMap<String, String>, // Token -> Path
}

impl DictionaryBuilder {
    fn new() -> Self {
        Self {
            next_d_id: 1,
            directories: HashMap::new(),
        }
    }

    fn extract_directories(
        &mut self,
        node: &TrieNode,
        parent_token: String,
        path_since_anchor: String,
    ) {
        for (seg, child) in &node.children {
            let mut current_path = if path_since_anchor.is_empty() {
                seg.clone()
            } else {
                format!("{}/{}", path_since_anchor, seg)
            };

            let mut current_token = parent_token.clone();

            // 核心改进：计算“有价值的分支”数量（例如出现次数 > 10 的分支）
            let significant_children = child.children.values().filter(|c| c.count > 10).count();

            // 触发条件：有多个有价值的分支，或者是一个高频的叶子节点，并且路径足够长
            if (significant_children > 1 || (child.children.is_empty() && child.count > 10))
                && current_path.len() > 15
            {
                let new_token = format!("$D{}", self.next_d_id);
                self.next_d_id += 1;

                let dict_value = if parent_token.is_empty() {
                    format!("/{}", current_path)
                } else {
                    format!("{}/{}", parent_token, current_path)
                };

                self.directories.insert(new_token.clone(), dict_value);

                current_token = new_token;
                current_path = String::new();
            }

            self.extract_directories(child, current_token, current_path);
        }
    }

    fn build_resolved_dirs(&self) -> Vec<(String, String)> {
        let mut resolved_dirs = HashMap::new();
        for (k, v) in &self.directories {
            let mut resolved = v.clone();
            if resolved.starts_with("$D") {
                let t: String = resolved.chars().take_while(|c| *c != '/').collect();
                if let Some(parent_val) = self.directories.get(&t) {
                    resolved = resolved.replace(&t, parent_val);
                }
            }
            resolved_dirs.insert(k.clone(), resolved);
        }

        let mut sorted_dirs: Vec<_> = resolved_dirs.into_iter().collect();
        // Sort by length descending to match longest prefix first
        sorted_dirs.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        sorted_dirs
    }

    fn compress_path(
        &self,
        original_path: &str,
        sorted_resolved_dirs: &[(String, String)],
    ) -> String {
        for (token, resolved_path) in sorted_resolved_dirs {
            if original_path.starts_with(resolved_path) {
                return format!("{}{}", token, &original_path[resolved_path.len()..]);
            }
        }
        original_path.to_string()
    }
}

fn main() {
    let start_total = Instant::now();

    println!("1. Reading and extracting paths...");
    let start_extract = Instant::now();
    let file = File::open("benchmarks/input_100mb.txt").expect("Could not open input file");
    let reader = BufReader::with_capacity(1024 * 1024 * 4, file);

    // Simplistic regex for path extraction to simulate our plugin
    let path_re =
        Regex::new(r#"(?:[a-zA-Z]:\\|[/.])[\w\.\-\+_~=@#]+(?:[/\\][\w\.\-\+_~=@#]+)+"#).unwrap();

    let mut unique_paths = HashSet::new();
    for line_result in reader.lines() {
        if let Ok(line) = line_result {
            for caps in path_re.captures_iter(&line) {
                let p = caps.get(0).unwrap().as_str();
                if p.len() > 10 {
                    unique_paths.insert(p.to_string());
                }
            }
        }
    }

    let paths: Vec<String> = unique_paths.into_iter().collect();
    println!(
        "   -> Extracted {} unique paths in {:.2}s",
        paths.len(),
        start_extract.elapsed().as_secs_f64()
    );

    println!("2. Building Radix Trie...");
    let start_trie = Instant::now();
    let mut root = TrieNode::default();
    for p in &paths {
        let segments: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
        root.insert(&segments);
    }
    println!(
        "   -> Trie built in {:.2}ms",
        start_trie.elapsed().as_millis()
    );

    println!("3. Extracting $D Directory Anchors...");
    let start_extract_d = Instant::now();
    let mut builder = DictionaryBuilder::new();
    builder.extract_directories(&root, String::new(), String::new());
    println!(
        "   -> Extracted {} $D anchors in {:.2}ms",
        builder.directories.len(),
        start_extract_d.elapsed().as_millis()
    );

    println!("4. Compressing $P Paths...");
    let start_compress = Instant::now();
    let resolved_dirs = builder.build_resolved_dirs();

    let mut compressed_paths = Vec::with_capacity(paths.len());
    for p in &paths {
        compressed_paths.push(builder.compress_path(p, &resolved_dirs));
    }
    println!(
        "   -> Compressed {} paths in {:.2}ms",
        paths.len(),
        start_compress.elapsed().as_millis()
    );

    println!(
        "Total Pipeline Time: {:.2}s\n",
        start_total.elapsed().as_secs_f64()
    );

    println!("========== Sample $D Directories ==========");
    let mut dirs: Vec<_> = builder.directories.iter().collect();
    dirs.sort_by_key(|(k, _)| k[2..].parse::<usize>().unwrap_or(0));
    for (k, v) in dirs.iter().take(20) {
        println!("{}: {}", k, v);
    }
    if dirs.len() > 20 {
        println!("... and {} more", dirs.len() - 20);
    }

    println!("\n========== Sample $P Paths ==========");
    for (i, cp) in compressed_paths.iter().take(20).enumerate() {
        println!("$P{}: {}", i + 1, cp);
    }
}
