# Architettura e criteri di conversione

## Componenti

PDF/A Office separa l'esperienza desktop dal motore PDF e dal controllo di conformità:

| Componente | Ruolo |
| --- | --- |
| Rust + eframe/egui | Interfaccia nativa, coda, avanzamento, annullamento e persistenza delle preferenze. |
| Libreria Rust del progetto | Controlli preliminari, gestione dei processi e dei temporanei, interpretazione dei rapporti e pubblicazione dei risultati. |
| lopdf | Lettura strutturale del PDF per i controlli preliminari e il confronto del numero di pagine. |
| Ghostscript 10.08.0 | Ricostruzione del documento con `pdfwrite`, profilo colore ICC e modalità PDF/A. |
| veraPDF 1.30.3 | Verifica indipendente del profilo 1b, 2b o 3b richiesto. |
| Temurin Java 21.0.12.1+1 | Runtime privato per la CLI veraPDF. |
| NSIS | Installer Windows x64 per l'utente corrente e disinstallatore. |

L'applicazione non utilizza un backend remoto, API a consumo o un browser incorporato. Rust avvia gli eseguibili locali tramite argomenti strutturati; non costruisce comandi da eseguire attraverso una shell. Il runtime Java viene distribuito privatamente nella cartella dell'app, senza impostare un Java di sistema.

## Procedimento per ogni file

1. Il programma crea una directory temporanea nella destinazione e vi copia l'input con un nome semplice. Le successive operazioni utilizzano questa copia.
2. lopdf controlla che esistano pagine e individua cifratura, firme, allegati incorporati, portfolio e moduli XFA. Cifratura, allegati e XFA comportano il rifiuto. La firma richiede un consenso esplicito alla creazione di una nuova copia.
3. Il profilo sRGB viene copiato nella directory temporanea e verificato come ICC RGB. Un file PostScript di definizione imposta l'OutputIntent.
4. Ghostscript converte con `-dPDFA=1`, `2` o `3` e `-dPDFACompatibilityPolicy=2`. Quest'ultima opzione richiede l'interruzione quando il motore incontra determinate incompatibilità con il formato selezionato.
5. Il risultato viene riaperto e controllato: deve essere leggibile, non cifrato e avere lo stesso numero di pagine dell'input.
6. veraPDF viene eseguito con il profilo esatto, per esempio `--flavour 2b --format xml`. L'app interpreta il rapporto e richiede un esito coerente e positivo.
7. Vengono calcolati gli hash SHA-256 e preparata la ricevuta JSON. Il programma riserva nomi liberi, salva rapporto e ricevuta, quindi pubblica il PDF validato con un'operazione che non sovrascrive file esistenti.

Le opzioni di Ghostscript includono `SAFER`, incorporamento dei font, assenza di downsampling delle immagini e disattivazione della rotazione automatica. Non viene applicato un preset generale come `/screen` o `/ebook`. Per le versioni compatibili e per PDF/A-2/3 viene utilizzata la conversione gestita delle trasparenze. I percorsi inseriti nel PostScript sono escapati e l'accesso al profilo ICC è autorizzato esplicitamente.

