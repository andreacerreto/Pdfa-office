# Guida operativa

## Installazione

PDF/A Office è destinato a Windows 10/11 a 64 bit. L'installer completo include tutti i motori e il runtime Java; non servono installazioni separate di Ghostscript o veraPDF.

Avvia `PDF-A-Office-0.1.0-Windows-x64-Setup.exe`, leggi la licenza e segui la procedura. La destinazione iniziale è `%LOCALAPPDATA%\Programs\PDF-A Office`. L'installazione riguarda il tuo utente e aggiunge un collegamento nel menu Start. Se scegli una cartella diversa, usa una cartella vuota dedicata all'applicazione.

Il pacchetto non è firmato con Authenticode: l'autore potrebbe essere indicato come non verificato. Se le regole dell'ufficio impediscono l'avvio, fai verificare il pacchetto dal responsabile IT. Gli hash dei file distribuiti sono in `SHA256SUMS.txt`; da PowerShell puoi calcolare quello dell'installer con:

```powershell
Get-FileHash .\PDF-A-Office-0.1.0-Windows-x64-Setup.exe -Algorithm SHA256
```

Il pacchetto portabile funziona estraendone tutto il contenuto in una cartella e aprendo `pdfa-office.exe`. Non avviare l'eseguibile direttamente dall'anteprima dello ZIP e non separarlo dalla cartella `resources`.

Per disinstallare usa la voce PDF/A Office nelle applicazioni di Windows oppure il collegamento **Disinstalla** nel menu Start. Conserva i tuoi documenti in una cartella di lavoro distinta dalle cartelle interne dell'applicazione.

## Convertire un lotto

1. Premi **Aggiungi PDF...**, seleziona uno o più file, oppure trascinali nella finestra. Il programma evita di aggiungere due volte lo stesso file alla coda.
2. Scegli **PDF/A-1b**, **PDF/A-2b** o **PDF/A-3b** in base alle istruzioni del destinatario. L'app propone inizialmente 2b; non decide quale profilo sia richiesto per una specifica pratica.
3. Premi **Scegli cartella...**. La cartella e il formato scelti vengono ricordati per l'avvio successivo.
4. Premi **Converti e verifica**. I file vengono elaborati in sequenza e la finestra rimane utilizzabile.
5. Attendi l'esito di ogni documento. **Conforme** compare soltanto dopo la validazione veraPDF sul profilo selezionato.
6. Premi **Apri PDF** e confronta la copia con l'originale. Espandi **Risultato e rapporto** per vedere gli avvisi e aprire il rapporto XML o la ricevuta JSON.

Durante l'elaborazione non è possibile cambiare i file o le opzioni del lotto. **Interrompi** annulla il lavoro in corso: i risultati già completati restano salvati e i file rimanenti possono essere ripresi. Anche la chiusura della finestra richiede prima l'interruzione del worker.

**Rimuovi completati** pulisce la coda senza cancellare i risultati dal disco. **Svuota coda** rimuove soltanto gli elementi visualizzati. I documenti non completati possono essere elaborati nuovamente con **Converti e verifica**, dopo aver risolto l'errore o cambiato le opzioni.

## File prodotti

Per `documento.pdf`, con profilo 1b:

```text
documento_pdfa-1b.pdf
documento_pdfa-1b.verapdf.xml
documento_pdfa-1b.ricevuta.json
```

Se uno dei tre nomi è già occupato, viene scelto un nuovo gruppo con suffisso progressivo, per esempio `documento_pdfa-1b_1.pdf`. I caratteri non utilizzabili nei nomi Windows vengono sostituiti e i nomi troppo lunghi vengono abbreviati.

Il rapporto XML contiene l'esito indipendente di veraPDF. La ricevuta JSON registra anche gli hash SHA-256 del PDF originale, del risultato e del rapporto, le versioni software, il numero di pagine e gli avvisi. Questi file documentano il controllo effettuato; non costituiscono una firma digitale, una marca temporale o un servizio di conservazione a norma.

