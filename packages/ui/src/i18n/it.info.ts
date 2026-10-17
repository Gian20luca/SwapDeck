/** Testi delle «i» (InfoTip): spiegano il perché di un concetto, senza ripetere l'etichetta. */
export const itInfo = {
  'info.label': 'Informazioni su {name}',

  'info.dashboard':
    'Le utenze Claude gestite da cswap, con il consumo di ciascuna. L’utenza attiva è quella che Claude Code usa adesso.',
  'info.usage.fiveHour':
    'Quanto hai consumato nella finestra di 5 ore. Quando arriva al 100% Claude si ferma su questa utenza fino al reset, indicato sotto la barra.',
  'info.usage.sevenDay':
    'Quanto hai consumato nel limite settimanale. Il tratteggio è il ritmo uniforme: dove saresti se consumassi allo stesso passo fino al reset.',
  'info.pace':
    'Confronta il consumo della settimana con un ritmo uniforme. Se sei in anticipo, il limite può finire prima del reset.',
  'info.usage.scoped':
    'Alcuni modelli hanno un tetto settimanale a parte, oltre ai limiti generali. Ognuno ha il suo reset.',

  'info.detail.name':
    'L’alias è un nome facile da ricordare al posto dell’email: lo usi nell’app e nei comandi di cswap.',
  'info.detail.use':
    'Disabilitare toglie l’utenza dalla scelta, anche del motore, senza cancellarla. Il terminale parte con questa utenza senza cambiare quella attiva.',
  'info.detail.slot':
    'Lo slot è il numero dell’utenza in cswap e ne decide l’ordine. Spostare o scambiare cambia solo i numeri, non le credenziali.',
  'info.detail.danger': 'Rimuovere toglie l’utenza da cswap. Per riaverla devi rifare il login.',

  'info.folders.list':
    'Una cartella mappata usa sempre l’utenza indicata, anche se quella attiva è un’altra. Utile per tenere un progetto su un’utenza precisa.',
  'info.folders.add':
    'Scegli l’utenza e la cartella del progetto: da lì in poi Claude Code, aperto con cswap, usa quell’utenza.',

  'info.history.range':
    'Il periodo mostrato nei grafici e nell’elenco dei cambi e delle decisioni.',
  'info.history.charts':
    'La linea è il consumo reale; il tratteggio è il ritmo uniforme fino al reset. Se la linea sta sopra, stai consumando più in fretta del previsto.',
  'info.history.events':
    'Un «Cambio» è un passaggio di utenza avvenuto, a mano o dal motore. Una «Decisione» è ciò che il motore ha valutato, anche quando ha scelto di non cambiare.',

  'info.tools.export':
    'Salva le utenze in un file, per fare una copia o spostarle su un altro computer. Il file contiene le credenziali in chiaro: conservalo con cura.',
  'info.tools.import':
    'Ricarica le utenze da un file esportato prima. Serve dopo una reinstallazione o su un nuovo computer.',
  'info.tools.import.force':
    'Se un’utenza è già presente nello stesso slot, il file la sostituisce. Senza questa scelta viene saltata.',
  'info.tools.unclaimed':
    'Credenziali che cswap ha messo da parte senza un’utenza collegata. Puoi eliminarle una a una per fare pulizia.',
  'info.tools.purge':
    'Cancella tutto ciò che cswap ha salvato (utenze, credenziali, profili di sessione). Non tocca il login corrente di Claude Code e non si può annullare.',

  'info.settings.app':
    'Come si presenta SwapDeck e ogni quanto legge i consumi. Valgono solo per questa app, non per cswap.',
  'info.settings.language':
    'La lingua dei testi di SwapDeck. «Automatica» segue la lingua del sistema.',
  'info.settings.theme': 'Aspetto chiaro o scuro. «Automatico» segue il tema del sistema.',
  'info.settings.interval':
    'Ogni quanti secondi SwapDeck rilegge i consumi. Con valori bassi i dati sono più freschi, ma si fanno più letture.',
  'info.settings.thresholds':
    'Quando il consumo dell’utenza attiva supera una di queste percentuali, ricevi una notifica.',
  'info.settings.notif':
    'Scegli quali avvisi ricevere e in quali orari. Se non li vuoi tutti, qui li spegni uno per uno.',
  'info.settings.autostart':
    'SwapDeck parte da solo quando accedi al computer, così il monitoraggio e il motore restano sempre attivi.',
  'info.config':
    'Valori letti dal cambio automatico di cswap («cswap auto»), non da SwapDeck. Dove non c’è un valore impostato vale quello di default.',
  'info.config.threshold':
    'Percentuale di consumo dell’utenza attiva oltre la quale cswap passa a un’altra. Più è bassa, più i cambi sono anticipati.',
  'info.config.intervalSeconds':
    'Ogni quanti secondi cswap controlla i consumi per decidere se cambiare.',
  'info.config.cooldownSeconds':
    'Pausa minima dopo un cambio prima di poterne fare un altro. Evita passaggi a raffica.',
  'info.config.hysteresisPct':
    'Margine, in punti percentuali sotto la soglia, che evita i cambi avanti e indietro tra utenze vicine alla soglia. Valori più alti rendono il cambio più stabile.',
  'info.config.strategy':
    '«best» sceglie l’utenza con più margine libero. «consume-first» consuma prima quella il cui limite si azzera per prima, così nulla va sprecato.',
  'info.config.includeApiKeyAccounts':
    'Le utenze con chiave API si pagano a consumo. Per default il cambio automatico non le usa come destinazione: attivalo solo se vuoi che lo faccia.',
  'info.config.unhealthyTicks':
    'Quanti giri di controllo consecutivi un’utenza deve risultare in errore prima di essere messa in quarantena, cioè esclusa dalla scelta. Evita di scartarla per un errore passeggero.',
  'info.config.model':
    'Quali limiti per modello (per esempio Fable) contano nella scelta. Nomi separati da virgola, oppure «all» per tutti; se è vuoto non conta nessun modello.',

  'info.notif.engine':
    'Avvisi sul motore: quando sta per cambiare utenza, quando l’ha fatto e quando non ha più dove andare.',
  'info.notif.usage':
    'Avvisi sul consumo delle utenze e sui loro reset, per accorgerti in tempo che un limite si avvicina.',
  'info.notif.state':
    'Avvisi sullo stato dell’app e di cswap: motore sospeso, login da rifare, dati vecchi, nuove versioni.',
  'info.notif.quiet':
    'Una fascia oraria senza notifiche, per esempio la notte. Gli avvisi bloccati si scartano e non arrivano dopo.',
  'info.notif.summary':
    'Una sola notifica al giorno con il quadro dei consumi e dei cambi, per chi non vuole avvisi continui.',
  'info.notif.switchPreview':
    'Ti dà il tempo di salvare il lavoro prima che il motore cambi utenza. Si attiva quando manca poco alla soglia, in punti o in minuti.',
  'info.notif.noReserve':
    'Succede quando tutte le utenze sono al limite: Claude si ferma finché una non si azzera. È l’avviso più urgente.',
  'info.notif.percent':
    'Un avviso ogni volta che il consumo supera una soglia, per esempio 80 e 95. Le soglie sono indipendenti per le 5 ore e per i 7 giorni.',
  'info.notif.exhaustion':
    'Stima quando finirà il limite settimanale se continui a questo ritmo, e ti avvisa se cade prima del reset.',
  'info.notif.modelLimit':
    'Gli stessi avvisi a soglia, ma per i limiti propri di un modello, che si esauriscono a parte.',
  'info.notif.engineState':
    'Il motore si ferma da solo in certi casi, per esempio se un altro motore è già attivo. L’avviso ti dice quando si ferma e quando riparte.',
  'info.notif.login':
    'Un’utenza con il login scaduto non funziona più finché non rifai l’accesso con /login. L’avviso arriva prima della scadenza.',
  'info.notif.stale':
    'Se i consumi non si aggiornano più, le barre mostrano numeri vecchi e il motore decide senza dati freschi.',
  'info.notif.quietCritical':
    'Gli avvisi critici (nessuna riserva, login da rifare) richiedono un’azione: se vuoi non perderli, lasciali passare anche di notte.',
  'info.rules.state':
    'Acceso: il motore cambia utenza da solo, per tutte le sessioni. Solo osservazione: decide e lo scrive nello storico, ma non cambia. Spento: non valuta nulla. Sospeso: si è fermato da solo (altro motore, regole non valide) e riparte quando il problema sparisce.',
  'info.rules.next':
    'L’utenza a cui il motore passerebbe se l’attiva arrivasse alla soglia adesso, con il motivo. Serve a provare le regole senza aspettare.',
  'info.maps.map':
    'Una mappa è un pacchetto con nome: lista delle utenze, soglie e criterio. La scegli tu, il motore non la cambia mai da solo. Acceso, osservazione e conflitti ignorati valgono per tutte le mappe.',
  'info.maps.preset':
    'I preset ci sono sempre e non si modificano. «Clona come mappa personale» ne fa una copia con un nome, che puoi modificare, rinominare ed eliminare. Un preset usa la lista della mappa da cui lo scegli.',
  'info.maps.box':
    'Le regole della mappa attiva: chi usare (lista), quando cambiare (soglie) e a chi passare (criterio). Valgono dal giro dopo il salvataggio.',
  'info.rules.list':
    'Chi può usare il motore e in che ordine. La prima è la principale: il motore parte da lì e, con «Torna al principale», ci torna dopo il suo reset. Le utenze escluse non le sceglie mai.',
  'info.rules.thresholds':
    'Quando cambiare: oltre la soglia delle 5 ore o dei 7 giorni l’utenza attiva lascia il posto. Dopo il reset si torna alla principale solo se è sotto la soglia di almeno l’isteresi; la pausa minima evita cambi a raffica.',
  'info.rules.criterion':
    'A chi passare tra le riserve sotto soglia. Settimanale più bassa; Migliore: più margine; Ordine fisso: la prima della lista; Reset più vicino; Ritmo sicuro: chi arriva al reset; 5 ore più libera; Bilanciato: pesi; Livellate; Margine sul modello.',
  'info.rules.weights':
    'Quanto conta ogni componente nel punteggio bilanciato: margine sulle 5 ore e sui 7 giorni, vicinanza del reset settimanale, ritmo. Contano le proporzioni; un fattore senza dati si toglie.',
  'info.rules.model':
    'Il limite settimanale proprio di un modello, per esempio Fable: il motore sceglie la riserva con più margine su quel limite e, a parità, la settimanale più bassa.',
  'info.rules.conflicts':
    'Un altro automatismo che cambia utenza con cswap (per esempio cswap auto) può contraddire questo motore: se è certo, SwapDeck si sospende finché c’è. «Ignora» lo fa andare avanti lo stesso.',

  'info.usage.what':
    'Il consumo di Claude Code su questo PC, letto dalle sessioni salvate: per progetto, sessione, agente o modello, attribuito all’utenza attiva in quel momento. Si misura in token equivalenti, pesati come i prezzi delle API.',
  'info.usage.other':
    'Quanto consumano con la stessa utenza gli altri dispositivi (altri PC, web): la quota letta dal server meno quella spiegata dai token di questo PC. La fiducia sale con il numero e la coerenza delle letture; servono almeno 3 intervalli.',
  'info.usage.privacy':
    'Resta tutto su questo PC, in un indice locale con orari, modelli e conteggi dei token. Il testo dei messaggi non si legge, il percorso delle cartelle non va nei log e l’app mostra solo il nome dell’ultima cartella.',
  'info.usage.firstScan':
    'La prima volta SwapDeck legge le sessioni degli ultimi 30 giorni e ne fa un indice; poi legge solo le righe nuove. Se cambi scheda, la lettura riprende da dove era arrivata.',
} as const;
