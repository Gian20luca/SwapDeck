import { expect, test, type Page } from '@playwright/test';

const SHOTS = 'e2e/screenshots';

async function open(page: Page) {
  await page.setViewportSize({ width: 420, height: 640 });
  await page.goto('/');
  await expect(page.getByRole('list', { name: 'Account' }).getByRole('listitem')).toHaveCount(5);
}

async function rules(page: Page) {
  await page.getByRole('tab', { name: 'Regole' }).click();
  await expect(page.getByRole('heading', { name: 'Regole del motore' })).toBeVisible();
}

const rowNames = (page: Page) =>
  page
    .getByRole('list', { name: 'Utenze in ordine di priorità' })
    .locator('.sd-flow__name')
    .evaluateAll((els) => els.map((e) => e.firstChild?.textContent ?? ''));

async function noHorizontalScroll(page: Page) {
  const overflow = await page.evaluate(() => {
    const root = document.documentElement;
    const out: string[] = [];
    if (root.scrollWidth > root.clientWidth) out.push(`pagina ${root.scrollWidth}`);
    for (const el of document.body.querySelectorAll('*')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.height > 0 && r.right > root.clientWidth + 0.5)
        out.push(`${el.tagName}.${el.className} ${Math.round(r.right)}`);
    }
    return out.slice(0, 5);
  });
  expect(overflow).toEqual([]);
}

test('riordino col mouse sulla maniglia ⋮⋮ e salvataggio', async ({ page }) => {
  await open(page);
  await rules(page);
  expect((await rowNames(page)).slice(0, 3)).toEqual(['alias6', 'alias1', 'alias3']);
  const handle = page.getByRole('button', { name: 'Sposta alias3, posizione 3 di 5' });
  const target = page.getByTestId('flow-row-6');
  await target.scrollIntoViewIfNeeded();
  await handle.scrollIntoViewIfNeeded();
  const from = (await handle.boundingBox())!;
  const to = (await target.boundingBox())!;
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + from.width / 2, to.y + 4, { steps: 8 });
  await page.mouse.up();
  expect((await rowNames(page)).slice(0, 3)).toEqual(['alias3', 'alias6', 'alias1']);
  await noHorizontalScroll(page);
  await page.screenshot({ path: `${SHOTS}/regole-flusso-420x640.png`, fullPage: true });
  await page.getByRole('button', { name: 'Salva' }).click();
  await expect(page.getByText('Regole salvate.', { exact: true })).toBeVisible();
  expect((await rowNames(page))[0]).toBe('alias3');
});

test('cambio di mappa dal selettore', async ({ page }) => {
  await open(page);
  await rules(page);
  await page.getByLabel('Mappa attiva').selectOption('eco');
  await expect(page.getByText('Mappa attiva: Eco.')).toBeVisible();
  await expect(page.getByLabel('Soglia sulle 5 ore (%)')).toHaveValue('90');
  // Preset in sola lettura nel riquadro della mappa: si clona come mappa personale.
  await expect(page.getByText('Preset in sola lettura: clonalo per modificarlo.')).toBeVisible();
  await expect(page.getByLabel('Soglia sulle 5 ore (%)')).toBeDisabled();
  await noHorizontalScroll(page);
  await page.getByRole('button', { name: 'Clona come mappa personale' }).click();
  await page.getByLabel('Nome della mappa').fill('Mia eco');
  await page.getByRole('button', { name: 'Conferma', exact: true }).click();
  await expect(page.getByText('Mappa personale «Mia eco» creata e aperta.')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Regole della mappa «Mia eco»' })).toBeVisible();
  await expect(page.getByLabel('Soglia sulle 5 ore (%)')).toBeEnabled();
  await page.getByRole('button', { name: 'Nuova' }).click();
  await page.getByLabel('Nome della mappa').fill('Notte');
  await page.getByRole('button', { name: 'Conferma', exact: true }).click();
  await expect(page.getByText('Mappa «Notte» creata.')).toBeVisible();
  await noHorizontalScroll(page);
});

test('dettaglio dell’utenza e ritorno all’elenco', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: 'Gestisci l’account alias3' }).click();
  const detail = page.getByRole('region', { name: 'Gestione di alias3' });
  await expect(detail).toBeVisible();
  await expect(detail.getByRole('heading', { name: 'Modifica alias3' })).toBeVisible();
  await expect(detail.getByRole('progressbar')).toHaveCount(0);
  await expect(detail.getByRole('button', { name: /Passa a questo account/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Indietro/ })).toBeFocused();
  await noHorizontalScroll(page);
  // Senza fullPage: dopo lo screenshot a pagina intera il clic cadrebbe fuori posto.
  await page.screenshot({ path: `${SHOTS}/dettaglio-420x640.png` });
  await page.getByRole('button', { name: /Indietro/ }).click();
  await expect(detail).toHaveCount(0);
  await expect(page.getByRole('list', { name: 'Account' }).getByRole('listitem')).toHaveCount(5);
});

test('pagina Notifiche a 420×640 senza scroll orizzontale', async ({ page }) => {
  await open(page);
  await page.getByRole('tab', { name: 'Impostazioni' }).click();
  await page.getByRole('button', { name: 'Notifiche…' }).click();
  await expect(page.getByRole('region', { name: 'Notifiche' })).toBeVisible();
  await page.getByLabel('Non disturbare a orari').check();
  await page.getByLabel('Reset imminente').check();
  await noHorizontalScroll(page);
  await page.screenshot({ path: `${SHOTS}/notifiche-420x640.png`, fullPage: true });
});

test('tooltip: non copre i controlli vicini e sparisce all’uscita del mouse', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: 'Gestisci l’account alias3' }).click();
  const back = page.getByRole('button', { name: /Indietro/ });
  await back.hover();
  const tip = page.getByRole('tooltip').filter({ hasText: 'Torna all’elenco degli account.' });
  await expect(tip).toBeVisible();
  const box = (await tip.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(420);
  const covered = await page.evaluate(
    ({ x, y, width, height }) =>
      [...document.querySelectorAll('button, input, select, [role="switch"]')]
        .filter((el) => !el.closest('.sd-tip')?.querySelector('[role="tooltip"]:not([hidden])'))
        .map((el) => ({ el, r: el.getBoundingClientRect() }))
        .filter(
          ({ r }) =>
            r.width > 0 && r.left < x + width && r.right > x && r.top < y + height && r.bottom > y,
        )
        .map(({ el }) => el.textContent ?? ''),
    box,
  );
  expect(covered).toEqual([]);
  await page.mouse.move(400, 630);
  await expect(tip).toBeHidden();
  await back.hover();
  await expect(tip).toBeVisible();
  await page.getByRole('textbox', { name: 'Alias', exact: true }).click();
  await expect(tip).toBeHidden();
});
