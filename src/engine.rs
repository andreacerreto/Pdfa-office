//! Local, fail-closed PDF/A conversion. A converted PDF is published only after
//! an independent veraPDF report confirms the requested conformance profile.

use crate::model::{ConversionOptions, ConversionResult, EnginePaths, JobProgress, PdfAStandard};
use anyhow::{anyhow, bail, Context, Result};
use lopdf::{Dictionary, Document, Object};
use roxmltree::{Document as XmlDocument, Node};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_PROCESS_LOG: u64 = 64 * 1024 * 1024;
const MAX_XML: usize = 32 * 1024 * 1024;

/// Explicit paths take precedence over the private bundled runtimes. Environment
/// overrides also make the same conversion pipeline usable in reproducible tests.
pub fn discover_resources(resources: Option<&Path>) -> Result<EnginePaths> {
    let mut roots = Vec::new();
    if let Some(path) = resources {
        roots.push(path.to_path_buf());
    }
    if let Some(path) = env::var_os("PDFA_OFFICE_RESOURCES") {
        roots.push(path.into());
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.join("resources"));
            roots.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = env::current_dir() {
        roots.push(cwd.join("resources"));
    }

    let pick = |variable: &str, relatives: &[&str], system: &[&str]| -> Result<PathBuf> {
        if let Some(explicit) = env::var_os(variable) {
            return resolve_executable_or_file(&explicit)
                .with_context(|| format!("Percorso {variable} non disponibile."));
        }
        for root in &roots {
            for relative in relatives {
                let candidate = root.join(relative);
                if candidate.is_file() {
                    return fs::canonicalize(candidate)
                        .context("Percorso risorse non accessibile.");
                }
            }
        }
        for candidate in system {
            if let Ok(path) = resolve_executable_or_file(OsStr::new(candidate)) {
                return Ok(path);
            }
        }
        bail!("Componente mancante: {variable}. Reinstalla PDF/A Office con i motori inclusi oppure imposta il percorso nelle opzioni avanzate.")
    };

    let ghostscript = pick(
        "PDFA_OFFICE_GS",
        &[
            "ghostscript/bin/gswin64c.exe",
            "ghostscript/bin/gs",
            "ghostscript/gs",
        ],
        if cfg!(windows) {
            &["gswin64c.exe"]
        } else {
            &["gs"]
        },
    )?;
    let java = pick(
        "PDFA_OFFICE_JAVA",
        &[
            "java/bin/java.exe",
            "jre/bin/java.exe",
            "java/bin/java",
            "jre/bin/java",
        ],
        if cfg!(windows) {
            &["java.exe"]
        } else {
            &["java"]
        },
    )?;
    let verapdf_jar = match pick(
        "PDFA_OFFICE_VERAPDF_JAR",
        &[
            "verapdf/verapdf-cli.jar",
            "verapdf/bin/cli-1.30.3.jar",
            "verapdf/bin/cli-1.28.2.jar",
        ],
        &[],
    ) {
        Ok(path) => path,
        Err(err) => {
            if env::var_os("PDFA_OFFICE_VERAPDF_JAR").is_some() {
                return Err(err);
            }
            roots
                .iter()
                .find_map(|root| find_cli_jar(&root.join("verapdf"), 3))
                .ok_or(err)?
        }
    };
    let icc_profile = pick(
        "PDFA_OFFICE_ICC",
        &[
            "profiles/srgb.icc",
            "ghostscript/iccprofiles/srgb.icc",
            "ghostscript/iccprofiles/default_rgb.icc",
        ],
        &[
            "/usr/share/color/icc/ghostscript/srgb.icc",
            "/usr/share/ghostscript/iccprofiles/srgb.icc",
            "/usr/share/color/icc/ghostscript/default_rgb.icc",
        ],
    )?;
    Ok(EnginePaths {
        ghostscript,
        java,
        verapdf_jar,
        icc_profile,
    })
}

fn resolve_executable_or_file(value: &OsStr) -> Result<PathBuf> {
    let supplied = Path::new(value);
    if supplied.is_file() {
        return Ok(fs::canonicalize(supplied)?);
    }
    if supplied.components().count() == 1 {
        if let Some(path) = env::var_os("PATH") {
            for dir in env::split_paths(&path) {
                let candidate = dir.join(value);
                if candidate.is_file() {
                    return Ok(fs::canonicalize(candidate)?);
                }
            }
        }
    }
    bail!("File non trovato: {}", supplied.display())
}

