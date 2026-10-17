/** Testi italiani della scheda «Chi consuma» (F20). */
export const itUsage = {
  'tabs.usage': 'Chi consuma',
  'tabs.usage.tip':
    'Chi consuma la quota: progetti, sessioni e agenti di Claude Code su questo PC.',
  'usage.title': 'Chi consuma',
  'usage.account': 'Utenza',
  'usage.account.all': 'Tutte le utenze',
  'usage.account.tip': 'Mostra i consumi di una sola utenza o di tutte.',
  'usage.window': 'Finestra',
  'usage.window.5h': '5 ore',
  'usage.window.7d': '7 giorni',
  'usage.window.tip':
    'Finestra dei limiti di Claude: con un’utenza scelta è quella in corso, altrimenti le ultime ore o giorni.',
  'usage.groupBy': 'Raggruppa per',
  'usage.group.project': 'Progetto',
  'usage.group.session': 'Sessione',
  'usage.group.agent': 'Agente',
  'usage.group.model': 'Modello',
  'usage.group.tip': 'Come raggruppare i consumi nella classifica.',
  'usage.firstScan': 'Prima analisi delle sessioni in corso…',
  'usage.firstScan.progress': 'Letto il {pct}% delle sessioni',
  'usage.firstScan.hint':
    'La prima lettura può richiedere qualche decina di secondi; le successive sono immediate.',
  'usage.noSource': 'Non trovo le sessioni di Claude Code su questo PC.',
  'usage.empty': 'Nessun consumo registrato in questa finestra.',
  'usage.ranking': 'Classifica dei consumi',
  'usage.total': 'Su questo PC: {tokens} token equivalenti',
  'usage.range': 'dal {from} al {to}',
  'usage.row.foot': '{tokens} token eq. · {messages} messaggi · ultimo uso {when}',
  'usage.row.bar': '{pct}% del consumo',
  'usage.main': 'Conversazione principale',
  'usage.truncated': 'Altre {count} righe non sono mostrate.',
  'usage.unknown.title': 'Utenza sconosciuta',
  'usage.unknown.text':
    '{tokens} token equivalenti di quando non si sapeva quale utenza fosse attiva (SwapDeck spento): non attribuiti a nessuna utenza.',
  'usage.other.title': 'Altri dispositivi (stima)',
  'usage.other.unavailable': 'Stima non ancora disponibile',
  'usage.other.unavailableHint':
    'Servono più letture del consumo con questa utenza per calibrarla.',
  'usage.other.noData':
    'Stima non ancora disponibile: nessuna lettura del server in questa finestra.',
  'usage.other.value': '≈ {pct}% della quota, circa {tokens} token eq.',
  'usage.other.detail': 'Server {server}% · questo PC {local}% · altri dispositivi ≈ {other}%',
  'usage.other.bar': 'Altri dispositivi: {pct}%',
  'usage.other.gaps': 'Storico incompleto: la stima è poco affidabile.',
  'usage.confidence.low': 'Fiducia bassa',
  'usage.confidence.medium': 'Fiducia media',
  'usage.confidence.high': 'Fiducia alta',
  'usage.confidence.tip':
    'Metodo: confronta la quota letta dal server con i token di questo PC e dà agli altri dispositivi la differenza; la fiducia cresce con il numero e la coerenza delle letture.',
  'usage.scan.indexReset': 'L’indice delle letture è stato rifatto da capo.',
  'usage.scan.saveFailed': 'Indice non salvato: la prossima analisi ripartirà dalla precedente.',
  'usage.scan.partial':
    'Lettura delle sessioni non completata: la classifica comprende solo la parte già letta.',
  'usage.scan.incomplete':
    'Alcune cartelle delle sessioni non sono leggibili: i dati possono essere parziali.',
  'usage.privacy':
    'Si leggono solo i conteggi dei token e le descrizioni degli agenti delle sessioni di Claude Code su questo PC; il testo dei messaggi non viene letto e nulla esce dal PC.',
  'usage.link': 'Chi consuma',
  'usage.linkLabel': 'Chi consuma questa utenza: {name}',
  'usage.link.tip': 'Apre la scheda Chi consuma filtrata su questa utenza.',
  'usage.linkFull': 'Chi consuma questa utenza',
} as const;
