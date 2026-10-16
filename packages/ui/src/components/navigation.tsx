import { createContext, useContext } from 'react';

export type TabId = 'accounts' | 'folders' | 'rules' | 'history' | 'tools' | 'settings';

/** Destinazione di una navigazione tra le schede; `account` (numero di cswap) vale per `history`. */
export interface NavTarget {
  tab: TabId;
  account?: number;
}

const NavContext = createContext<((target: NavTarget) => void) | null>(null);

export const NavProvider = NavContext.Provider;

/** Navigazione tra le schede di `SwapDeckApp`; `null` fuori dall'interfaccia a schede (per esempio la dashboard da sola). */
export function useNavigate(): ((target: NavTarget) => void) | null {
  return useContext(NavContext);
}
