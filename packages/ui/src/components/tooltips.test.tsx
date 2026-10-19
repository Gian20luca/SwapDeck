import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend } from '../dev/FakeBackend.js';
import { renderWith } from '../dev/testUtils.js';
import { MiniWidget } from './MiniWidget.js';
import { SwapDeckApp } from './SwapDeckApp.js';
import { placeTip, Tooltip } from './Tooltip.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

const INTERACTIVE =
  'button, input, select, textarea, a[href], summary, [role="tab"], [role="switch"], [role="slider"]';

/** Elementi interattivi visibili senza un tooltip collegato (`aria-describedby` → `role="tooltip"`). */
function missingTooltips(root: HTMLElement): string[] {
  const out: string[] = [];
  for (const el of root.querySelectorAll<HTMLElement>(INTERACTIVE)) {
    if (el.closest('[hidden], [role="tooltip"], [role="alertdialog"], .uplot')) continue;
    if (el instanceof HTMLInputElement && el.type === 'hidden') continue;
    const ids = (el.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean);
    const ok = ids.some((id) => document.getElementById(id)?.getAttribute('role') === 'tooltip');
    if (!ok) {
      const name =
        el.getAttribute('aria-label') ??
        el.textContent?.trim() ??
        (el as HTMLInputElement).name ??
        el.id;
      out.push(`${el.tagName.toLowerCase()} «${name.slice(0, 40)}»`);
    }
  }
  return out;
}

const TABS = [
  'Account',
  'Cartelle',
  'Regole',
  'Storico',
  'Chi consuma',
  'Strumenti',
  'Impostazioni',
];

describe('Tooltip', () => {
  it('si apre al focus, collega aria-describedby, si chiude con Esc; niente tooltip annidati', async () => {
    const user = userEvent.setup();
    renderWith(
      <Tooltip text="Esterno">
        <button type="button" aria-describedby="altro">
          Uno
        </button>
      </Tooltip>,
      new FakeBackend(),
    );
    const button = screen.getByRole('button', { name: 'Uno' });
    const tip = document.querySelector('[role="tooltip"]')!;
    expect(button.getAttribute('aria-describedby')).toBe(`altro ${tip.id}`);
    expect(tip).not.toBeVisible();
    await user.tab();
    expect(tip).toBeVisible();
    await user.keyboard('{Escape}');
    expect(tip).not.toBeVisible();
  });

  it('dentro un tooltip non se ne crea un altro', () => {
    renderWith(
      <Tooltip text="Fuori">
        <span>
          <Tooltip text="Dentro">
            <button type="button">Due</button>
          </Tooltip>
        </span>
      </Tooltip>,
      new FakeBackend(),
    );
    expect(document.querySelectorAll('[role="tooltip"]')).toHaveLength(1);
  });
});

describe('placeTip', () => {
  const view = { width: 420, height: 640 };
  const size = { width: 200, height: 40 };
  it('sotto se libero; sopra se sotto c’è un controllo; mai fuori dalla finestra', () => {
    const anchor = { left: 20, top: 100, width: 80, height: 26 };
    expect(placeTip(anchor, size, view, [])).toEqual({ left: 8, top: 132 });
    const below = { left: 20, top: 140, width: 120, height: 26 };
    const p = placeTip(anchor, size, view, [below]);
    expect(p.top).toBe(54);
    // In fondo alla finestra non c'è spazio sotto: sopra.
    const low = { left: 300, top: 600, width: 80, height: 26 };
    const q = placeTip(low, size, view, []);
    expect(q.top + size.height).toBeLessThanOrEqual(600);
    expect(q.left + size.width).toBeLessThanOrEqual(view.width - 8);
  });

  it('di lato se sopra e sotto ci sono controlli', () => {
    const anchor = { left: 20, top: 300, width: 60, height: 26 };
    const above = { left: 0, top: 240, width: 420, height: 50 };
    const below = { left: 0, top: 336, width: 420, height: 50 };
    const p = placeTip(anchor, { width: 150, height: 30 }, view, [above, below]);
    expect(p.left).toBe(86);
  });
});

describe('Tooltip su ogni elemento interattivo', () => {
  it('in tutte le schede, nel dettaglio dell’utenza e nella pagina Notifiche', async () => {
    const user = userEvent.setup();
    renderWith(
      <SwapDeckApp />,
      new FakeBackend({ rules: { enabled: true, primary: 6, reserves: [1, 3] } }),
    );
    await screen.findByRole('list', { name: 'Account' });
    const problems: string[] = [];
    for (const tab of TABS) {
      await user.click(screen.getByRole('tab', { name: tab }));
      const panel = screen.getByRole('tabpanel');
      await waitFor(() => expect(within(panel).queryByRole('status', { name: '' })).toBeNull());
      await waitFor(() => expect(panel.querySelector('.sd-state[role="status"]')).toBeNull());
      problems.push(...missingTooltips(document.body).map((p) => `${tab}: ${p}`));
    }
    await user.click(screen.getByRole('tab', { name: 'Account' }));
    await user.click(await screen.findByRole('button', { name: 'Aggiungi account' }));
    problems.push(...missingTooltips(document.body).map((p) => `Aggiungi: ${p}`));
    await user.click(screen.getByRole('radio', { name: /token/i }));
    problems.push(...missingTooltips(document.body).map((p) => `Aggiungi token: ${p}`));
    await user.click(await screen.findByRole('button', { name: 'Gestisci l’account alias3' }));
    problems.push(...missingTooltips(document.body).map((p) => `Dettaglio: ${p}`));
    await user.click(screen.getByRole('tab', { name: 'Impostazioni' }));
    await user.click(await screen.findByRole('button', { name: 'Avanzate' }));
    await screen.findByRole('list', { name: 'Impostazioni di cswap auto' });
    problems.push(...missingTooltips(document.body).map((p) => `Avanzate: ${p}`));
    await user.click(await screen.findByRole('button', { name: 'Notifiche…' }));
    await screen.findByRole('region', { name: 'Notifiche' });
    await waitFor(() => expect(document.querySelector('.sd-state[role="status"]')).toBeNull());
    problems.push(...missingTooltips(document.body).map((p) => `Notifiche: ${p}`));
    expect([...new Set(problems)].join(' | ')).toBe('');
  }, 60_000);

  it('nel mini-widget', async () => {
    renderWith(<MiniWidget />, new FakeBackend());
    await screen.findAllByRole('button');
    await waitFor(() => expect(missingTooltips(document.body).join(' | ')).toBe(''));
  });
});
