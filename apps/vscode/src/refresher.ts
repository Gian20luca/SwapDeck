/**
 * Lettura condivisa con contatore di generazione: una lettura partita prima di un'azione non deve
 * sovrascrivere lo stato prodotto dall'azione.
 */
export class Refresher<T> {
  private generation = 0;
  private inflight: Promise<T> | undefined;

  /**
   * @param read lettura (non deve rifiutare: gli errori diventano un valore).
   * @param accept riceve il valore e se è ancora attuale; restituisce lo stato da dare ai chiamanti.
   */
  constructor(
    private readonly read: () => Promise<T>,
    private readonly accept: (value: T, current: boolean) => T,
  ) {}

  /** Una sola lettura alla volta: le chiamate concorrenti condividono la stessa. */
  refresh(): Promise<T> {
    if (this.inflight) return this.inflight;
    const generation = this.generation;
    const p: Promise<T> = this.read()
      .then((value) => this.accept(value, generation === this.generation))
      .finally(() => {
        if (this.inflight === p) this.inflight = undefined;
      });
    this.inflight = p;
    return p;
  }

  /** Dopo un'azione: le letture in corso diventano vecchie e la prossima ne avvia una nuova. */
  invalidate(): void {
    this.generation++;
    this.inflight = undefined;
  }
}
