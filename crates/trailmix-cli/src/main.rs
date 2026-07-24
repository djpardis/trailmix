use std::{env, error::Error, process::ExitCode};

use trailmix::{AnalysisConfig, AudioBuffer};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("trailmix: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let Some(path) = arguments.next() else {
        return Err("usage: trailmix-cli <audio-file>".into());
    };
    if arguments.next().is_some() {
        return Err("usage: trailmix-cli <audio-file>".into());
    }

    let decoded = trailmix_codecs::decode_file(&path)?;
    let analysis = trailmix::analyze(
        AudioBuffer {
            samples: &decoded.samples,
            sample_rate: decoded.sample_rate,
        },
        AnalysisConfig::default(),
    );
    println!("{}", serde_json::to_string_pretty(&analysis)?);
    Ok(())
}