fn find_cli_jar(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let kind = entry.file_type().ok()?;
        let path = entry.path();
        if kind.is_file() {
            let name = path.file_name()?.to_str()?;
            if name == "verapdf-cli.jar" || (name.starts_with("cli-") && name.ends_with(".jar")) {
                return fs::canonicalize(path).ok();
            }
        } else if kind.is_dir() {
            if let Some(found) = find_cli_jar(&path, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct ValidationSummary {
    pub profile_name: String,
    pub verapdf_version: String,
    pub document_name: String,
}

/// Do not accept merely the string `isCompliant="true"`: interrupted batches,
/// the wrong profile and empty/duplicate jobs must all fail closed.
pub fn validate_report(xml: &str, standard: PdfAStandard) -> Result<ValidationSummary> {
    if xml.len() > MAX_XML {
        bail!("Il rapporto veraPDF supera il limite di sicurezza.");
    }
    let document = XmlDocument::parse(xml).context("Il rapporto veraPDF non è XML valido.")?;
    let root = document.root_element();
    if root.tag_name().name() != "report" {
        bail!("Il rapporto veraPDF non contiene la radice prevista.");
    }
    if root.descendants().any(|n| {
        n.is_element()
            && n.tag_name()
                .name()
                .to_ascii_lowercase()
                .contains("exception")
    }) {
        bail!("veraPDF ha segnalato un errore durante la verifica; il file non viene pubblicato.");
    }
    let jobs = unique_child(root, "jobs")?;
    let job = unique_child(jobs, "job")?;
    if root.descendants().filter(|n| n.has_tag_name("job")).count() != 1 {
        bail!("Il rapporto veraPDF deve contenere esattamente un documento.");
    }
    let reports: Vec<_> = root
        .descendants()
        .filter(|n| n.has_tag_name("validationReport"))
        .collect();
    if reports.len() != 1 {
        bail!("Il rapporto veraPDF è incompleto o contiene più verifiche del previsto.");
    }
    let report = unique_child(job, "validationReport")?;
    let expected = format!("PDF/A-{} validation profile", standard.flavour());
    let profile_name = report
        .attribute("profileName")
        .context("Profilo di validazione mancante.")?;
    if !profile_name.eq_ignore_ascii_case(&expected) {
        bail!("veraPDF ha verificato un profilo diverso da {standard}.");
    }
    if report.attribute("isCompliant") != Some("true") {
        let failures: Vec<_> = report
            .descendants()
            .filter(|n| n.has_tag_name("description"))
            .filter_map(|n| n.text())
            .take(4)
            .map(str::trim)
            .collect();
        bail!(
            "Il documento prodotto non è conforme a {standard}. {}",
            failures.join(" | ")
        );
    }
    let details = unique_child(report, "details")?;
    require_number(details, "failedRules", 0)?;
    require_number(details, "failedChecks", 0)?;
    for attr in ["passedRules", "passedChecks"] {
        if number_attribute(details, attr)? == 0 {
            bail!("Il rapporto veraPDF non documenta controlli completati.");
        }
    }
    let summary = unique_child(root, "batchSummary")?;
    require_number(summary, "totalJobs", 1)?;
    for attr in [
        "failedToParse",
        "encrypted",
        "outOfMemory",
        "veraExceptions",
    ] {
        require_number(summary, attr, 0)?;
    }
    let counts = unique_child(summary, "validationReports")?;
    require_number(counts, "compliant", 1)?;
    require_number(counts, "nonCompliant", 0)?;
    require_number(counts, "failedJobs", 0)?;
    if counts.text().map(str::trim) != Some("1") {
        bail!("Conteggio dei rapporti veraPDF non coerente.");
    }
    let item = unique_child(job, "item")?;
    let item_name = unique_child(item, "name")?
        .text()
        .filter(|s| !s.trim().is_empty())
        .context("Nome documento assente nel rapporto veraPDF.")?;
    let version = root
        .descendants()
        .find(|n| n.has_tag_name("releaseDetails") && n.attribute("id") == Some("core"))
        .and_then(|n| n.attribute("version"))
        .filter(|s| !s.is_empty())
        .context("Versione veraPDF non presente nel rapporto.")?;
    Ok(ValidationSummary {
        profile_name: profile_name.to_owned(),
        verapdf_version: version.to_owned(),
        document_name: item_name.to_owned(),
    })
}

fn unique_child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Result<Node<'a, 'input>> {
    let mut found = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == name);
    let first = found
        .next()
        .with_context(|| format!("Campo {name} mancante nel rapporto veraPDF."))?;
    if found.next().is_some() {
        bail!("Campo {name} duplicato nel rapporto veraPDF.");
    }
    Ok(first)
}

fn number_attribute(node: Node<'_, '_>, name: &str) -> Result<u64> {
    node.attribute(name)
        .with_context(|| format!("Contatore {name} assente nel rapporto veraPDF."))?
        .parse()
        .with_context(|| format!("Contatore {name} non valido nel rapporto veraPDF."))
}

fn require_number(node: Node<'_, '_>, name: &str, expected: u64) -> Result<()> {
    if number_attribute(node, name)? != expected {
        bail!("Il rapporto veraPDF segnala un esito non valido ({name}).");
    }
    Ok(())
}

pub fn convert_pdf(
    paths: &EnginePaths,
    input: &Path,
    options: &ConversionOptions,
    cancel: &AtomicBool,
    on_progress: impl Fn(JobProgress),
) -> Result<ConversionResult> {
    let started = Instant::now();
    ensure_not_cancelled(cancel)?;
    if options.timeout.is_zero() {
        bail!("Il limite di tempo deve essere maggiore di zero.");
    }
    let input = fs::canonicalize(input).context("Il PDF di origine non è accessibile.")?;
    if !input.is_file() {
        bail!("Seleziona un file PDF regolare.");
    }
    fs::create_dir_all(&options.output_dir)
        .context("Impossibile creare la cartella di destinazione.")?;
    let output_dir = fs::canonicalize(&options.output_dir)
        .context("Cartella di destinazione non accessibile.")?;
    let work = tempfile::Builder::new()
        .prefix(".pdfa-office-")
        .tempdir_in(&output_dir)
        .context("Non è possibile creare i file temporanei nella cartella di destinazione.")?;
    let result = convert_in_workdir(
        paths,
        &input,
        options,
        cancel,
        &on_progress,
        started,
        &output_dir,
        work.path(),
    );
    let result = match result {
        Ok(value) => Ok(value),
        Err(error) if cancel.load(Ordering::Acquire) => Err(error),
        Err(error) => {
            match save_failure_log(&input, options.standard, &output_dir, work.path(), &error) {
                Ok(log) => Err(anyhow!("{error:#}\nDettagli salvati in {}", log.display())),
                Err(_) => Err(error),
            }
        }
    };
    // Drop would silently ignore deletion failures (for example an antivirus
    // still holding a file on Windows). Explicit close makes cleanup observable.
    match (result, cleanup_workdir(work)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(value), Err(cleanup)) => Err(cleanup.context(format!(
            "Il PDF conforme è stato salvato in {}, ma la pulizia dei file temporanei non è stata completata.",
            value.output_path.display()
        ))),
        (Err(error), Err(cleanup)) => Err(error.context(format!("Pulizia dei file temporanei non completata: {cleanup:#}"))),
    }
}

fn cleanup_workdir(work: tempfile::TempDir) -> Result<()> {
    // Only this exact, exclusively created job directory may be removed.
    // Never scan or delete similarly named directories from other jobs.
    let path = work.path().to_path_buf();
    let mut last_error = match work.close() {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => error,
    };
    for milliseconds in [40, 80, 160] {
        thread::sleep(Duration::from_millis(milliseconds));
        match fs::remove_dir_all(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last_error = error,
        }
    }
    Err(last_error).with_context(|| {
        format!(
            "Impossibile rimuovere la cartella temporanea {}",
            path.display()
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn convert_in_workdir(
    paths: &EnginePaths,
    input: &Path,
    options: &ConversionOptions,
    cancel: &AtomicBool,
    on_progress: &impl Fn(JobProgress),
    started: Instant,
    output_dir: &Path,
    work: &Path,
) -> Result<ConversionResult> {
    on_progress(JobProgress::Inspecting);
    fs::copy(input, work.join("input.pdf"))
        .context("Impossibile preparare la copia temporanea del PDF.")?;
    ensure_not_cancelled(cancel)?;
    let (page_count, mut warnings) =
        inspect_input(&work.join("input.pdf"), options.allow_signed_copy)?;
    if options.standard == PdfAStandard::A1b {
        warnings.push("PDF/A-1b può richiedere l'appiattimento delle trasparenze. Controlla la resa visiva del risultato.".into());
    }
    let original_hash = sha256_file(&work.join("input.pdf"))?;
    let original_size = fs::metadata(work.join("input.pdf"))?.len();
    fs::copy(&paths.icc_profile, work.join("srgb.icc"))
        .context("Profilo colore ICC non disponibile.")?;
    check_rgb_profile(&work.join("srgb.icc"))?;
    let icc = fs::canonicalize(work.join("srgb.icc"))?;
    let icc_external = external_runtime_path(&icc);
    let icc_text = icc_external
        .to_str()
        .context("Il percorso temporaneo contiene caratteri non rappresentabili.")?
        .replace('\\', "/");
    fs::write(work.join("pdfa.ps"), pdfa_definition(&icc_text))?;

    let mut version_command = Command::new(external_runtime_path(&paths.ghostscript));
    version_command.arg("--version");
    let version_run = run_command(
        version_command,
        work,
        "ghostscript-version",
        cancel,
        options.timeout.min(Duration::from_secs(15)),
    )?;
    if !version_run.success() {
        bail!("Il motore Ghostscript non si avvia.");
    }
    let ghostscript_version = read_small_text(&work.join("ghostscript-version.stdout"))?
        .trim()
        .to_string();
    let gs_parts: Vec<u32> = ghostscript_version
        .split('.')
        .map(|s| s.parse().unwrap_or(0))
        .collect();
    if gs_parts.first().copied().unwrap_or(0) < 10 {
        bail!("È richiesto Ghostscript 10 o successivo.");
    }

    ensure_not_cancelled(cancel)?;
    on_progress(JobProgress::Converting);
    let mut gs = Command::new(external_runtime_path(&paths.ghostscript));
    gs.args([
        "-dSAFER",
        "-dBATCH",
        "-dNOPAUSE",
        "-sDEVICE=pdfwrite",
        "-dPDFACompatibilityPolicy=2",
        "-sColorConversionStrategy=RGB",
        "-sProcessColorModel=DeviceRGB",
        "-dEmbedAllFonts=true",
        "-dEmbedSubstituteFonts=true",
        "-dAutoRotatePages=/None",
        "-dDownsampleColorImages=false",
        "-dDownsampleGrayImages=false",
        "-dDownsampleMonoImages=false",
    ])
    .arg(format!("-dPDFA={}", options.standard.part()))
    .arg(format!("--permit-file-read={icc_text}"))
    .arg(format!("-sOutputICCProfile={icc_text}"));
    if options.standard != PdfAStandard::A1b
        && (gs_parts[0] > 10 || gs_parts.get(1).copied().unwrap_or(0) >= 7)
    {
        gs.arg("-sBlendConversionStrategy=Managed");
    }
    gs.args(["-sOutputFile=converted.pdf", "pdfa.ps", "input.pdf"]);
    let status = run_command(gs, work, "ghostscript", cancel, options.timeout)?;
    if !status.success() {
        bail!(
            "Ghostscript non ha completato la conversione. {}",
            diagnostic_tail(work, "ghostscript")
        );
    }
    let generated = work.join("converted.pdf");
    if !generated.is_file() || fs::metadata(&generated)?.len() == 0 {
        bail!("Il motore non ha prodotto un documento PDF.");
    }
    let output_doc =
        Document::load(&generated).context("Il documento prodotto non è un PDF leggibile.")?;
    if output_doc.is_encrypted()
        || output_doc.encryption_state.is_some()
        || output_doc.get_pages().len() != page_count
    {
        bail!(
            "Il documento prodotto non conserva il numero di pagine originale o risulta cifrato."
        );
    }
    drop(output_doc);
    let gs_diagnostics = diagnostic_tail(work, "ghostscript");
    let lower_diagnostics = gs_diagnostics.to_ascii_lowercase();
    // Ghostscript prints "Loading font ... (or substitute)" even during normal
    // font loading. Flag explicit substitution messages instead of that boilerplate.
    if lower_diagnostics.contains("substituting") || lower_diagnostics.contains("substitute font") {
        warnings.push(
            "Ghostscript segnala una sostituzione di font: confronta l'aspetto con l'originale."
                .into(),
        );
    }
    if gs_diagnostics.to_ascii_lowercase().contains("warning")
        || gs_diagnostics.to_ascii_lowercase().contains("repaired")
    {
        warnings.push(
            "Ghostscript ha emesso avvisi sul documento; i dettagli sono riportati nella ricevuta."
                .into(),
        );
    }

    ensure_not_cancelled(cancel)?;
    on_progress(JobProgress::Validating);
    let mut java = Command::new(external_runtime_path(&paths.java));
    java.args([
        "-Xmx1024m",
        "-Dfile.encoding=UTF-8",
        "-Djava.awt.headless=true",
        "--add-exports=java.base/sun.security.pkcs=ALL-UNNAMED",
        "-jar",
    ])
    .arg(external_runtime_path(&paths.verapdf_jar))
    .args([
        "--format",
        "xml",
        "--flavour",
        options.standard.flavour(),
        "--maxfailuresdisplayed",
        "10",
        "converted.pdf",
    ]);
    let validator_status = run_command(java, work, "verapdf", cancel, options.timeout)?;
    let xml = read_small_text(&work.join("verapdf.stdout"))?;
    let validation = validate_report(&xml, options.standard).with_context(|| {
        format!(
            "Validazione indipendente {standard} non superata.",
            standard = options.standard
        )
    })?;
    if !validator_status.success() {
        bail!(
            "veraPDF ha restituito un errore: {}",
            diagnostic_tail(work, "verapdf")
        );
    }
    if Path::new(&validation.document_name).file_name() != Some(OsStr::new("converted.pdf")) {
        bail!("Il rapporto veraPDF non si riferisce al file prodotto.");
    }
    ensure_not_cancelled(cancel)?;
    let output_hash = sha256_file(&generated)?;
    let report_hash = sha256_file(&work.join("verapdf.stdout"))?;
    on_progress(JobProgress::Saving);
    let publication = reserve_output(output_dir, input, options.standard)?;
    let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let result = ConversionResult {
        input_path: input.to_path_buf(),
        output_path: publication.pdf.clone(),
        report_path: publication.report.clone(),
        receipt_path: publication.receipt.clone(),
        page_count,
        standard: options.standard,
        warnings: warnings.clone(),
        elapsed_ms,
    };
    let receipt = serde_json::json!({
        "schema_version": 1,
        "application": { "name": "PDF/A Office", "version": env!("CARGO_PKG_VERSION") },
        "created_unix_seconds": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "result": result,
        "input": { "path": input, "sha256": original_hash, "bytes": original_size },
        "output": { "path": publication.pdf, "sha256": output_hash, "bytes": fs::metadata(&generated)?.len() },
        "validation": { "engine": "veraPDF", "version": validation.verapdf_version, "profile": validation.profile_name,
            "is_compliant": true, "report_sha256": report_hash },
        "conversion": { "engine": "Ghostscript", "version": ghostscript_version, "diagnostics": gs_diagnostics },
        "note": "La conformità PDF/A è verificata automaticamente. La verifica non certifica la validità di firme, l'equivalenza del contenuto o la resa visiva rispetto all'originale."
    });
    fs::write(
        work.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    // Sidecars first; the validated PDF is the final commit point. Every move is
    // no-clobber. An output from a concurrent job/user is never overwritten.
    let mut committed = PublishedFiles::default();
    ensure_not_cancelled(cancel)?;
    publish_noclobber(work.join("verapdf.stdout"), &publication.report)?;
    committed.paths.push(publication.report.clone());
    ensure_not_cancelled(cancel)?;
    publish_noclobber(work.join("receipt.json"), &publication.receipt)?;
    committed.paths.push(publication.receipt.clone());
    ensure_not_cancelled(cancel)?;
    publish_noclobber(generated, &publication.pdf)?;
    committed.paths.push(publication.pdf.clone());
    committed.complete = true;
    Ok(result)
}

fn dereference<'a>(doc: &'a Document, mut value: &'a Object) -> Option<&'a Object> {
    for _ in 0..32 {
        match value {
            Object::Reference(id) => value = doc.objects.get(id)?,
            _ => return Some(value),
        }
    }
    None
}

fn dict_value<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    dereference(doc, dict.get(key).ok()?)
}

fn is_name(value: Option<&Object>, name: &[u8]) -> bool {
    matches!(value, Some(Object::Name(actual)) if actual.as_slice() == name)
}

fn inspect_input(path: &Path, allow_signed_copy: bool) -> Result<(usize, Vec<String>)> {
    let doc = Document::load(path)
        .context("Il PDF non è leggibile oppure è protetto. Usa una copia PDF non cifrata.")?;
    if doc.is_encrypted() || doc.encryption_state.is_some() || doc.trailer.get(b"Encrypt").is_ok() {
        bail!("Il documento è cifrato. Fornisci una copia PDF sbloccata prima di convertirlo.");
    }
    let page_count = doc.get_pages().len();
    if page_count == 0 {
        bail!("Il documento non contiene pagine convertibili.");
    }
    let mut signed = false;
    let mut forms = false;
    let mut interactive = false;
    let mut pending: Vec<_> = doc
        .objects
        .values()
        .map(|object| (object, 0usize))
        .collect();
    while let Some((object, depth)) = pending.pop() {
        if depth > 128 {
            bail!("Il PDF contiene una struttura troppo complessa per il controllo preliminare.");
        }
        if let Object::Array(items) = object {
            pending.extend(items.iter().map(|child| (child, depth + 1)));
            continue;
        }
        let dictionary = match object {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        // Indirect objects are already in doc.objects; inspect nested direct
        // dictionaries too (e.g. a signature directly stored in a field's /V).
        pending.extend(dictionary.iter().map(|(_, child)| (child, depth + 1)));
        if is_name(dict_value(&doc, dictionary, b"Type"), b"EmbeddedFile")
            || dict_value(&doc, dictionary, b"EF").is_some()
            || dict_value(&doc, dictionary, b"EmbeddedFiles").is_some()
            || dict_value(&doc, dictionary, b"Collection").is_some()
            || matches!(dict_value(&doc, dictionary, b"AF"), Some(Object::Array(a)) if !a.is_empty())
        {
            bail!("Il PDF contiene allegati incorporati o un portfolio. Questa versione non ne garantisce la conservazione: estrai gli allegati e converti una copia priva di allegati.");
        }
        if dict_value(&doc, dictionary, b"XFA").is_some() {
            bail!(
                "I moduli XFA dinamici non sono supportati. Esporta prima una copia PDF statica."
            );
        }
        if dict_value(&doc, dictionary, b"FT").is_some() {
            forms = true;
        }
        if is_name(dict_value(&doc, dictionary, b"S"), b"JavaScript")
            || is_name(dict_value(&doc, dictionary, b"S"), b"Launch")
        {
            interactive = true;
        }
        let byte_range = matches!(dict_value(&doc, dictionary, b"ByteRange"), Some(Object::Array(a)) if a.len() >= 4 && a.iter().any(|x| matches!(x, Object::Integer(i) if *i > 0)));
        let contents = matches!(dict_value(&doc, dictionary, b"Contents"), Some(Object::String(s, _)) if s.iter().any(|b| *b != 0));
        if byte_range && contents {
            signed = true;
        }
    }
    if signed && !allow_signed_copy {
        bail!("Il PDF contiene una firma digitale o una marca temporale. La conversione crea una copia priva della validità della firma originale. Abilita esplicitamente la conversione di una copia firmata per continuare.");
    }
    let mut warnings = Vec::new();
    if signed {
        warnings.push("Il documento di origine contiene una firma: il risultato è una nuova copia e non conserva la validità della firma originale.".into());
    }
    if forms {
        warnings.push("Il documento contiene campi di modulo. La conversione può appiattirli: verifica che i valori siano visibili nel risultato.".into());
    }
    if interactive {
        warnings.push(
            "Il documento contiene azioni interattive che non saranno conservate nel PDF/A.".into(),
        );
    }
    Ok((page_count, warnings))
}

fn check_rgb_profile(path: &Path) -> Result<()> {
    let mut header = [0u8; 128];
    File::open(path)?
        .read_exact(&mut header)
        .context("Profilo ICC incompleto.")?;
    if &header[36..40] != b"acsp" || &header[16..20] != b"RGB " {
        bail!("Il profilo colore deve essere un profilo ICC RGB valido.");
    }
    Ok(())
}

fn postscript_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('(');
    for byte in value.bytes() {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            32..=126 => out.push(byte as char),
            _ => out.push_str(&format!("\\{byte:03o}")),
        }
    }
    out.push(')');
    out
}

fn pdfa_definition(icc: &str) -> String {
    format!("%!PS-Adobe-3.0\n/ICCProfile {} def\n\
[/_objdef {{icc_PDFA}} /type /stream /OBJ pdfmark\n\
[{{icc_PDFA}} << /N 3 >> /PUT pdfmark\n\
[{{icc_PDFA}} ICCProfile (r) file /PUT pdfmark\n\
[/_objdef {{OutputIntent_PDFA}} /type /dict /OBJ pdfmark\n\
[{{OutputIntent_PDFA}} << /Type /OutputIntent /S /GTS_PDFA1 /DestOutputProfile {{icc_PDFA}} /OutputConditionIdentifier (sRGB) >> /PUT pdfmark\n\
[{{Catalog}} << /OutputIntents [{{OutputIntent_PDFA}}] >> /PUT pdfmark\n", postscript_literal(icc))
}

fn ensure_not_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        bail!("Conversione annullata. Il documento originale non è stato modificato.");
    }
    Ok(())
}

// std::fs::canonicalize on Windows produces verbatim paths (\\?\C:\...).
// Native command-line programs and ICC PostScript strings do not consistently
// accept that prefix, especially after converting backslashes to slashes.
pub(crate) fn external_runtime_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        return PathBuf::from(without_verbatim_prefix(text));
    }
    path.to_path_buf()
}

