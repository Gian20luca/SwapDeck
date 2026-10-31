# SwapDeck: come funziona

SwapDeck tiene d'occhio i limiti di tutti i tuoi account di Claude Code e, se vuoi, cambia account da solo quando quello in uso sta per finire i limiti. Si appoggia a **cswap** (claude-swap), il programma che salva gli account e passa dall'uno all'altro: SwapDeck ne è l'interfaccia grafica.

Tutto resta sul tuo PC: niente server, niente account SwapDeck. Token e password non finiscono mai nei log.

## Le parole da sapere

- **Utenza:** un account Claude registrato in cswap. Una sola alla volta è **attiva**, ed è quella che usano Claude Code, l'estensione di VS Code e tutte le sessioni aperte.
- **Limite delle 5 ore** e **limite dei 7 giorni:** le due finestre di consumo di Claude. Quando una arriva al 100% quell'utenza si ferma fino al reset.
- **Cambio di utenza:** vale per **tutte** le sessioni aperte sul PC, non solo per una finestra.

## Dove la trovi

- **Icona nell'area di notifica** (vicino all'orologio): un anello che si riempie con il consumo dell'utenza attiva. Il tooltip riassume lo stato.
  - Clic sull'icona: apre la finestra principale.
  - Menu dell'icona (tasto destro): **cambio rapido** dell'utenza, mini-widget, mappa attiva, **Esci**.
- **Finestra principale:** chiudendola con la X l'app non si spegne, resta nell'area di notifica. Si spegne solo con «Esci».
- **Mini-widget:** finestrella sempre in primo piano con l'utenza attiva e le sue barre. Si trascina dove vuoi; la freccia la riduce a una striscia, la croce la nasconde.
- **Estensione di VS Code** (facoltativa): la stessa app dentro VS Code, nella barra laterale (icona SwapDeck), più una voce nella barra di stato in basso.

L'app parte da sola quando accedi a Windows (o al desktop Linux) e resta accesa anche con lo schermo spento o il PC bloccato.

## Passo 1: aggiungere le utenze

Al primo avvio non ci sono utenze. Per ciascun account:

1. In Claude Code (terminale o VS Code) scrivi `/login` e accedi con l'account da aggiungere.
2. In SwapDeck premi **Aggiungi account**: registra l'account con cui Claude Code è collegato in quel momento.
3. Ripeti per gli altri account.

«Aggiungi account» serve solo per le utenze **nuove**. Per quelle già registrate c'è «Rifai login» (vedi sotto).

## Passo 2: leggere la dashboard

Una scheda per utenza con:

- **barra 5 ore** e **barra 7 giorni**, con l'ora del reset;
- **ritmo**: quanto stai consumando rispetto a quanto potresti per arrivare al reset senza esaurire;
- **stima di esaurimento**, se al ritmo attuale finiresti prima del reset;
- stato del login (per esempio «login in scadenza»).

Le «i» cerchiate accanto ai titoli spiegano ogni concetto: passaci sopra col mouse.

Altre schede utili:

- **Storico:** grafici dei consumi nel tempo.
- **Chi consuma:** quali progetti, sessioni, agenti o modelli di questo PC hanno consumato di più, sulle 5 ore o sui 7 giorni.

## Cambiare utenza a mano

Dal menu dell'icona (**cambio rapido**) o dalla scheda dell'utenza. Il cambio è immediato e vale per tutte le sessioni.

## Rifai login (utenza scaduta o in scadenza)

Quando il login di un'utenza scade, la scheda mostra **Rifai login**. Va bene anche se l'account è di un'altra persona:

1. Premi **Rifai login**: compare un link con **Copia link**. (Può aprirsi anche una scheda del browser sul tuo PC: chiudila.)
2. Manda il link alla persona titolare dell'account. Lei lo apre, accede e ottiene un **codice**.
3. Lei ti rimanda il codice: incollalo nel campo **Codice** e premi **Conferma**.
4. SwapDeck controlla che l'account sia quello giusto, salva il login nuovo e torna all'utenza che stavi usando.

Hai 5 minuti per incollare il codice; **Annulla** è sempre disponibile. Se la persona accede con un account diverso, non viene salvato niente e torni all'utenza di prima. Nessuna altra utenza viene toccata.

## Il motore: cambio automatico (facoltativo)

Il motore cambia utenza al posto tuo quando quella attiva si avvicina al limite. È **spento** finché non lo accendi tu.

