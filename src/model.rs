use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf, str::FromStr, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PdfAStandard {
    #[serde(rename = "1b")]
    A1b,
    #[default]
    #[serde(rename = "2b")]
    A2b,
    #[serde(rename = "3b")]
    A3b,
}

impl PdfAStandard {
    pub const ALL: [Self; 3] = [Self::A1b, Self::A2b, Self::A3b];
    pub fn flavour(self) -> &'static str {
        match self {
            Self::A1b => "1b",
            Self::A2b => "2b",
            Self::A3b => "3b",
        }
    }
    pub fn part(self) -> u8 {
        match self {
            Self::A1b => 1,
            Self::A2b => 2,
            Self::A3b => 3,
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::A1b => "Compatibilità con archivi che richiedono PDF/A-1. Le trasparenze vengono appiattite.",
            Self::A2b => "Mantiene le trasparenze. Sceglilo quando il destinatario accetta PDF/A-2.",
            Self::A3b => "Per destinatari che richiedono PDF/A-3. Questa versione dell'app non conserva allegati incorporati.",
        }
    }
}

impl fmt::Display for PdfAStandard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PDF/A-{}", self.flavour())
    }
}

impl FromStr for PdfAStandard {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().trim_start_matches("pdf/a-") {
            "1b" => Ok(Self::A1b),
            "2b" => Ok(Self::A2b),
            "3b" => Ok(Self::A3b),
            _ => Err("Formato non valido: usa 1b, 2b oppure 3b.".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnginePaths {
    pub ghostscript: PathBuf,
    pub java: PathBuf,
    pub verapdf_jar: PathBuf,
    pub icc_profile: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ConversionOptions {
    pub standard: PdfAStandard,
    pub output_dir: PathBuf,
    pub allow_signed_copy: bool,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobProgress {
    Inspecting,
    Converting,
    Validating,
    Saving,
}

impl fmt::Display for JobProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Inspecting => "Controllo del documento",
            Self::Converting => "Conversione",
            Self::Validating => "Validazione PDF/A",
            Self::Saving => "Salvataggio",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub report_path: PathBuf,
    pub receipt_path: PathBuf,
    pub page_count: usize,
    pub standard: PdfAStandard,
    pub warnings: Vec<String>,
    pub elapsed_ms: u64,
}