#[cfg(any(windows, test))]
fn without_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_owned();
    }
    path.to_owned()
}

fn run_command(
    mut command: Command,
    work: &Path,
    label: &str,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<ExitStatus> {
    ensure_not_cancelled(cancel)?;
    let stdout_path = work.join(format!("{label}.stdout"));
    let stderr_path = work.join(format!("{label}.stderr"));
    command
        .current_dir(work)
        .stdin(Stdio::null())
        .stdout(Stdio::from(File::create(&stdout_path)?))
        .stderr(Stdio::from(File::create(&stderr_path)?))
        .env_remove("GS_OPTIONS")
        .env_remove("JAVA_TOOL_OPTIONS")
        .env_remove("_JAVA_OPTIONS")
        .env_remove("JDK_JAVA_OPTIONS");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().with_context(|| {
        format!("Impossibile avviare {label}. Verifica l'installazione dei motori.")
    })?;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            ensure_not_cancelled(cancel)?;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{label} ha superato il limite di {} secondi. Prova con un limite maggiore per documenti complessi.", timeout.as_secs());
        }
        let log_size = fs::metadata(&stdout_path)
            .map(|m| m.len())
            .unwrap_or(0)
            .saturating_add(fs::metadata(&stderr_path).map(|m| m.len()).unwrap_or(0));
        if log_size > MAX_PROCESS_LOG {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{label} ha prodotto un rapporto eccessivamente grande. Il processo è stato interrotto.");
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(60)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error).context("Errore nel controllo del processo di conversione.");
            }
        }
    }
}

