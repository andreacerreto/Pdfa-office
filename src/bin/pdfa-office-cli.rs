use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pdfa_office::{
    engine::{convert_pdf, discover_resources},
    model::{ConversionOptions, PdfAStandard},
};
use std::{path::PathBuf, process::ExitCode, sync::atomic::AtomicBool, time::Duration};

#[derive(Parser)]
#[command(
    name = "pdfa-office-cli",
    version,
    about = "Conversione PDF/A locale con validazione obbligatoria veraPDF"
)]
struct Cli {
    /// Cartella resources distribuita con l'applicazione.
    #[arg(long, global = true)]
    resources: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Mostra i componenti locali individuati.
    Doctor,
    /// Converte uno o più PDF; il codice di uscita è 1 se almeno un file fallisce.
    Convert {
        /// Profilo PDF/A richiesto: 1b, 2b, 3b.
        #[arg(short, long, default_value = "2b")]
        format: PdfAStandard,
        /// Cartella in cui salvare PDF e rapporti. Gli originali non sono sovrascritti.
        #[arg(short, long)]
        output: PathBuf,
        /// Consente una nuova copia di un PDF firmato: la firma originale non sarà valida nella copia.
        #[arg(long)]
        allow_signed_copy: bool,
        /// Tempo massimo per ciascun processo esterno, in secondi.
        #[arg(long, default_value = "300", value_parser = clap::value_parser!(u64).range(10..=7200))]
        timeout_seconds: u64,
        /// Emette un riepilogo JSON su stdout; i progressi restano su stderr.
        #[arg(long)]
        json: bool,
        /// Documenti PDF da convertire.
        #[arg(required = true, num_args = 1..)]
        files: Vec<PathBuf>,
    },
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let paths = discover_resources(cli.resources.as_deref())
        .context("Componenti di conversione non disponibili")?;
    match cli.command {
        Commands::Doctor => {
            println!("PDF/A Office {}", env!("CARGO_PKG_VERSION"));
            println!("Ghostscript: {}", paths.ghostscript.display());
            println!("Java:        {}", paths.java.display());
            println!("veraPDF:     {}", paths.verapdf_jar.display());
            println!("Profilo ICC: {}", paths.icc_profile.display());
            println!("Conversione e verifica eseguite esclusivamente sul computer locale.");
            Ok(true)
        }
        Commands::Convert {
            format,
            output,
            allow_signed_copy,
            timeout_seconds,
            json,
            files,
        } => {
            let options = ConversionOptions {
                standard: format,
                output_dir: output,
                allow_signed_copy,
                timeout: Duration::from_secs(timeout_seconds),
            };
            let cancel = AtomicBool::new(false);
            let mut results = Vec::with_capacity(files.len());
            let mut success = true;
            for (index, input) in files.iter().enumerate() {
                eprintln!(
                    "[{}/{}] {} -> {}",
                    index + 1,
                    files.len(),
                    input.display(),
                    format
                );
                match convert_pdf(&paths, input, &options, &cancel, |stage| {
                    eprintln!("  {stage}...")
                }) {
                    Ok(result) => {
                        if !json {
                            println!("CONFORME: {}", result.output_path.display());
                            println!("Rapporto: {}", result.report_path.display());
                            for warning in &result.warnings {
                                eprintln!("Avviso: {warning}");
                            }
                        }
                        results.push(serde_json::json!({ "status": "conforme", "result": result }));
                    }
                    Err(error) => {
                        success = false;
                        let message = format!("{error:#}");
                        eprintln!("ERRORE: {message}");
                        results.push(serde_json::json!({ "status": "errore", "input_path": input, "error": message }));
                    }
                }
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
            Ok(success)
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::from(2)
        }
    }
}
