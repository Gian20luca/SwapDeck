import { expect, test, type Page } from '@playwright/test';

const SHOTS = 'e2e/screenshots';

async function open(page: Page) {
  await page.setViewportSize({ width: 420, height: 640 });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
  await expect(page.getByRole('list', { name: 'Account' }).getByRole('listitem')).toHaveCount(5);
}

async function tab(page: Page, name: string) {
  await page.getByRole('tab', { name }).click();
  await expect(page.getByRole('tab', { name })).toHaveAttribute('aria-selected', 'true');
}

/**
 * Niente scroll orizzontale: né della pagina né di un contenitore interno, e nessun elemento
 * visibile che esca dal bordo destro (contenuti tagliati da un `overflow: hidden`).
 */
async function noHorizontalScroll(page: Page) {
  const problems = await page.evaluate(() => {
    const root = document.documentElement;
    const limit = root.clientWidth + 0.5;
    const found: string[] = [];
    if (root.scrollWidth > root.clientWidth) found.push(`pagina: ${root.scrollWidth}px`);
    const name = (el: Element) =>
      `${el.tagName.toLowerCase()}.${[...el.classList].join('.')} "${(el.textContent ?? '').trim().slice(0, 30)}"`;
    for (const el of document.body.querySelectorAll('*')) {
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      const style = getComputedStyle(el);
      if (style.visibility === 'hidden') continue;
      if (rect.right > limit) found.push(`${name(el)} esce di ${Math.round(rect.right - limit)}px`);
      if (/(auto|scroll)/.test(style.overflowX) && el.scrollWidth > el.clientWidth + 1) {
        found.push(`${name(el)} scorre di ${el.scrollWidth - el.clientWidth}px`);
      }
    }
    return found.slice(0, 10);
  });
  expect(problems).toEqual([]);
}

test('navigazione a schede a 420×640 senza scroll orizzontale', async ({ page }) => {
  await open(page);
  await noHorizontalScroll(page);
  // Pannello delle azioni aperto: è la parte più larga della scheda Account.
  await page.getByRole('button', { name: 'Gestisci l’account alias1' }).click();
  await expect(page.getByRole('group', { name: 'Gestione di alias1' })).toBeVisible();
  await noHorizontalScroll(page);
  // Senza fullPage: allungando la pagina sparisce la barra e il ritaglio non sarebbe fedele.
  await page.screenshot({ path: `${SHOTS}/account-420x640.png` });

  await tab(page, 'Cartelle');
  await expect(page.getByRole('heading', { name: 'Cartelle mappate' })).toBeVisible();
  await expect(page.getByText('C:\\Progetti\\cliente-a')).toBeVisible();
  await noHorizontalScroll(page);

  await tab(page, 'Regole');
  await expect(page.getByRole('heading', { name: 'Regole del motore' })).toBeVisible();
  await expect(page.getByRole('alert', { name: 'Stato del motore' })).toContainText(
    'Attività pianificata cswap-guard',
  );
  await noHorizontalScroll(page);

  await tab(page, 'Storico');
  await expect(page.getByRole('heading', { name: 'Storico' })).toBeVisible();
  await expect(page.getByRole('img', { name: /Grafico/ }).first()).toBeVisible();
  await noHorizontalScroll(page);

  await tab(page, 'Strumenti');
  await expect(page.getByRole('heading', { name: 'Strumenti' })).toBeVisible();
  await expect(page.getByRole('note')).toContainText('credenziali in chiaro');
  await noHorizontalScroll(page);

  await tab(page, 'Impostazioni');
  await expect(page.getByRole('heading', { name: 'Impostazioni' })).toBeVisible();
  await expect(page.getByRole('list', { name: 'Configurazione di cswap' })).toBeVisible();
  await noHorizontalScroll(page);
  await page.screenshot({ path: `${SHOTS}/impostazioni-420x640.png`, fullPage: true });

  await tab(page, 'Account');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
});

test('rimuovere un account chiede conferma e lo toglie dalla lista', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: 'Gestisci l’account alias1' }).click();
  const panel = page.getByRole('group', { name: 'Gestione di alias1' });
  await panel.getByRole('button', { name: 'Rimuovi' }).click();
  const dialog = page.getByRole('alertdialog');
  await expect(dialog).toContainText('Rimuovere alias1?');

  // Annullando l'account resta.
  await dialog.getByRole('button', { name: 'Annulla' }).click();
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
  await expect(page.getByRole('list', { name: 'Account' }).getByRole('listitem')).toHaveCount(5);

  await panel.getByRole('button', { name: 'Rimuovi' }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'Rimuovi' }).click();
  await expect(page.getByRole('list', { name: 'Account' }).getByRole('listitem')).toHaveCount(4);
  await expect(page.getByText('alias1', { exact: true })).toHaveCount(0);
});

test('purge: doppia conferma con la parola, poi non resta alcun account', async ({ page }) => {
  await open(page);
  await tab(page, 'Strumenti');
  await page.getByRole('button', { name: 'Elimina tutti i dati…' }).click();
  const dialog = page.getByRole('alertdialog');
  await expect(dialog).toContainText('Passo 1 di 2');
  await dialog.getByRole('button', { name: 'Continua' }).click();
  await expect(dialog).toContainText('Passo 2 di 2');
  const confirm = dialog.getByRole('button', { name: 'Elimina tutti i dati…' });
  await expect(confirm).toBeDisabled();
  await dialog.getByLabel('Parola di conferma').fill('purge');
  await confirm.click();
  await expect(page.getByText('Dati di cswap eliminati.')).toBeVisible();
  await tab(page, 'Account');
  await expect(page.getByText('Nessun account gestito')).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/purge-eseguito.png`, fullPage: true });
});

test('regole: preset come cswap-guard, salvataggio e conflitto ignorato', async ({ page }) => {
  await open(page);
  await tab(page, 'Regole');
  await page.getByRole('button', { name: 'Preset come cswap-guard' }).click();
  await expect(page.getByLabel('Account principale')).toHaveValue('6');
  await expect(page.getByRole('status').filter({ hasText: 'Preset applicato' })).toBeVisible();
  await page.getByLabel('Solo osservazione').check();
  await page.getByRole('button', { name: 'Salva' }).click();
  await expect(page.getByText('Regole salvate.')).toBeVisible();
  await page
    .getByRole('button', { name: 'Ignora il conflitto Attività pianificata cswap-guard' })
    .click();
  await expect(page.getByRole('status', { name: 'Stato del motore' })).toContainText(
    'Motore acceso',
  );
  await page.screenshot({ path: `${SHOTS}/regole-420x640.png`, fullPage: true });
});