1. Apri la scheda **Regole**.
2. Metti le utenze **in ordine**: la prima è la **principale**, le altre sono le **riserve**.
3. Scegli una **mappa**, cioè un insieme di regole già pronto:
   - **Principale con riserve:** esce dalla principale al 95% delle 5 ore (o al 98% dei 7 giorni), va sulla riserva con il settimanale più basso e torna alla principale dopo il suo reset.
   - **Eco:** esce al 90% e sceglie la riserva il cui limite settimanale si azzera prima.
   - **Sport:** esce al 98% e sceglie la riserva con più margine sulle 5 ore.
   - **Sicura:** sceglie la riserva che arriva al reset senza esaurirsi al ritmo attuale.

   Puoi anche clonare una mappa e cambiarne soglie e criterio.
4. Premi **Accendi**. La prima volta SwapDeck chiede conferma con tre scelte:
   - **Accendi:** cambia davvero.
   - **Solo osservazione:** decide e scrive nello storico cosa avrebbe fatto, ma non cambia niente. Ottimo per i primi giorni.
   - **Annulla.**

La scheda Regole mostra anche **«Adesso andrebbe a»**: l'utenza che il motore sceglierebbe se l'attiva superasse la soglia in questo momento.

### Massimo per utenza

Ogni utenza può avere un **massimo** suo, per esempio per non consumare più dell'80% settimanale dell'account di un collega:

1. Nella lista delle utenze della scheda Regole, premi la **rotellina** ⚙ accanto alla casella.
2. Scrivi la **Soglia sulle 5 ore** e/o la **Soglia sui 7 giorni**. Lasciato vuoto, vale la mappa.
3. Premi **Salva** (o Invio). Nella riga compare per esempio «· max 7d 80%».

Il massimo è un **tetto**: il motore usa il valore più basso tra il massimo dell'utenza e la soglia della mappa. Con la mappa al 95% e l'utenza a 85% si usa l'85%; con la mappa al 50% e l'utenza a 85% si usa il 50%. Vale per tutte le mappe.

Esempio: per usare la tua utenza fino al 100% e quelle dei colleghi fino all'80%, lascia vuota la soglia dei 7 giorni della mappa («solo blocco al 100%») e metti 80 come massimo dei 7 giorni sulle utenze dei colleghi.

Se tutte le utenze sono oltre il proprio massimo, il motore non cambia e ti avvisa con «Nessuna riserva disponibile».

### Da sapere

- Tra due cambi passano almeno 5 minuti, e il motore si fida solo di consumi letti da poco.
- Se trova un altro automatismo che cambia account (per esempio `cswap auto` o uno script pianificato), il motore si mette **«sospeso»** da solo, per non fare cambi in contemporanea. Riparte quando l'altro si ferma. Se sai che va bene così, puoi ignorare quel conflitto dalla scheda Regole.
- App e VS Code non cambiano mai insieme: il motore gira in un posto solo alla volta.

## Notifiche

Nelle impostazioni, pagina **Notifiche**, scegli cosa ti avvisa:

- **Cambi di utenza:** preavviso, cambio fatto, ritorno alla principale, nessuna riserva disponibile.
- **Consumi e reset:** soglie sulle 5 ore e sui 7 giorni (di default 80% e 95%), esaurimento previsto, reset.
- **Stato:** motore sospeso o ripreso, login da rifare o in scadenza, aggiornamenti.

C'è anche **Non disturbare** a orari e un **riepilogo giornaliero**.

## Se qualcosa non va

| Cosa vedi                           | Cosa fare                                                                                            |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Motore «sospeso»                    | C'è un altro automatismo che cambia account: fermalo, oppure ignora il conflitto dalla scheda Regole |
| Il motore non cambia mai            | È spento o in «Solo osservazione», oppure non hai messo l'ordine delle utenze                        |
| «Nessuna riserva disponibile»       | Tutte le utenze sono oltre la soglia o il proprio massimo: aspetta un reset o alza un massimo        |
| Un'utenza con login da rifare       | Usa **Rifai login** sulla sua scheda                                                                 |
| Consumi non aggiornati per un po'   | Normale: cswap li legge al massimo ogni minuto e a volte Claude li limita                            |
| L'estensione di VS Code non compare | Command Palette → «Developer: Reload Window»                                                         |
| Non capisci cosa succede            | Strumenti → Diagnostica → **Copia diagnostica** e mandala a chi ti ha dato SwapDeck                  |

Impostazioni, storico e log stanno in `%APPDATA%\swapdeck` su Windows e in `~/.config/swapdeck` e `~/.local/share/swapdeck` su Linux.
