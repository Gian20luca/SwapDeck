import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ConfirmDialog } from './ConfirmDialog.js';

function setup(props: Partial<Parameters<typeof ConfirmDialog>[0]> = {}) {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  render(
    <ConfirmDialog
      open
      title="Rimuovere?"
      message="Sicuro?"
      onConfirm={onConfirm}
      onCancel={onCancel}
      {...props}
    />,
  );
  return { onConfirm, onCancel };
}

describe('ConfirmDialog semplice', () => {
  it('non rende nulla da chiuso', () => {
    render(<ConfirmDialog open={false} title="x" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('è accessibile: ruolo, titolo e descrizione', () => {
    setup();
    const dialog = screen.getByRole('alertdialog', { name: 'Rimuovere?' });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(dialog).toHaveAccessibleDescription('Sicuro?');
  });

  it('conferma con un clic', async () => {
    const { onConfirm } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Conferma' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('annulla con il pulsante, con Esc e con clic sullo sfondo', async () => {
    const { onCancel } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Annulla' }));
    await userEvent.keyboard('{Escape}');
    await userEvent.click(document.querySelector('.sd-overlay')!);
    expect(onCancel).toHaveBeenCalledTimes(3);
  });

  it('focus iniziale: Conferma di default, Annulla se pericoloso', () => {
    setup();
    expect(screen.getByRole('button', { name: 'Conferma' })).toHaveFocus();
  });

  it('con tone=danger il focus parte da Annulla', () => {
    setup({ tone: 'danger' });
    expect(screen.getByRole('button', { name: 'Annulla' })).toHaveFocus();
  });

  it('intrappola il focus con Tab e Maiusc+Tab', async () => {
    setup();
    const cancel = screen.getByRole('button', { name: 'Annulla' });
    const confirm = screen.getByRole('button', { name: 'Conferma' });
    expect(confirm).toHaveFocus();
    await userEvent.tab();
    expect(cancel).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(confirm).toHaveFocus();
  });

  it('restituisce il focus a chi l’ha aperto', async () => {
    function Host() {
      return (
        <>
          <button>apri</button>
        </>
      );
    }
    const { rerender } = render(<Host />);
    const opener = screen.getByText('apri');
    opener.focus();
    rerender(
      <>
        <Host />
        <ConfirmDialog open title="T" onConfirm={vi.fn()} onCancel={vi.fn()} />
      </>,
    );
    expect(screen.getByRole('button', { name: 'Conferma' })).toHaveFocus();
    rerender(
      <>
        <Host />
        <ConfirmDialog open={false} title="T" onConfirm={vi.fn()} onCancel={vi.fn()} />
      </>,
    );
    expect(opener).toHaveFocus();
  });

  it('con onConfirm asincrono resta occupato finché non termina', async () => {
    let done!: () => void;
    const onConfirm = vi.fn(() => new Promise<void>((resolve) => (done = resolve)));
    setup({ onConfirm });
    await userEvent.click(screen.getByRole('button', { name: 'Conferma' }));
    expect(screen.getByRole('button', { name: 'Operazione in corso…' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Annulla' })).toBeDisabled();
    done();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Conferma' })).toBeEnabled());
  });
});

describe('ConfirmDialog a doppia conferma', () => {
  it('chiede prima la conferma semplice, poi la parola', async () => {
    const { onConfirm } = setup({
      requireWord: 'ELIMINA',
      confirmLabel: 'Elimina tutto',
      tone: 'danger',
    });
    expect(screen.getByText('Passo 1 di 2')).toBeInTheDocument();
    expect(screen.queryByLabelText('Parola di conferma')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Continua' }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByText('Passo 2 di 2')).toBeInTheDocument();
    const input = screen.getByLabelText('Parola di conferma');
    expect(input).toHaveFocus();
    const final = screen.getByRole('button', { name: 'Elimina tutto' });
    expect(final).toBeDisabled();
    await userEvent.type(input, 'elimina');
    expect(final).toBeDisabled();
    await userEvent.clear(input);
    await userEvent.type(input, 'ELIMINA');
    expect(final).toBeEnabled();
    await userEvent.click(final);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('Invio nel campo conferma solo se la parola è giusta', async () => {
    const { onConfirm } = setup({ requireWord: 'OK' });
    await userEvent.click(screen.getByRole('button', { name: 'Continua' }));
    await userEvent.type(screen.getByLabelText('Parola di conferma'), 'N{Enter}');
    expect(onConfirm).not.toHaveBeenCalled();
    await userEvent.type(screen.getByLabelText('Parola di conferma'), '{Backspace}OK{Enter}');
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('annullare al secondo passo non esegue nulla', async () => {
    const { onConfirm, onCancel } = setup({ requireWord: 'OK' });
    await userEvent.click(screen.getByRole('button', { name: 'Continua' }));
    await userEvent.click(screen.getByRole('button', { name: 'Annulla' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
