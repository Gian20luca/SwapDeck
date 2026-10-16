import {
  cloneElement,
  createContext,
  isValidElement,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactElement,
} from 'react';

/** Dentro un tooltip non se ne apre un altro. */
const InsideTooltip = createContext(false);

const MARGIN = 8;

function focusVisible(el: EventTarget): boolean {
  try {
    return (el as Element).matches(':focus-visible');
  } catch {
    return true;
  }
}
const OPEN_DELAY_MS = 350;

export interface TooltipProps {
  /** Testo breve: cosa fa e cosa succede. */
  text: string;
  /** Un solo elemento (pulsante, campo, interruttore, voce): riceve `aria-describedby`. */
  children: ReactElement<{ 'aria-describedby'?: string }>;
  /** Avvolge l'elemento in un blocco invece che in linea (righe intere, campi a tutta larghezza). */
  block?: boolean;
}

/**
 * Tooltip accessibile: si apre al passaggio del mouse (con un breve ritardo) e subito al focus da tastiera, si
 * chiude con Esc, all'uscita e alla perdita del focus. Il testo resta nel DOM (`role="tooltip"`,
 * nascosto) così `aria-describedby` lo legge anche quando non si vede. La bolla è `position:
 * fixed`, tenuta dentro la finestra: niente scroll orizzontale nemmeno a 420 px. Annidato in un
 * altro tooltip mostra solo l'elemento.
 */
export function Tooltip({ text, children, block = false }: TooltipProps) {
  const nested = useContext(InsideTooltip);
  const id = useId();
  const [open, setOpen] = useState(false);
  const [style, setStyle] = useState<CSSProperties>({});
  const wrap = useRef<HTMLSpanElement>(null);
  const bubble = useRef<HTMLSpanElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clear = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => clear, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open]);

  useLayoutEffect(() => {
    if (!open || !wrap.current || !bubble.current) return;
    const anchor = wrap.current.getBoundingClientRect();
    const box = bubble.current.getBoundingClientRect();
    const vw = document.documentElement.clientWidth || window.innerWidth;
    const vh = document.documentElement.clientHeight || window.innerHeight;
    let left = anchor.left + anchor.width / 2 - box.width / 2;
    left = Math.max(MARGIN, Math.min(left, vw - box.width - MARGIN));
    let top = anchor.bottom + 6;
    if (top + box.height > vh - MARGIN) top = Math.max(MARGIN, anchor.top - box.height - 6);
    setStyle({ left, top });
  }, [open]);

  if (nested || !isValidElement(children) || text.trim() === '') return children;

  const described = [children.props['aria-describedby'], id].filter(Boolean).join(' ');
  const Wrapper = block ? 'div' : 'span';
  return (
    <InsideTooltip.Provider value={true}>
      <Wrapper
        ref={wrap as never}
        className={block ? 'sd-tip sd-tip--block' : 'sd-tip'}
        onMouseEnter={() => {
          clear();
          timer.current = setTimeout(() => setOpen(true), OPEN_DELAY_MS);
        }}
        onMouseLeave={() => {
          clear();
          setOpen(false);
        }}
        onFocus={(event) => {
          clear();
          // Solo col focus da tastiera: un focus dato dal codice (per esempio «Indietro» all'apertura
          // di una pagina dopo un clic) non deve coprire il contenuto.
          if (focusVisible(event.target)) setOpen(true);
        }}
        onBlur={() => setOpen(false)}
        onPointerDown={() => {
          clear();
          setOpen(false);
        }}
      >
        {cloneElement(children, { 'aria-describedby': described })}
        <span
          ref={bubble}
          id={id}
          role="tooltip"
          className="sd-tip__bubble"
          hidden={!open}
          style={style}
        >
          {text}
        </span>
      </Wrapper>
    </InsideTooltip.Provider>
  );
}
