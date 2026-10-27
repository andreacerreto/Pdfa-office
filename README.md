# PDF/A Office

Convertitore desktop per Windows 10 e Windows 11 a 64 bit. Converte uno o più PDF in **PDF/A-1b, PDF/A-2b oppure PDF/A-3b** e verifica ogni risultato con veraPDF prima di salvarlo come conversione riuscita.

L'interfaccia e il coordinamento delle operazioni sono scritti in **Rust**. La conversione utilizza **Ghostscript** e la validazione utilizza **veraPDF**, eseguiti localmente. Non è un motore di conversione PDF scritto interamente in Rust.

## Utilizzo

1. Esegui `PDF-A-Office-0.1.0-Windows-x64-Setup.exe` del pacchetto distribuito. L'installazione è per l'utente corrente e non richiede privilegi amministrativi.
2. Apri **PDF/A Office** dal menu Start.
3. Aggiungi i PDF, oppure trascinali nella finestra.
4. Scegli il formato richiesto dal destinatario e la cartella in cui salvare i risultati.
5. Premi **Converti e verifica**. Usa **Apri PDF** per controllare la copia prodotta.

Il pacchetto completo contiene Ghostscript **10.08.0**, veraPDF **1.30.3**, il runtime Java Temurin **21.0.12.1+1** e il profilo colore sRGB. Non occorre installare manualmente questi componenti. L'installer completo e l'applicazione operano senza una connessione Internet; i documenti non vengono caricati su servizi online.

Se utilizzi il pacchetto portabile, estrai **tutta** la cartella prima di avviare `pdfa-office.exe`: conserva accanto agli eseguibili la cartella `resources` e gli altri file distribuiti.

L'installer non dispone di una firma Authenticode. Windows o le politiche informatiche dell'ufficio possono quindi segnalarne l'autore come non verificato oppure impedirne l'avvio. Non è necessario disattivare le protezioni: verifica provenienza e checksum e, in un ufficio gestito, concorda l'installazione con il responsabile IT.

## Cosa viene salvato

Esempio: da `pratica.pdf`, scegliendo PDF/A-2b, il programma genera:

| File | Contenuto |
| --- | --- |
| `pratica_pdfa-2b.pdf` | Il PDF convertito e validato. |
| `pratica_pdfa-2b.verapdf.xml` | Il rapporto originale di veraPDF. |
| `pratica_pdfa-2b.ricevuta.json` | Profilo, versioni dei motori, hash SHA-256, percorsi, tempi e avvisi. |

I documenti originali e i risultati già esistenti non vengono sovrascritti. In caso di nomi occupati il programma aggiunge `_1`, `_2` e così via. Se conversione o validazione falliscono, non viene pubblicato un PDF finale; quando possibile viene salvato un file `pratica_pdfa-2b_errore.log`. Se fallisce soltanto la pulizia dei temporanei dopo il salvataggio, il messaggio indica il PDF già validato e la cartella da controllare.

## Scelta del formato e limiti

| Profilo | Indicazione pratica |
| --- | --- |
| PDF/A-1b | Quando richiesto dal destinatario. Può richiedere l'appiattimento delle trasparenze. |
| PDF/A-2b | Impostazione iniziale dell'app; utilizzabile se il destinatario accetta PDF/A-2. Consente le trasparenze. |
| PDF/A-3b | Quando è richiesto espressamente PDF/A-3. In questa versione non serve a mantenere o aggiungere allegati. |

La verifica di conformità è obbligatoria. Non dimostra però l'identità visiva con il documento di partenza: font sostituiti, moduli e trasparenze possono influire sulla copia. Controlla il risultato, soprattutto prima di un deposito o di un invio formale.

I profili di livello **b** non garantiscono l'identità del testo copiato o ricercabile. L'appiattimento di una pagina con trasparenze in PDF/A-1b può rasterizzare anche il testo. Quando ricerca, copia-incolla o estrazione del testo sono importanti, controlla anche queste operazioni sul risultato.

- I PDF firmati sono bloccati per impostazione iniziale. L'opzione dedicata consente di produrre una nuova copia, che **non conserva la validità della firma digitale originale**.
- I PDF cifrati, i moduli XFA e i documenti con allegati incorporati o portfolio sono rifiutati. Il blocco degli allegati vale anche per PDF/A-3b.
- Se una fattura contiene un XML incorporato, selezionare PDF/A-3b non ne assicura il trasferimento: quel documento viene rifiutato.
- Non è incluso un motore OCR. La conversione di una scansione non aggiunge automaticamente testo ricercabile.

La [guida operativa](docs/USO.md) descrive installazione, uso e risoluzione degli errori. L'[architettura](docs/ARCHITETTURA.md) documenta il procedimento e le sue garanzie.

## Riga di comando

Dalla cartella dell'applicazione, in PowerShell:

```powershell
.\pdfa-office-cli.exe doctor
.\pdfa-office-cli.exe convert -f 2b -o "C:\Lavoro\PDF-A" "C:\Lavoro\pratica.pdf"
.\pdfa-office-cli.exe convert -f 1b -o "C:\Lavoro\PDF-A" --timeout-seconds 900 --json "C:\Lavoro\a.pdf" "C:\Lavoro\b.pdf"
```

`--allow-signed-copy` abilita esplicitamente la creazione di copie di PDF firmati. La CLI usa un limite iniziale di **300 secondi per processo esterno**, configurabile fra **10 e 7200 secondi**. L'interfaccia grafica usa **600 secondi per processo esterno**. Non si tratta del tempo massimo dell'intero lotto.

## Compilazione e installer

Prerequisiti sul computer di sviluppo Windows x64:

- Rust con toolchain MSVC e Cargo, installabili da [rustup.rs](https://rustup.rs/).
- Visual Studio Build Tools con strumenti C++ e Windows SDK.
- [NSIS 3](https://nsis.sourceforge.io/Download) e [7-Zip](https://www.7-zip.org/).
- PowerShell e accesso Internet per scaricare crate Rust e dipendenze durante la preparazione.

Apri PowerShell nella cartella del progetto ed esegui:

```powershell
.\scripts\build-windows.ps1
```

Lo script esegue i test Rust, compila gli eseguibili, prepara i componenti e crea in `dist` l'installer, il pacchetto portabile e `SHA256SUMS.txt`. I percorsi di NSIS e 7-Zip possono essere specificati con `-MakeNsis` e `-SevenZip`. `-SkipDependencies` riutilizza componenti già preparati in `build\package`.

Le versioni, gli URL e gli hash SHA-256 attesi sono fissati in `scripts/dependencies.json`; il download fallisce se un archivio non corrisponde al suo hash. `Cargo.lock` fissa le dipendenze Rust. Il workflow `.github/workflows/windows.yml` permette la stessa preparazione su un runner Windows; la sua presenza non implica che sia già stato eseguito.

## Licenze

Il codice dell'applicazione è distribuito con licenza **AGPL-3.0-or-later**; il progetto sorgente contiene interfaccia, motore di coordinamento, test e script di packaging. Ghostscript ha una propria licenza AGPL/commerciale; veraPDF usa il modello duale GPL-3.0-or-later/MPL-2.0; il runtime Java Temurin è soggetto alla GPLv2 con Classpath Exception e ai relativi avvisi.

Consulta `LICENSE`, `THIRD-PARTY-NOTICES.md` e gli avvisi inclusi nei componenti prima di ridistribuire o integrare il pacchetto in altri prodotti.
