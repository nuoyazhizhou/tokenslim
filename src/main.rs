//! src 模块
//!

use env_logger::Env;
use tokenslim::cli::run_cli;
use tokenslim::cli::CliError;
use tokenslim::utils::i18n::t1;

fn main() {
    // 提前拦截 -v / --verbose 参数，注入 debug 日志级别
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "-v" || arg == "--verbose") {
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", "debug");
        }
        if std::env::var("TOKENSLIM_LOG").is_err() {
            std::env::set_var("TOKENSLIM_LOG", "debug");
        }
    }

    tokenslim::core::tracing_init::init_tracing();

    let env = Env::default()
        .filter_or("RUST_LOG", "info")
        .write_style_or("RUST_LOG_STYLE", "always");

    env_logger::init_from_env(env);

    if let Err(e) = run_cli() {
        match e {
            CliError::InvalidArgs(msg) => {
                eprintln!("{}", msg);
                std::process::exit(2);
            }
            other => {
                log::error!("{}", t1("main_unhandled_error", other));
                std::process::exit(1);
            }
        }
    }
}
