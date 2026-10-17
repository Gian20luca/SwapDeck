import { expect, test, type Page } from '@playwright/test';

const SHOTS = 'e2e/screenshots';

async function open(page: Page) {
  await page.setViewportSize({ width: 420, height: 640 });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
  await expect(page.getByRole('list', { name: 'Account' }).getByRole('listitem')).toHaveCount(5);
}

/** Niente scroll orizzontale della pagina e nessun elemento visibile oltre il bordo destro. */
async function noHorizontalScroll(page: Page) {
  const problems = await page.evaluate(() => {
    const root = document.documentElement;
    const limit = root.clientWidth + 0.5;
    const found: string[] = [];
    if (root.scrollWidth > root.clientWidth) found.push(`pagina: ${root.scrollWidth}px`);
    for (const el of document.body.querySelectorAll('*')) {
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      if (getComputedStyle(el).visibility === 'hidden') continue;
      if (rect.right > limit) {
        found.push(
          `${el.tagName.toLowerCase()}.${[...el.classList].join('.')} esce di ${Math.round(rect.right - limit)}px`,
        );
      }
    }
    return found.slice(0, 10);
  });
  expect(problems).toEqual([]);
}

const ranking = (page: Page) => page.getByRole('list', { name: 'Classifica dei consumi' });

test('«Chi consuma questa utenza» dalla dashboard apre la scheda filtrata; raggruppamento e finestra', async ({
  page,
}) => {
  await open(page);
  await noHorizontalScroll(page);
  await page.getByRole('button', { name: 'Chi consuma questa utenza: alias1' }).click();

  await expect(page.getByRole('tab', { name: 'Chi consuma' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.getByRole('heading', { name: 'Chi consuma' })).toBeVisible();
  await expect(page.getByLabel('Utenza')).toHaveValue('1');
  await expect(ranking(page).getByRole('listitem').first()).toContainText('Progetti-SwapDeck');
  await expect(page.getByRole('list', { name: 'Altri dispositivi (stima)' })).toContainText(
    'Fiducia media',
  );
  await expect(page.getByText(/nulla esce dal PC/)).toBeVisible();
  await noHorizontalScroll(page);
  await page.screenshot({ path: `${SHOTS}/chi-consuma-420x640.png` });

  // Raggruppamento per agente.
  await page.getByRole('button', { name: 'Agente' }).click();
  await expect(page.getByRole('button', { name: 'Agente' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(ranking(page)).toContainText('Conversazione principale');
  await expect(ranking(page)).toContainText('Explore');
  await noHorizontalScroll(page);

  // Finestra di 7 giorni: la stima delle 7 giorni non è ancora calibrata.
  await page.getByRole('button', { name: '7 giorni' }).click();
  await expect(page.getByRole('button', { name: '7 giorni' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByRole('list', { name: 'Altri dispositivi (stima)' })).toContainText(
    'Stima non ancora disponibile',
  );
  await noHorizontalScroll(page);

  // Tutte le utenze: compare l'utenza sconosciuta.
  await page.getByLabel('Utenza').selectOption('all');
  await expect(page.getByText('Utenza sconosciuta')).toBeVisible();
  await noHorizontalScroll(page);
});

test('dallo Storico: il link nel pannello dell’utenza apre «Chi consuma»', async ({ page }) => {
  await open(page);
  await page.getByRole('tab', { name: 'Storico' }).click();
  await page.getByRole('button', { name: 'Chi consuma questa utenza: alias6' }).click();
  await expect(page.getByLabel('Utenza')).toHaveValue('6');
  await expect(page.getByRole('tab', { name: 'Chi consuma' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
});