fn read_small_text(path: &Path) -> Result<String> {
    let bytes = fs::read(path).context("Impossibile leggere il rapporto del motore.")?;
    if bytes.len() > MAX_XML {
        bail!("Il rapporto supera la dimensione massima prevista.");
    }
    String::from_utf8(bytes).context("Il rapporto del motore non contiene testo UTF-8 valido.")
}

fn diagnostic_tail(work: &Path, label: &str) -> String {
    let mut text = String::new();
    for extension in ["stdout", "stderr"] {
        if let Ok(bytes) = fs::read(work.join(format!("{label}.{extension}"))) {
            let start = bytes.len().saturating_sub(8192);
            text.push_str(&String::from_utf8_lossy(&bytes[start..]));
            text.push('\n');
        }
    }
    text.trim().to_string()
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

struct Publication {
    pdf: PathBuf,
    report: PathBuf,
    receipt: PathBuf,
    reservation: PathBuf,
    lock: Option<File>,
}

impl Drop for Publication {
    fn drop(&mut self) {
        drop(self.lock.take());
        let _ = fs::remove_file(&self.reservation);
    }
}

fn safe_stem(input: &Path) -> String {
    let name = input
        .file_stem()
        .unwrap_or_else(|| OsStr::new("documento"))
        .to_string_lossy();
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c == '.' || c == ' ');
    if cleaned.is_empty() {
        "documento".into()
    } else {
        cleaned.into()
    }
}

