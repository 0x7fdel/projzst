//! Command-line interface for projzst tool

use clap::{Parser, Subcommand};
use projzst::{info, pack, unpack, FullMetadata, DEFAULT_ZSTD_LEVEL};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "projzst")]
#[command(version, about = "Pack and unpack .pjz files with metadata")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Pack {
        #[arg(short, long)]
        input: PathBuf,

        #[arg(short, long)]
        name: Option<String>,

        #[arg(short, long)]
        auth: Option<String>,

        #[arg(short, long)]
        fmt: Option<String>,

        #[arg(short, long)]
        ed: Option<String>,

        #[arg(short, long)]
        ver: Option<String>,

        #[arg(short, long)]
        desc: Option<String>,

        #[arg(short = 'x', long)]
        extra: Option<PathBuf>,

        #[arg(short, long, default_value_t = DEFAULT_ZSTD_LEVEL)]
        level: i32,

        #[arg(short, long)]
        output: PathBuf,
    },
    Unpack {
        #[arg(short, long)]
        input: PathBuf,

        #[arg(short, long)]
        output: PathBuf,
    },
    Info {
        #[arg(short, long)]
        input: PathBuf,

        #[arg(short, long)]
        output: PathBuf,
    },
}

fn run() -> projzst::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack {
            input,
            name,
            auth,
            fmt,
            ed,
            ver,
            desc,
            extra,
            level,
            output,
        } => {
            let metadata = FullMetadata::new(name, auth, fmt, ed, ver, desc);
            pack(&input, &output, metadata, extra, level)?;
            println!("Successfully packed: {}", output.display());
        }

        Commands::Unpack { input, output } => {
            let metadata = unpack(&input, &output)?;
            println!("Successfully unpacked: {}", output.display());
            println!(
                "Package: {} v{}",
                metadata.name.unwrap_or_default(),
                metadata.ver.unwrap_or_default()
            );
        }

        Commands::Info { input, output } => {
            let metadata = info(&input, &output)?;
            println!("Metadata saved to: {}", output.display());
            if let Some(name) = metadata.name {
                println!("Name: {}", name);
            }
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}
