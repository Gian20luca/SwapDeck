/** Nome dell'eseguibile di SwapDeck per la piattaforma indicata. */
export function swapdeckExecutable(platform: NodeJS.Platform): string {
  return platform === 'win32' ? 'swapdeck.exe' : 'swapdeck';
}
