/** Percentuale di consumo limitata a 0..100; valori non finiti diventano null. */
export function clampPercent(value: number | null | undefined): number | null {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return null;
  }
  return Math.min(100, Math.max(0, value));
}
