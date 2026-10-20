import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend, type FakeBackendOptions } from '../../dev/FakeBackend.js';
import { renderWith } from '../../dev/testUtils.js';
import { translate } from '../../i18n/index.js';
import { Dashboard } from '../Dashboard.js';
import { SettingsPage } from './SettingsPage.js';
import { reloginNotice } from './Relogin.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

async function setup(options: FakeBackendOptions = {}) {
  const backend = new FakeBackend(options);
  const copyText = vi.fn(async () => {});
  (backend as { copyText?: unknown }).copyText = copyText;
  const user = userEvent.setup();
  renderWith(<Dashboard manage />, backend);
  await screen.findByRole('list', { name: 'Account' });
  return { backend, user, copyText };
}

async function open(user: ReturnType<typeof userEvent.setup>) {
  const buttons = await screen.findAllByRole('button', { name: /^Rifai login di / });
  await user.click(buttons[0]!);
  return screen.findByRole('region', { name: /^Rifai login di / });
}

describe('Rifai login (F21)', () => {
  it('pulsante solo sulle utenze da riloggare e solo se l’host lo offre', async () => {
    await setup();
    expect(await screen.findAllByRole('button', { name: /^Rifai login di / })).toHaveLength(2);
  });

  it('nessun pulsante senza l’operazione nell’host', async () => {
    await setup({ relogin: false });
    expect(screen.queryByRole('button', { name: /^Rifai login di / })).toBeNull();
  });

  it('link con «Copia link», codice inviato e mai mostrato, esito e ritorno all’utenza di prima', async () => {
    const { backend, user, copyText } = await setup();
    const panel = await open(user);
    expect(
      within(panel).getByText(/si può aprire una scheda del browser: chiudila/i),
    ).toBeVisible();
    const url = within(panel).getByLabelText('Link di accesso');
    expect((url as HTMLTextAreaElement).value).toMatch(/^https:\/\/claude\.ai\/oauth/);
    await user.click(within(panel).getByRole('button', { name: 'Copia link' }));
    expect(copyText).toHaveBeenCalledWith(expect.stringMatching(/^https:/));
    expect(await within(panel).findByText('Link copiato.')).toBeInTheDocument();
    expect(within(panel).getByText('Hai 5 minuti per completare il login.')).toBeInTheDocument();
    const field = within(panel).getByLabelText('Codice');
    expect(field).toHaveAttribute('autocomplete', 'off');
    await user.type(field, 'CODICE-SEGRETO#1');
    await user.click(within(panel).getByRole('button', { name: 'Conferma' }));
    expect(
      await screen.findByText(/rinnovato\. È di nuovo attiva l’utenza di prima\./),
    ).toBeVisible();
    expect(screen.queryByRole('region', { name: /^Rifai login di / })).toBeNull();
    expect(document.body.innerHTML).not.toContain('CODICE-SEGRETO');
    expect(backend.relogin?.codes).toBe(1);
  });

  it('codice non valido resta in attesa; «Annulla» chiude con credenziali invariate', async () => {
    const { user } = await setup();
    const panel = await open(user);
    await user.type(within(panel).getByLabelText('Codice'), 'ab cd');
    await user.click(within(panel).getByRole('button', { name: 'Conferma' }));
    expect(await within(panel).findByText(/Codice non valido/)).toBeInTheDocument();
    await user.click(within(panel).getByRole('button', { name: 'Annulla' }));
    expect(await screen.findByText('Login annullato: credenziali invariate.')).toBeVisible();
  });

  it('altro account e Claude Code mancante: messaggi chiari', async () => {
    const { user } = await setup({ relogin: { result: 'wrongAccount' } });
    const panel = await open(user);
    await user.type(within(panel).getByLabelText('Codice'), 'x1');
    await user.click(within(panel).getByRole('button', { name: 'Conferma' }));
    expect(
      await screen.findByText(/Ha effettuato l’accesso con un altro account/),
    ).toBeInTheDocument();
  });

  it('Claude Code non trovato rimanda alle impostazioni', async () => {
    const { user } = await setup({ relogin: { result: 'claudeNotFound' } });
    const buttons = await screen.findAllByRole('button', { name: /^Rifai login di / });
    await user.click(buttons[0]!);
    expect(await screen.findByText(/Claude Code non trovato/)).toHaveTextContent(
      'Impostazioni → Avanzate',
    );
  });

  it('esiti senza utenza di prima e con avviso', () => {
    const t = (key: Parameters<typeof translate>[1], params?: Record<string, string | number>) =>
      translate('it', key, params);
    expect(
      reloginNotice({ result: 'wrongAccount', account: 2, restored: false }, 'B', t).text,
    ).toContain('Non c’era un’utenza a cui tornare.');
    expect(reloginNotice({ result: 'updated', account: 2, restored: false }, 'B', t).text).toBe(
      'Login di B rinnovato. Non c’era un’utenza a cui tornare.',
    );
    const failed = reloginNotice(
      {
        result: 'loginFailed',
        account: 2,
        previous: 6,
        restored: false,
        detail: 'Invalid code',
        warning: 'x',
      },
      'B',
      t,
    );
    expect(failed.tone).toBe('danger');
    expect(failed.text).toBe(
      'Login non riuscito. Invalid code L’utenza di prima non è tornata attiva. Attenzione: x',
    );
  });

  it('percorso di Claude Code in Impostazioni → Avanzate', async () => {
    const backend = new FakeBackend();
    const setPrefs = vi.spyOn(backend, 'setPreferences');
    const user = userEvent.setup();
    renderWith(<SettingsPage />, backend);
    await user.click(await screen.findByRole('button', { name: 'Avanzate' }));
    const field = await screen.findByLabelText('Percorso di Claude Code');
    await user.type(field, '/opt/claude');
    await user.click(
      within(field.closest('form')!).getByRole('button', { name: 'Salva percorso' }),
    );
    await waitFor(() => expect(setPrefs).toHaveBeenCalledWith({ claudeCodePath: '/opt/claude' }));
  });
});
