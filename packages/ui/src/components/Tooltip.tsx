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
const GAP = 6;
const OBSTACLES =
  'button, input, select, textarea, a[href], summary, [role="tab"], [role="switch"], [tabindex="0"]';

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

function overlap(a: Box, b: Box): number {
  const w = Math.min(a.left + a.width, b.left + b.width) - Math.max(a.left, b.left);
  const h = Math.min(a.top + a.height, b.top + b.height) - Math.max(a.top, b.top);
  return w > 0 && h > 0 ? w * h : 0;
}

/**
 * Posizione della bolla: sotto, sopra, a destra o a sinistra dell'elemento, dentro la finestra.
 * Vince il primo lato che non copre altri controlli (`obstacles`); se nessuno è libero, quello che
 * ne copre meno. Pura, per i test.
 */
export function placeTip(
  anchor: Box,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  obstacles: readonly Box[],
): { left: number; top: number } {
  const clampX = (x: number) => Math.max(MARGIN, Math.min(x, viewport.width - size.width - MARGIN));
  const clampY = (y: number) =>
    Math.max(MARGIN, Math.min(y, viewport.height - size.height - MARGIN));
  const cx = anchor.left + anchor.width / 2 - size.width / 2;
  const cy = anchor.top + anchor.height / 2 - size.height / 2;
  const sides = [
    { left: clampX(cx), top: anchor.top + anchor.height + GAP },
    { left: clampX(cx), top: anchor.top - size.height - GAP },
    { left: anchor.left + anchor.width + GAP, top: clampY(cy) },
    { left: anchor.left - size.width - GAP, top: clampY(cy) },
  ];
  let best = { left: clampX(cx), top: clampY(anchor.top + anchor.height + GAP) };
  let bestScore = Number.POSITIVE_INFINITY;
  for (const p of sides) {
    const fits =
      p.left >= MARGIN - 0.5 &&
      p.top >= MARGIN - 0.5 &&
      p.left + size.width <= viewport.width - MARGIN + 0.5 &&
      p.top + size.height <= viewport.height - MARGIN + 0.5;
    if (!fits) continue;
    const box = { ...p, ...size };
    const score = obstacles.reduce((sum, o) => sum + overlap(box, o), 0) + overlap(box, anchor);
    if (score < bestScore) {
      best = p;
      bestScore = score;
      if (score === 0) break;
    }
  }
  return best;
}

/** Un solo tooltip aperto alla volta: aprirne uno chiude il precedente. */
let closeOpen: (() => void) | null = null;

/** Istante dell'ultimo clic o tocco: un focus subito dopo viene dal mouse, non dalla tastiera. */
let lastPointer = 0;
if (typeof document !== 'undefined')
  document.addEventListener('pointerdown', () => (lastPointer = Date.now()), true);

function focusVisible(el: EventTarget): boolean {
  if (Date.now() - lastPointer < 800) return false;
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
 * Tooltip accessibile: si apre al passaggio del mouse (con un breve ritardo) e subito al focus da tastiera; si
 * chiude con Esc, all'uscita, alla perdita del focus, al clic, allo scorrimento e quando se ne apre
 * un altro. Si mette sul lato che non copre altri controlli (`placeTip`). Il testo resta nel DOM (`role="tooltip"`,
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

  // Aperto: Esc, un clic altrove o lo scorrimento lo chiudono; aprirne un altro pure.
  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(false);
    if (closeOpen) closeOpen();
    closeOpen = close;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', close, true);
    window.addEventListener('scroll', close, true);
    return () => {
      if (closeOpen === close) closeOpen = null;
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('pointerdown', close, true);
      window.removeEventListener('scroll', close, true);
    };
  }, [open]);

  useLayoutEffect(() => {
    if (!open || !wrap.current || !bubble.current) return;
    const own = wrap.current;
    const anchor = own.getBoundingClientRect();
    const box = bubble.current.getBoundingClientRect();
    const viewport = {
      width: document.documentElement.clientWidth || window.innerWidth,
      height: document.documentElement.clientHeight || window.innerHeight,
    };
    const obstacles: Box[] = [];
    for (const el of document.querySelectorAll(OBSTACLES)) {
      if (own.contains(el)) continue;
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.height > 0 && r.bottom > 0 && r.top < viewport.height)
        obstacles.push({ left: r.left, top: r.top, width: r.width, height: r.height });
    }
    setStyle(
      placeTip(
        { left: anchor.left, top: anchor.top, width: anchor.width, height: anchor.height },
        { width: box.width, height: box.height },
        viewport,
        obstacles,
      ),
    );
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
