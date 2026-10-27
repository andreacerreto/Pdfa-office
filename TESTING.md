# Verifiche della versione 0.1.1

Data: 27 ottobre 2026. Aggiornamento dell'interfaccia e del confezionamento.

- Compilazione release su Windows x64 con Rust 1.99.1, target
  `x86_64-pc-windows-gnu` e GCC/MinGW-w64 WinLibs.
- `cargo fmt --check` superato e **9 test Rust superati su Windows**:
  4 test unitari e 5 test di integrazione. I due test riservati a Unix su
  timeout e annullamento non sono eseguiti su Windows.
- Verifica dell'interfaccia aggiornata nell'installazione Windows: aggiungendo
  un PDF senza destinazione compaiono l'avviso e il pulsante attivo
  **Scegli destinazione...**. Scelta la cartella, torna **Converti e verifica**.
- Conversione di un PDF sintetico dalla GUI e dalla CLI, con i motori del
  pacchetto: risultato PDF/A-2b conforme secondo veraPDF.
- Il confezionamento individua la directory tramite Cargo, rispettando anche
  `CARGO_TARGET_DIR` e le impostazioni locali di `build.target-dir`.
- Lo script NSIS usa percorsi Windows nativi e viene letto come UTF-8 per
  mantenere corretti i testi italiani dell'installer.

Le prove riguardano un campione sintetico e non costituiscono un collaudo
esaustivo di ogni documento o configurazione Windows. Il collaudo precedente
dei profili e dei casi particolari resta documentato di seguito.

---

# Verifiche della versione 0.1.0

Data: 27 ottobre 2026. Tutti i documenti usati sono sintetici e non contengono
dati di clienti. Gli esiti descrivono il campione provato; non sono una garanzia
di conversione universale di qualsiasi PDF.

## Compilazione e controlli Rust

- Toolchain utilizzata: Rust 1.99.1.
- Binari Windows prodotti con target `x86_64-pc-windows-gnu`, profilo release.
- Interfaccia grafica e CLI compilate senza errori e senza avvisi del progetto.
- Sorgenti formattati con rustfmt.
- **11 test Rust superati**: rapporto veraPDF completo e profilo corretto;
  rifiuto di XML malformato, entità esterne, job duplicati, contatori incoerenti
  ed eccezioni; escape PostScript; percorsi Windows; pubblicazione senza
  sovrascrittura; prenotazioni concorrenti; annullamento, timeout e pulizia.
- Log: `docs/qa/rust-tests.txt`.

## Prove complete con i binari Windows inclusi

Ambiente: eseguibili Windows x64 eseguiti tramite **Wine 11** su Linux, con
Ghostscript **10.08.0**, veraPDF **1.30.3** e Temurin **21.0.12.1+1** del bundle.
Queste prove esercitano i binari Windows e le loro dipendenze effettive, ma non
sostituiscono un collaudo su un PC Windows 10/11 reale.

| Verifica | Esito |
| --- | --- |
| Documento di due pagine con testo, tabella, accenti ed euro, nei tre profili | 3 conversioni conformi |
| Documento con vera sovrapposizione semitrasparente, nei tre profili | 3 conversioni conformi |
| Nome e cartelle con accenti, spazi, `&`, `%` e parentesi | Conforme, senza errori di percorso |
| Ripetizione di un nome già presente | Nuovo nome, file esistente conservato |
| PDF cifrato | Rifiutato, nessun PDF finale pubblicato |
| PDF con allegato incorporato, richiesta PDF/A-3b | Rifiutato, nessun allegato perso in silenzio |
| Hash SHA-256 degli originali prima e dopo | Invariati |
| Temporanei e prenotazioni al termine | Rimossi |
| Estrazione di accenti e simbolo euro nel documento di testo | Corretta in 1b, 2b e 3b |
| Avvio interfaccia nativa e individuazione dei componenti | Riusciti |

Totale: **8 conversioni riuscite e 2 rifiuti attesi**. Le conversioni sono
considerate riuscite solo con rapporto veraPDF conforme al profilo richiesto.
Evidenza: `docs/qa/windows-smoke.json`. L'anteprima in `docs/anteprima.png`
proviene dall'applicazione realmente avviata nell'ambiente di prova.

## Prove aggiuntive del nucleo su Linux

Con Ghostscript 10.02.1 di sistema e veraPDF 1.30.3 sono state completate
**13 conversioni riuscite e 4 rifiuti attesi**. Il campione include scansioni,
pagine ruotate, nomi particolari, collisioni, PDF cifrati, allegati, PDF
malformati e un campo firma sintetico popolato. Quest'ultimo esercita il
blocco prudenziale e il consenso esplicito, non la verifica crittografica di
una firma reale. Tutti gli originali sono rimasti invariati.

Nove confronti di rendering della prima pagina, a 108 dpi, hanno conservato
le dimensioni della pagina. La differenza media assoluta dei canali RGB
rispetto agli originali è compresa tra 0 e 0,535 su una scala 0-255. È stato
anche eseguito un controllo visivo dei documenti e della trasparenza.

La vecchia versione Ghostscript 10.02.1 mostra un problema di mappatura
Unicode per accenti/euro nel documento generato da ReportLab. Il problema
non si ripresenta nel pacchetto Windows con Ghostscript 10.08.0, verificato
separatamente. Un PDF/A-1b con trasparenza può essere rasterizzato e perdere
testo ricercabile: è una limitazione descritta nella guida.

Evidenze: `docs/qa/e2e-linux.json` e `docs/qa/visual-checks-linux.json`.

## Ambito del collaudo e riproduzione

Non sono stati eseguiti test su un PC Windows fisico, sulla configurazione
specifica dell'ufficio, su file reali dei clienti o su un corpus esaustivo
di PDF. Il workflow GitHub Actions è incluso ma non è stato pubblicato o
eseguito su un account GitHub.

Per ricompilare: seguire `README.md` e `scripts/build-windows.ps1`. Per
ripetere il test completo della CLI con motori reali:

```text
python tests/make_fixtures.py test-output/fixtures
python tests/test_e2e.py percorso/del/pdfa-office-cli test-output
```

Su Windows il percorso del secondo comando punta a `pdfa-office-cli.exe`.
I test Python richiedono ReportLab, pypdf e Pillow; non sono dipendenze
dell'applicazione installata. Per la GUI e per gli utenti dell'ufficio non
servono Rust, Python, Java installato globalmente o strumenti di sviluppo.
