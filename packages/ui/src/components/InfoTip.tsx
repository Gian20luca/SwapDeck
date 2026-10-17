import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { useI18n } from '../i18n/index.js';
import { placeTip, type Box } from './Tooltip.js';

const OPEN_DELAY_MS = 150;
const CLOSE_DELAY_MS = 120;

/** Una sola «i» aperta alla volta. */
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

export interface InfoTipProps {
  /** Nome del concetto: entra in «Informazioni su {nome}». */
  name: string;
  /** Spiegazione breve (1–3 frasi): cosa fa e quando serve. */
  text: ReactNode;
}

/**
 * Piccola «i» cerchiata accanto a un concetto (titolo di sezione, impostazione, campo). La spiegazione
 * sta in un popover: si apre al passaggio del mouse e al focus da tastiera; col clic o il tocco
 * resta aperta, finché Esc, un clic fuori o un secondo clic la chiudono. Il popover è `fixed` e
 * tenuto nella finestra (`placeTip`): niente scroll orizzontale a 420 px. I pulsanti non la usano:
 * hanno il tooltip normale.
 */
export function InfoTip({ name, text }: InfoTipProps) {
  const { t } = useI18n();
  const id = useId();
  const [hover, setHover] = useState(false);
  const [focus, setFocus] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [style, setStyle] = useState<CSSProperties>({});
  const wrap = useRef<HTMLSpanElement>(null);
  const bubble = useRef<HTMLSpanElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const open = hover || focus || pinned;

  const clear = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => clear, []);

  const closeAll = () => {
    clear();
    setHover(false);
    setFocus(false);
    setPinned(false);
  };

  useEffect(() => {
    if (!open) return;
    const close = () => {
      setHover(false);
      setFocus(false);
      setPinned(false);
    };
    if (closeOpen && closeOpen !== close) closeOpen();
    closeOpen = close;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };
    const onPointer = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) close();
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', onPointer, true);
    window.addEventListener('scroll', close, true);
    return () => {
      if (closeOpen === close) closeOpen = null;
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('pointerdown', onPointer, true);
      window.removeEventListener('scroll', close, true);
    };
  }, [open]);

  useLayoutEffect(() => {
    if (!open || !wrap.current || !bubble.current) return;
    const anchor = wrap.current.getBoundingClientRect();
    const box = bubble.current.getBoundingClientRect();
    const viewport = {
      width: document.documentElement.clientWidth || window.innerWidth,
      height: document.documentElement.clientHeight || window.innerHeight,
    };
    setStyle(
      placeTip(
        { left: anchor.left, top: anchor.top, width: anchor.width, height: anchor.height },
        { width: box.width, height: box.height },
        viewport,
        [] as Box[],
      ),
    );
  }, [open]);

  return (
    <span
      ref={wrap}
      className="sd-info"
      onMouseEnter={() => {
        clear();
        timer.current = setTimeout(() => setHover(true), OPEN_DELAY_MS);
      }}
      onMouseLeave={() => {
        clear();
        timer.current = setTimeout(() => setHover(false), CLOSE_DELAY_MS);
      }}
    >
      <button
        type="button"
        className="sd-info__btn"
        aria-label={t('info.label', { name })}
        aria-expanded={open}
        aria-describedby={id}
        onFocus={(event) => {
          if (focusVisible(event.target)) setFocus(true);
        }}
        onBlur={() => setFocus(false)}
        onClick={() => {
          clear();
          if (pinned) closeAll();
          else {
            setPinned(true);
            setHover(true);
          }
        }}
      >
        <span aria-hidden="true">i</span>
      </button>
      <span
        ref={bubble}
        id={id}
        role="tooltip"
        className="sd-info__bubble"
        hidden={!open}
        style={style}
      >
        {text}
      </span>
    </span>
  );
}

/** Etichetta (o titolo) seguita dalla «i»: sta fuori dal `<label>` così il clic sulla «i» non attiva il campo. */
export function WithInfo({
  info,
  name,
  children,
}: {
  info?: ReactNode;
  name: string;
  children: ReactNode;
}) {
  if (!info) return <>{children}</>;
  return (
    <span className="sd-info-row">
      {children}
      <InfoTip name={name} text={info} />
    </span>
  );
}