fn reserve_output(output: &Path, input: &Path, standard: PdfAStandard) -> Result<Publication> {
    let stem = format!("{}_pdfa-{}", safe_stem(input), standard.flavour());
    for index in 0..10000 {
        let base = if index == 0 {
            stem.clone()
        } else {
            format!("{stem}_{index}")
        };
        let pdf = output.join(format!("{base}.pdf"));
        let report = output.join(format!("{base}.verapdf.xml"));
        let receipt = output.join(format!("{base}.ricevuta.json"));
        let reservation = output.join(format!(".{base}.pdfa-office-lock"));
        let lock = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&reservation)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).context("Impossibile prenotare il nome del PDF di destinazione.")
            }
        };
        let candidate = Publication {
            pdf,
            report,
            receipt,
            reservation,
            lock: Some(lock),
        };
        if candidate.pdf.exists() || candidate.report.exists() || candidate.receipt.exists() {
            continue;
        }
        return Ok(candidate);
    }
    bail!("Troppi file con lo stesso nome nella cartella di destinazione.")
}

fn publish_noclobber(source: PathBuf, destination: &Path) -> Result<()> {
    #[allow(deprecated)]
    let temporary = tempfile::TempPath::from_path(source);
    temporary.persist_noclobber(destination).map_err(|error| {
        anyhow!(
            "Impossibile pubblicare {} senza sovrascrivere file: {}",
            destination.display(),
            error.error
        )
    })?;
    Ok(())
}

