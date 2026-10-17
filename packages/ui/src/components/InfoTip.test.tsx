import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { FakeBackend } from '../dev/FakeBackend.js';
import { renderWith } from '../dev/testUtils.js';
import { I18nProvider } from '../i18n/index.js';
import { InfoTip, WithInfo } from './InfoTip.js';
import { CheckField, Section, SelectField, TextField } from './manage/common.js';

function bubble(): HTMLElement {
  return document.querySelector('[role="tooltip"]')!;
}

describe('InfoTip', () => {
  it('bottone con nome accessibile, aria-describedby e aria-expanded; chiuso all’inizio', () => {
    render(<InfoTip name="Soglia" text="Spiega la soglia." />);
    const button = screen.getByRole('button', { name: 'Informazioni su Soglia' });
    expect(button).toHaveAttribute('aria-expanded', 'false');
    expect(button.getAttribute('aria-describedby')).toBe(bubble().id);
    expect(bubble()).toHaveTextContent('Spiega la soglia.');
    expect(bubble()).not.toBeVisible();
  });

  it('si apre al passaggio del mouse e si chiude all’uscita', async () => {
    const user = userEvent.setup();
    render(<InfoTip name="Soglia" text="Testo" />);
    const button = screen.getByRole('button', { name: /Soglia/ });
    await user.hover(button);
    await waitFor(() => expect(bubble()).toBeVisible());
    expect(button).toHaveAttribute('aria-expanded', 'true');
    await user.unhover(button);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250));
    });
    expect(bubble()).not.toBeVisible();
  });

  it('si apre col focus da tastiera e si chiude con Esc', async () => {
    const user = userEvent.setup();
    render(<InfoTip name="Soglia" text="Testo" />);
    await user.tab();
    expect(bubble()).toBeVisible();
    await user.keyboard('{Escape}');
    expect(bubble()).not.toBeVisible();
  });

  it('il clic la tiene aperta anche senza mouse sopra; un secondo clic la chiude', async () => {
    const user = userEvent.setup();
    render(<InfoTip name="Soglia" text="Testo" />);
    const button = screen.getByRole('button', { name: /Soglia/ });
    await user.click(button);
    await user.unhover(button);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250));
    });
    expect(bubble()).toBeVisible();
    await user.click(button);
    expect(bubble()).not.toBeVisible();
  });

  it('il clic fuori e Esc chiudono quella fissata col clic', async () => {
    const user = userEvent.setup();
    render(
      <div>
        <InfoTip name="Soglia" text="Testo" />
        <button type="button">Altrove</button>
      </div>,
    );
    await user.click(screen.getByRole('button', { name: /Soglia/ }));
    expect(bubble()).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Altrove' }));
    expect(bubble()).not.toBeVisible();
    await user.click(screen.getByRole('button', { name: /Soglia/ }));
    expect(bubble()).toBeVisible();
    await user.keyboard('{Escape}');
    expect(bubble()).not.toBeVisible();
  });

  it('ne resta aperta una sola', async () => {
    const user = userEvent.setup();
    render(
      <div>
        <InfoTip name="Uno" text="Primo" />
        <InfoTip name="Due" text="Secondo" />
      </div>,
    );
    await user.click(screen.getByRole('button', { name: /Uno/ }));
    expect(screen.getByText('Primo')).toBeVisible();
    await user.click(screen.getByRole('button', { name: /Due/ }));
    expect(screen.getByText('Secondo')).toBeVisible();
    expect(screen.getByText('Primo')).not.toBeVisible();
  });

  it('il nome accessibile segue la lingua', () => {
    render(
      <I18nProvider initial="en">
        <InfoTip name="Threshold" text="Text" />
      </I18nProvider>,
    );
    expect(screen.getByRole('button', { name: 'About Threshold' })).toBeInTheDocument();
  });
});

describe('WithInfo e campi di common.tsx', () => {
  it('WithInfo senza info mostra solo i figli', () => {
    render(
      <WithInfo name="X">
        <span>solo</span>
      </WithInfo>,
    );
    expect(screen.getByText('solo')).toBeInTheDocument();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('prop info su Section e sui campi: la «i» sta accanto all’etichetta, fuori dal label', async () => {
    const user = userEvent.setup();
    renderWith(
      <Section title="Titolo" info="Info sezione">
        <TextField label="Nome" value="" onChange={() => undefined} info="Info testo" />
        <SelectField
          label="Scelta"
          value="a"
          onChange={() => undefined}
          options={[{ value: 'a', label: 'A' }]}
          info="Info scelta"
        />
        <CheckField label="Attivo" checked={false} onChange={() => undefined} info="Info check" />
        <TextField label="Senza" value="" onChange={() => undefined} />
      </Section>,
      new FakeBackend(),
    );
    for (const name of ['Titolo', 'Nome', 'Scelta', 'Attivo'])
      expect(screen.getByRole('button', { name: `Informazioni su ${name}` })).toBeInTheDocument();
    expect(screen.getAllByRole('button')).toHaveLength(4);
    // Il nome della sezione non include il testo della «i».
    expect(screen.getByRole('region', { name: 'Titolo' })).toBeInTheDocument();
    // Il clic sulla «i» non attiva il campo.
    await user.click(screen.getByRole('button', { name: 'Informazioni su Attivo' }));
    expect(screen.getByRole('checkbox', { name: 'Attivo' })).not.toBeChecked();
    expect(screen.getByLabelText('Nome')).toBeInTheDocument();
  });
});