La documentazione ufficiale di Ghostscript conferma la produzione delle parti PDF/A 1-3 al livello b e descrive le opzioni impiegate: [High Level Devices](https://ghostscript.readthedocs.io/en/latest/VectorDevices.html) e [Using Ghostscript](https://ghostscript.readthedocs.io/en/latest/Use.html).

## Validazione rigorosa

Un codice di uscita positivo del convertitore non basta a dichiarare conforme il documento. Il parser del rapporto veraPDF richiede:

- XML valido, struttura prevista ed esattamente un documento e un rapporto di validazione;
- nome del profilo corrispondente al profilo richiesto;
- `isCompliant="true"`, zero regole/controlli falliti e controlli superati effettivamente presenti;
- riepilogo del lotto coerente, senza errori di parsing, cifratura, memoria o eccezioni;
- versione del validatore presente e riferimento al file convertito;
- conclusione del processo veraPDF con successo.

Un rapporto mancante, incompleto, malformato o riferito a un altro profilo viene rifiutato. I formati e le opzioni della CLI sono documentati da veraPDF nelle pagine [CLI Validation](https://docs.verapdf.org/cli/validation/) e [CLI Help](https://docs.verapdf.org/cli/help/).

La conformità PDF/A e la fedeltà visiva sono controlli differenti. Ghostscript ricostruisce il documento: font, moduli, annotazioni e contenuti non grafici possono cambiare. La validazione non certifica l'identità del contenuto con l'originale e non preserva la validità delle sue firme. Per questo l'interfaccia invita a controllare anche il PDF prodotto e rifiuta gli allegati che questa versione non sa conservare.

## Processi, annullamento e file

La GUI elabora un documento alla volta in un worker Rust e riceve gli aggiornamenti mediante un canale. L'utente può interrompere il lotto; se chiude la finestra durante il lavoro, la chiusura viene sospesa fino alla conclusione del worker dopo la richiesta di annullamento.

I processi esterni vengono monitorati per timeout e annullamento. La GUI usa 600 secondi per processo; la CLI usa inizialmente 300 secondi e accetta valori da 10 a 7200. Non è un limite complessivo del lotto. stdout e stderr sono separati e scritti su file temporanei, evitando blocchi dovuti a pipe non consumate. Sono previsti limiti alla dimensione dei log e dei rapporti. I processi Windows non aprono finestre console aggiuntive.

L'originale non viene riscritto. I nomi vengono prenotati con creazione esclusiva e i file finali sono pubblicati senza sovrascrittura. Se la pubblicazione del gruppo fallisce, il programma tenta di rimuovere i file parziali appena prodotti. Un arresto forzato del sistema può comunque lasciare temporanei o rapporti intermedi: la pubblicazione di tre file distinti non costituisce una singola transazione del filesystem.

Le normali conclusioni eliminano la directory temporanea. Gli errori conservano, quando possibile, un log diagnostico nella destinazione. Le ricevute riportano hash e dati del procedimento; non sostituiscono firma digitale, marca temporale o conservazione a norma.

## Percorsi e sviluppo

Il layout del pacchetto è:

```text
pdfa-office.exe
pdfa-office-cli.exe
resources/
  ghostscript/bin/gswin64c.exe
  ghostscript/...
  java/bin/java.exe
  java/...
  verapdf/verapdf-cli.jar
  profiles/srgb.icc
  dependencies.json
```

La ricerca dei componenti considera una cartella esplicita, `PDFA_OFFICE_RESOURCES`, i percorsi relativi all'eseguibile e i fallback disponibili. Per sviluppo e verifiche sono disponibili override dedicati: `PDFA_OFFICE_GS`, `PDFA_OFFICE_JAVA`, `PDFA_OFFICE_VERAPDF_JAR`, `PDFA_OFFICE_ICC`. La distribuzione completa usa i componenti inclusi; non richiede queste variabili.

`src/model.rs` contiene i tipi condivisi; `src/engine.rs` implementa la pipeline; `src/ui.rs` gestisce l'interfaccia; i due file sotto `src/bin` sono gli ingressi grafico e CLI. `scripts/build-windows.ps1` prepara la distribuzione Windows, mentre `scripts/prepare-dependencies.ps1` scarica i componenti fissati in `scripts/dependencies.json` e ne verifica gli hash SHA-256. La preparazione richiede rete; il funzionamento dell'applicazione no.

## Licenze e distribuzione

L'applicazione è AGPL-3.0-or-later. Ghostscript segue il modello AGPL/commerciale di Artifex; veraPDF è disponibile sotto GPL-3.0-or-later o MPL-2.0; Temurin/OpenJDK usa GPLv2 con Classpath Exception e avvisi aggiuntivi. I componenti conservano licenze distinte anche se inclusi nello stesso installer.

Riferimenti primari: [Artifex Licensing](https://artifex.com/licensing), [repository veraPDF-apps](https://github.com/veraPDF/veraPDF-apps), [Eclipse Adoptium](https://adoptium.net/). I testi e gli avvisi del pacchetto sono indicati in `LICENSE` e `THIRD-PARTY-NOTICES.md`. Le versioni effettivamente distribuite e i rispettivi riferimenti di provenienza sono fissati nel manifesto delle dipendenze.

Questa documentazione descrive l'implementazione e non costituisce un verbale di collaudo. Gli esiti delle verifiche eseguite sono documentati separatamente.
