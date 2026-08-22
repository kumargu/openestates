use std::path::{Path, PathBuf};

use backend::search::intent_classifier::{evaluate_classifier, train_classifier};
use serde::Serialize;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments
        .next()
        .unwrap_or_else(|| "train-and-evaluate".to_string());
    let project_root = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_project_root);
    if arguments.next().is_some() {
        return Err(usage());
    }

    match command.as_str() {
        "train" => print_json(&train_classifier(&project_root)?),
        "evaluate" => print_json(&evaluate_classifier(&project_root)?),
        "train-and-evaluate" => {
            let training = train_classifier(&project_root)?;
            let evaluation = evaluate_classifier(&project_root)?;
            print_json(&serde_json::json!({
                "training": training,
                "evaluation": evaluation,
            }))
        }
        _ => Err(usage()),
    }
}

fn default_project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has a repository parent")
        .to_path_buf()
}

fn print_json(value: &impl Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| format!("failed to encode report: {error}"))?
    );
    Ok(())
}

fn usage() -> String {
    "usage: openestates-intent-classifier [train|evaluate|train-and-evaluate] [project-root]"
        .to_string()
}
