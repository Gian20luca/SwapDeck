import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { I18nProvider } from '../i18n/index.js';
import { ThemeProvider } from '../theme/index.js';
import { LanguageSelect, ThemeSelect } from './PreferenceControls.js';

describe('PreferenceControls', () => {
  it('cambia lingua e le etichette si traducono', async () => {
    render(
      <I18nProvider initial="it">
        <LanguageSelect />
      </I18nProvider>,
    );
    await userEvent.selectOptions(screen.getByLabelText('Lingua'), 'en');
    expect(screen.getByLabelText('Language')).toHaveValue('en');
    expect(screen.getByRole('option', { name: 'Automatic' })).toBeInTheDocument();
  });

  it('cambia tema e aggiorna data-theme', async () => {
    render(
      <ThemeProvider initial="light">
        <ThemeSelect />
      </ThemeProvider>,
    );
    await userEvent.selectOptions(screen.getByLabelText('Tema'), 'dark');
    expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  });
});