Se la conversione o la validazione falliscono, il PDF finale non viene pubblicato. Se la cartella è scrivibile, viene conservato un log come `documento_pdfa-1b_errore.log`. Se il problema riguarda soltanto la pulizia dei temporanei dopo il salvataggio, il messaggio indica il PDF già validato e la cartella temporanea da controllare. Un annullamento richiesto dall'utente non produce quel log di errore. I rapporti possono contenere i percorsi dei documenti e messaggi diagnostici: valuta questi contenuti prima di condividerli con altri.

## Documenti particolari

| Situazione | Comportamento e intervento |
| --- | --- |
| PDF firmato o con marca temporale | Bloccato inizialmente. Se vuoi una nuova copia priva della validità della firma originale, attiva la casella dedicata. Il consenso non viene ricordato all'avvio successivo. |
| Allegati incorporati o portfolio | Rifiutato, anche selezionando 3b. Estrai gli allegati e prepara una copia senza allegati, se appropriato al tuo procedimento. |
| Fattura con XML incorporato | Il programma non preserva automaticamente l'XML e non costruisce fatture ibride. Il documento con allegati viene rifiutato. |
| PDF cifrato | Serve prima una copia non cifrata ottenuta con le autorizzazioni necessarie. L'app non richiede o rimuove password. |
| Modulo XFA | Non supportato. Esporta una copia PDF statica dal software che gestisce il modulo. |
| Modulo PDF ordinario | Può essere appiattito. Verifica che tutti i valori compilati siano visibili nel risultato. |
| Scansione | Viene convertita come documento grafico; non viene eseguito OCR. |
| Font sostituito | La conformità può essere superata anche con una resa diversa. Controlla simboli, importi e impaginazione. |
| Trasparenze con 1b | Possono essere appiattite o rasterizzate. Usa il formato richiesto e controlla la resa finale. |

## Errori più comuni

**Componenti di conversione non disponibili.** Reinstalla il pacchetto completo o ripristina la cartella `resources`. Se avevi scelto manualmente dei componenti, usa **Usa componenti inclusi**. **Indica cartella resources...** è utile per una distribuzione portabile o per lo sviluppo.

**Destinazione non scrivibile.** Scegli una cartella per cui hai permessi di scrittura e spazio libero. La conversione crea temporaneamente copie dei documenti nella cartella scelta.

**Limite di tempo superato.** L'interfaccia consente 600 secondi a ciascun processo esterno. Per file particolarmente complessi puoi utilizzare la CLI con `--timeout-seconds`, fino a 7200 secondi.

**Validazione non superata.** Apri i dettagli e il log. Il file non viene considerato conforme. Può essere necessario esportare nuovamente il PDF dal software di origine, sistemare i font o preparare una copia statica del documento. Non è sufficiente rinominare un file per renderlo PDF/A.

## Comandi utili

Dalla cartella che contiene gli eseguibili:

```powershell
# Controlla dove vengono individuati i componenti.
.\pdfa-office-cli.exe doctor

# Converte uno o più PDF.
.\pdfa-office-cli.exe convert -f 2b -o "C:\Pratiche\Convertiti" "C:\Pratiche\uno.pdf" "C:\Pratiche\due.pdf"

# Aumenta il tempo massimo per ogni processo e restituisce un riepilogo JSON.
.\pdfa-office-cli.exe convert -f 3b -o "C:\Pratiche\Convertiti" --timeout-seconds 1200 --json "C:\Pratiche\uno.pdf"

# Usa una cartella di componenti esplicita.
.\pdfa-office-cli.exe --resources "D:\PDF-A-Office\resources" doctor
```

La CLI usa 300 secondi per processo come valore iniziale. Accetta limiti da 10 a 7200 secondi. Con `--json` il riepilogo è scritto su stdout e i progressi su stderr. Il codice di uscita è 0 per un lotto completato, 1 se almeno un documento fallisce e 2 per un errore di avvio/configurazione; anche gli errori di sintassi degli argomenti restituiscono un codice non nullo.
