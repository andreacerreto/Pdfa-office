# Componenti, licenze e disponibilità dei sorgenti

PDF/A Office 0.1.0 - stato del pacchetto: 27 ottobre 2026.

Il codice dell'applicazione è distribuito con licenza **GNU AGPL versione 3 o
successiva**; il testo è nel file `LICENSE`. I sorgenti dell'applicazione,
`Cargo.lock`, gli script di compilazione e la definizione dell'installer sono
forniti insieme al programma. I componenti seguenti sono distribuiti senza
modifiche ai rispettivi binari. Non sono prodotti o approvati da PDF/A Office.

## Ghostscript 10.08.0

- Titolare e progetto: Artifex Software e altri contributori.
- Licenza del pacchetto utilizzato: GNU AGPLv3; testo originale in
  `resources/ghostscript/doc/COPYING`.
- Binari e sorgenti della versione:
  <https://github.com/ArtifexSoftware/ghostpdl-downloads/releases/tag/gs10080>.
- Repository: <https://github.com/ArtifexSoftware/ghostpdl>.
- Documentazione licenze: <https://artifex.com/licensing>.
- Il profilo sRGB in `resources/profiles/srgb.icc` è una copia del profilo
  distribuito con questa stessa versione di Ghostscript. Gli avvisi originali
  di Ghostscript e delle librerie integrate sono conservati nella distribuzione.

## veraPDF 1.30.3

- Progetto veraPDF, Open Preservation Foundation e contributori.
- Dual licensing GPLv3+ / MPLv2+; testi originali in
  `licenses/verapdf/LICENSE.GPL` e `licenses/verapdf/LICENSE.MPL`.
- Sorgenti CLI e applicazione:
  <https://github.com/veraPDF/veraPDF-apps/tree/v1.30.3>.
- Librerie, parser e profili:
  <https://github.com/veraPDF> (versioni 1.30.3, indicate anche nei metadati
  Maven del JAR distribuito).
- Distribuzione ufficiale:
  <https://software.verapdf.org/rel/1.30/>.
- I testi delle licenze e degli avvisi delle librerie incluse nel JAR sono
  conservati nel JAR originale e copiati in `licenses/verapdf/bundled/`.

## Eclipse Temurin OpenJDK JRE 21.0.12.1+1, Windows x64

- Progetto Eclipse Adoptium / OpenJDK e rispettivi contributori.
- GNU GPL versione 2 con Classpath Exception, più le licenze delle componenti
  di terze parti. L'intera cartella `resources/java/legal/` e il file
  `resources/java/NOTICE` sono inclusi e devono accompagnare il runtime.
- Binari, sorgenti e metadati di questa versione:
  <https://github.com/adoptium/temurin21-binaries/releases/tag/jdk-21.0.12.1%2B1>.
- Repository sorgente: <https://github.com/adoptium/jdk21u>.
- Commit sorgente dichiarato dal runtime: `1c417fbfc2f7`.
- Script di build: <https://github.com/adoptium/temurin-build>, commit
  `e6ba7dec3d07654074559310376a3ae89da5f4ac`.
- Il runtime rimane privato all'applicazione e non modifica il Java del sistema.
- Le DLL Microsoft Visual C++ già presenti nella distribuzione Temurin sono
  copiate senza modifiche anche accanto all'eseguibile Ghostscript, per consentire
  il caricamento locale delle sue dipendenze; restano soggette ai rispettivi
  termini di redistribuzione e non acquisiscono la licenza AGPL.

## Librerie Rust, caratteri e strumenti di compilazione

- L'inventario delle versioni e delle licenze dei crate è in
  `licenses/rust/INDEX.md`; le sottocartelle contengono i testi originali.
- `Cargo.lock` fissa versioni e checksum; i sorgenti integrali di ciascun crate
  si possono ottenere dall'indirizzo crates.io indicato nell'inventario.
- I font incorporati da egui includono le rispettive licenze OFL, Ubuntu Font
  Licence e MIT, copiate con gli altri avvisi.
- Rust: <https://github.com/rust-lang/rust>, MIT / Apache-2.0 e avvisi delle
  librerie standard. La versione del compilatore utilizzata è riportata nel
  rapporto di build.
- La build GNU per Windows può includere `libwinpthread-1.dll` di MinGW-w64.
  Eventuali relative licenze sono nella cartella `licenses/toolchain/`.
- Installer realizzato con NSIS: <https://nsis.sourceforge.io/License>.
  Le licenze degli strumenti e degli stub distribuiti sono conservate in
  `licenses/toolchain/`.

## Ricompilazione e redistribuzione

Le istruzioni in `README.md` e `scripts/build-windows.ps1` descrivono come
ricompilare l'applicazione e ottenere le dipendenze ufficiali; gli URL esatti
dei binari e i relativi SHA-256 sono in `scripts/dependencies.json`.

I sorgenti dell'applicazione devono accompagnare una sua redistribuzione,
insieme a questi avvisi e alle licenze dei componenti. Per una distribuzione
a terzi, mantenere disponibili anche i sorgenti corrispondenti dei componenti
copyleft alle condizioni delle rispettive licenze. I collegamenti precedenti
identificano le versioni utilizzate; non sono una promessa di assistenza o
una garanzia di disponibilità futura da parte dei progetti esterni.

Questa impostazione open source non autorizza una distribuzione proprietaria
in contrasto con le licenze dei componenti. Artifex offre anche licenze
commerciali per Ghostscript.