#[derive(Default)]
struct PublishedFiles {
    paths: Vec<PathBuf>,
    complete: bool,
}
impl Drop for PublishedFiles {
    fn drop(&mut self) {
        if !self.complete {
            for path in self.paths.iter().rev() {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn save_failure_log(
    input: &Path,
    standard: PdfAStandard,
    output: &Path,
    work: &Path,
    error: &anyhow::Error,
) -> Result<PathBuf> {
    let base = format!("{}_pdfa-{}_errore", safe_stem(input), standard.flavour());
    for index in 0..10000 {
        let name = if index == 0 {
            format!("{base}.log")
        } else {
            format!("{base}_{index}.log")
        };
        let path = output.join(name);
        let mut file = match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        writeln!(file, "PDF/A Office — conversione non completata\nOrigine: {}\nProfilo: {}\nErrore: {error:#}\n", input.display(), standard)?;
        for label in ["ghostscript-version", "ghostscript", "verapdf"] {
            writeln!(file, "\n[{label}]\n{}", diagnostic_tail(work, label))?;
        }
        return Ok(path);
    }
    bail!("Impossibile salvare il rapporto di errore.")
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn postscript_path_cannot_escape_literal() {
        assert_eq!(
            postscript_literal("C:/PDF (A)/a\\b\n.icc"),
            "(C:/PDF \\(A\\)/a\\\\b\\012.icc)"
        );
        assert_eq!(postscript_literal("è.icc"), "(\\303\\250.icc)");
    }

    #[test]
    fn windows_runtime_paths_remove_verbatim_prefixes() {
        assert_eq!(
            without_verbatim_prefix(r"\\?\C:\Users\Andrea\profilo.icc"),
            r"C:\Users\Andrea\profilo.icc"
        );
        assert_eq!(
            without_verbatim_prefix(r"\\?\UNC\server\studio\profilo.icc"),
            r"\\server\studio\profilo.icc"
        );
        assert_eq!(
            without_verbatim_prefix(r"C:\PDF (A)\profilo.icc"),
            r"C:\PDF (A)\profilo.icc"
        );
    }

    #[test]
    fn reservations_avoid_all_existing_outputs_and_concurrent_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let first =
            reserve_output(dir.path(), Path::new("pratica.pdf"), PdfAStandard::A2b).unwrap();
        let second =
            reserve_output(dir.path(), Path::new("pratica.pdf"), PdfAStandard::A2b).unwrap();
        assert_ne!(first.pdf, second.pdf);
        fs::write(&first.report, "do not overwrite").unwrap();
        let first_pdf = first.pdf.clone();
        drop(first);
        let third =
            reserve_output(dir.path(), Path::new("pratica.pdf"), PdfAStandard::A2b).unwrap();
        assert_ne!(first_pdf, third.pdf);
    }

    #[test]
    fn publishing_never_replaces_an_existing_document() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("temporary.pdf");
        let target = dir.path().join("original.pdf");
        fs::write(&source, "converted").unwrap();
        fs::write(&target, "original").unwrap();
        assert!(publish_noclobber(source, &target).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "original");
    }

    #[cfg(unix)]
    #[test]
    fn timed_out_process_is_reaped_and_its_directory_is_cleaned() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().to_path_buf();
        let cancel = AtomicBool::new(false);
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        let error = run_command(
            command,
            work.path(),
            "timeout-test",
            &cancel,
            Duration::from_millis(150),
        )
        .unwrap_err();
        assert!(error.to_string().contains("limite"));
        cleanup_workdir(work).unwrap();
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_process_is_reaped_and_its_directory_is_cleaned() {
        let work = tempfile::tempdir().unwrap();
        let path = work.path().to_path_buf();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let trigger = cancel.clone();
        let thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            trigger.store(true, Ordering::Release);
        });
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        let error = run_command(
            command,
            work.path(),
            "cancel-test",
            &cancel,
            Duration::from_secs(5),
        )
        .unwrap_err();
        thread.join().unwrap();
        assert!(error.to_string().contains("annullata"));
        cleanup_workdir(work).unwrap();
        assert!(!path.exists());
    }
}
