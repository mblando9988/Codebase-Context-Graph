mod config;
mod db;
mod indexer;
mod parser;
mod scanner;
mod server;

use std::path::PathBuf;
use std::process;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = &args[1];
    let project_root = get_project_root(&args);

    let result: Result<(), Box<dyn std::error::Error>> = match command.as_str() {
        "init" => {
            indexer::init_project(&project_root)?;
            Ok(())
        }
        "index" => {
            let advanced = args.contains(&"--analysis-mode".to_string());
            indexer::index_project(&project_root, advanced)?;
            Ok(())
        }
        "watch" => {
            indexer::watch_project(&project_root)?;
            Ok(())
        }
        "smoke" => {
            indexer::smoke_test(&project_root)?;
            Ok(())
        }
        "serve" => {
            server::run(&project_root)?;
            Ok(())
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }

    Ok(())
}

fn print_usage() {
    eprintln!("codebase-context-graph");
    eprintln!("  init [--project-root <path>]");
    eprintln!("  index [--project-root <path>] [--analysis-mode standard|advanced]");
    eprintln!("  watch [--project-root <path>]");
    eprintln!("  smoke [--project-root <path>]");
    eprintln!("  serve [--project-root <path>]");
    eprintln!("No Node.js required.");
}

fn get_project_root(args: &[String]) -> PathBuf {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--project-root" {
            if let Some(path) = iter.next() {
                return PathBuf::from(path);
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
