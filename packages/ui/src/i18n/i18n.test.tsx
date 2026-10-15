import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { en } from './en.js';
import { I18nProvider, interpolate, resolveLocale, translate, useI18n } from './index.js';
import { it as itDict } from './it.js';

describe('dizionari', () => {
  it('it ed en hanno le stesse chiavi', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(itDict).sort());
  });
  it('nessuna traduzione è vuota e i segnaposto coincidono', () => {
    const placeholders = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join(',');
    for (const key of Object.keys(itDict) as (keyof typeof itDict)[]) {
      expect(en[key].length).toBeGreaterThan(0);
      expect(placeholders(en[key]), key).toBe(placeholders(itDict[key]));
    }
  });
});

describe('funzioni', () => {
  it('interpola i segnaposto e lascia quelli senza valore', () => {
    expect(interpolate('Ciao {nome} {x}', { nome: 'Ada' })).toBe('Ciao Ada {x}');
    expect(interpolate('Ciao')).toBe('Ciao');
  });
  it('risolve auto dalla lingua del sistema', () => {
    expect(resolveLocale('auto', 'en-US')).toBe('en');
    expect(resolveLocale('auto', 'it-IT')).toBe('it');
    expect(resolveLocale('auto', 'de-DE')).toBe('it');
    expect(resolveLocale('en', 'it-IT')).toBe('en');
  });
  it('traduce in entrambe le lingue', () => {
    expect(translate('it', 'common.cancel')).toBe('Annulla');
    expect(translate('en', 'common.cancel')).toBe('Cancel');
  });
});

function Probe() {
  const { t, locale, setPreference } = useI18n();
  return (
    <div>
      <span data-testid="loc">{locale}</span>
      <span>{t('common.refresh')}</span>
      <button onClick={() => setPreference('en')}>vai</button>
    </div>
  );
}

describe('I18nProvider', () => {
  it('cambia lingua e notifica onChange', async () => {
    const seen: string[] = [];
    render(
      <I18nProvider initial="it" onChange={(p) => seen.push(p)}>
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByText('Aggiorna')).toBeInTheDocument();
    await userEvent.click(screen.getByText('vai'));
    expect(screen.getByText('Refresh')).toBeInTheDocument();
    expect(seen).toEqual(['en']);
  });
  it('senza provider usa l’italiano', () => {
    render(<Probe />);
    expect(screen.getByTestId('loc')).toHaveTextContent('it');
  });
});
