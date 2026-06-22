use clap::Parser;
use regex::Regex;
use std::fs;
use std::io::{self, Read, Write};
use tokenslim::core::log_reorderer::{LogReorderer, ReorderConfig};

#[derive(Parser, Debug)]
#[clap(
    author,
    version,
    about = "TokenSlim Log Reorderer (Standalone Utility)"
)]
struct Args {
    #[clap(short, long)]
    input: Option<String>,

    #[clap(short, long)]
    output: Option<String>,

    #[clap(
        long,
        help = "Enable deterministic A-Z sorting of build targets",
        default_value_t = true
    )]
    deterministic: bool,

    #[clap(
        short = 'n',
        long,
        help = "Normalize lines (sort compiler flags like -I/-L, strip memory addresses)"
    )]
    normalize: bool,

    #[clap(
        short = 'p',
        long,
        help = "Shorten long absolute paths to prevent line wrapping in diff tools"
    )]
    shorten_paths: bool,
}

fn shorten_paths_in_text(text: &str) -> String {
    lazy_static::lazy_static! {
        static ref PATH_RE: Regex = Regex::new(r"(?:[a-zA-Z]:\\|[/.])[\w\.\-\+_~=@#]+(?:[/\\][\w\.\-\+_~=@#]+)+").unwrap();
    }

    PATH_RE
        .replace_all(text, |caps: &regex::Captures| {
            let path = caps.get(0).unwrap().as_str();
            if path.len() > 40
                && (path.starts_with('/') || path.starts_with("C:\\") || path.starts_with("D:\\"))
            {
                let parts: Vec<&str> = path
                    .split(|c| c == '/' || c == '\\')
                    .filter(|s| !s.is_empty())
                    .collect();
                if parts.len() > 3 {
                    let last_parts = &parts[parts.len() - 3..];
                    return format!(".../{}/{}/{}", last_parts[0], last_parts[1], last_parts[2]);
                }
            }
            path.to_string()
        })
        .to_string()
}

fn main() {
    let args = Args::parse();

    let input_text = match args.input {
        Some(path) => {
            let bytes = fs::read(path).expect("Failed to read input file");
            String::from_utf8_lossy(&bytes).into_owned()
        }
        None => {
            let mut buffer = Vec::new();
            io::stdin()
                .read_to_end(&mut buffer)
                .expect("Failed to read from stdin");
            String::from_utf8_lossy(&buffer).into_owned()
        }
    };

    let config = ReorderConfig {
        enabled: true,
        max_lines: 500_000, // Large buffer for deep out-of-order logs
        sticky_context: true,
        deterministic_sort: args.deterministic,
    };

    let mut reorderer = LogReorderer::new(config);

    let lines: Vec<String> = input_text
        .lines()
        .map(|s| {
            let mut line = s.to_string();
            if args.normalize {
                line = reorderer.normalize_line(&line);
            }
            if args.shorten_paths {
                line = shorten_paths_in_text(&line);
            }
            line
        })
        .collect();

    let reordered = reorderer.reorder(lines);

    let output_text = reordered.join("\n");

    match args.output {
        Some(path) => fs::write(path, output_text).expect("Failed to write output file"),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            let _ = handle.write_all(output_text.as_bytes());
        }
    }
}
