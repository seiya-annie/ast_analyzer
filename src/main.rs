// src/main.rs
mod config;
mod strategies;

use clap::Parser;
use config::MonitorConfig;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

/// 根据预设规则扫描Rust文件的AST变更
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// 原始文件路径
    #[arg(long)]
    file: String,

    /// 包含旧版代码的临时文件路径
    #[arg(long)]
    old: PathBuf,

    /// 包含新版代码的临时文件路径
    #[arg(long)]
    new: PathBuf,

    /// 监控配置文件的路径 (TOML)
    #[arg(long)]
    config: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();

    // 1. 解析配置文件
    let config_str = match fs::read_to_string(&args.config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading config file {:?}: {}", args.config, e);
            return ExitCode::FAILURE;
        }
    };
    let config: MonitorConfig = match toml::from_str(&config_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error parsing TOML config file: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 2. 读取新旧文件内容
    let old_code = fs::read_to_string(&args.old).unwrap_or_default();
    let new_code = fs::read_to_string(&args.new).unwrap_or_default();

    // 3. 将文件内容解析为 AST
    let old_ast = syn::parse_file(&old_code).unwrap_or_else(|_| syn::parse_file("").unwrap());
    let new_ast = syn::parse_file(&new_code).unwrap_or_else(|_| syn::parse_file("").unwrap());

    let mut all_reports = Vec::new();

    // 4. 应用所有策略
    // 策略 A
    for rule in config.strategy_a.iter().filter(|r| r.file == args.file) {
        all_reports.extend(strategies::analyze_strategy_a(&old_ast, &new_ast, rule));
    }

    // 策略 B (使用原始代码字符串)
    for rule in config.strategy_b.iter().filter(|r| r.file == args.file) {
        all_reports.extend(strategies::analyze_strategy_b(&old_code, &new_code, rule));
    }

    // 策略 C
    for rule in config.strategy_c.iter().filter(|r| r.file == args.file) {
        all_reports.extend(strategies::analyze_strategy_c(&old_ast, &new_ast, rule));
    }

    // 5. 输出报告
    if !all_reports.is_empty() {
        for report in all_reports {
            println!("{}", report);
        }
    }

    ExitCode::SUCCESS
}