import { expect, test, type Page } from '@playwright/test';

const ACCOUNTS = ['alias1', 'alias2', 'alias3', 'alias5', 'alias6'];
const SHOTS = 'e2e/screenshots';

const card = (page: Page, name: string) =>
  page.getByRole('listitem').filter({ has: page.getByText(name, { exact: true }) });

async function loaded(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
  await expect(page.getByRole('list', { name: 'Account' }).getByRole('listitem')).toHaveCount(
    ACCOUNTS.length,
  );
}

async function noHorizontalScroll(page: Page) {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
}

test('carica la dashboard con tutti gli account delle fixture', async ({ page }) => {
  await loaded(page);
  for (const name of ACCOUNTS) await expect(card(page, name)).toHaveCount(1);
  await expect(card(page, 'alias6').getByText('Attivo', { exact: true })).toBeVisible();
  await expect(page.getByRole('progressbar').first()).toBeVisible();
  await expect(card(page, 'alias2').getByText('Login richiesto')).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/dashboard-chiara.png`, fullPage: true });
});

test('cambio di account dal pulsante aggiorna lo stato', async ({ page }) => {
  await loaded(page);
  await page.getByRole('button', { name: 'Passa a questo account: alias1' }).click();
  await expect(card(page, 'alias1').getByText('Attivo', { exact: true })).toBeVisible();
  await expect(card(page, 'alias6').getByText('Attivo', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Attivo', { exact: true })).toHaveCount(1);
  await expect(page.getByRole('status').filter({ hasText: 'alias1' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Passa a questo account: alias1' })).toBeDisabled();
});

test('cambio rapido best porta all’account con più margine', async ({ page }) => {
  await loaded(page);
  const quick = page.getByRole('group', { name: 'Cambio rapido' });
  await quick.getByRole('button', { name: 'Migliore' }).click();
  await expect(page.getByText('Attivo', { exact: true })).toHaveCount(1);
  await expect(
    page.getByRole('status').filter({ hasText: /Passato all’account|Nessun cambio/ }),
  ).toBeVisible();
  await expect(quick.getByRole('button', { name: 'Migliore' })).toBeEnabled();
});

test('ConfirmDialog semplice: annulla e conferma', async ({ page }) => {
  await loaded(page);
  await page.getByRole('button', { name: 'Demo conferma semplice' }).click();
  const dialog = page.getByRole('alertdialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('heading', { name: 'Rimuovere l’account?' })).toBeVisible();
  await expect(dialog.getByRole('textbox')).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Annulla' }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByText('Annullato')).toBeVisible();

  await page.getByRole('button', { name: 'Demo conferma semplice' }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'Conferma' }).click();
  await expect(page.getByRole('alertdialog')).toBeHidden();
  await expect(page.getByText('Confermato: simple')).toBeVisible();
});

test('ConfirmDialog con doppia conferma a parola', async ({ page }) => {
  await loaded(page);
  await page.getByRole('button', { name: 'Demo doppia conferma' }).click();
  const dialog = page.getByRole('alertdialog');
  await expect(dialog.getByText('Passo 1 di 2')).toBeVisible();
  await dialog.getByRole('button', { name: 'Continua' }).click();
  await expect(dialog.getByText('Passo 2 di 2')).toBeVisible();
  const word = dialog.getByRole('textbox', { name: 'Parola di conferma' });
  const confirm = dialog.getByRole('button', { name: 'Conferma' });
  await expect(confirm).toBeDisabled();
  await word.fill('Purge');
  await expect(confirm).toBeDisabled();
  await word.fill('purge');
  await expect(confirm).toBeEnabled();
  await confirm.click();
  await expect(dialog).toBeHidden();
  await expect(page.getByText('Confermato: typed')).toBeVisible();
});

test('Esc annulla il dialogo', async ({ page }) => {
  await loaded(page);
  await page.getByRole('button', { name: 'Demo doppia conferma' }).click();
  await expect(page.getByRole('alertdialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('alertdialog')).toBeHidden();
});

test('cambio di lingua it/en', async ({ page }) => {
  await loaded(page);
  await page.getByLabel('Lingua').selectOption('en');
  await expect(page.getByRole('heading', { level: 1, name: 'Accounts' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Refresh' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Switch to this account: alias1' })).toBeVisible();
  await expect(page.getByText('Active', { exact: true })).toHaveCount(1);
  await page.getByLabel('Language').selectOption('it');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
});

test('tema scuro', async ({ page }) => {
  await loaded(page);
  const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const light = await bg();
  await page.getByLabel('Tema').selectOption('dark');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  expect(await bg()).not.toBe(light);
  await page.screenshot({ path: `${SHOTS}/dashboard-scura.png`, fullPage: true });
});

test('mini-widget a 280×160 senza overflow', async ({ page }) => {
  await page.setViewportSize({ width: 280, height: 160 });
  await page.goto('/');
  await page.getByLabel('Vista').selectOption('mini');
  const mini = page.getByRole('region', { name: 'Mini-widget account attivo' });
  await expect(mini).toBeVisible();
  await expect(mini.getByText('alias6')).toBeVisible();
  const box = await mini.boundingBox();
  expect(box!.width).toBeLessThanOrEqual(280);
  expect(box!.height).toBeLessThanOrEqual(160);
  const inner = await mini.evaluate((el) => ({
    x: el.scrollWidth - el.clientWidth,
    y: el.scrollHeight - el.clientHeight,
  }));
  expect(inner.x).toBeLessThanOrEqual(0);
  expect(inner.y).toBeLessThanOrEqual(0);
  await mini.screenshot({ path: `${SHOTS}/mini-widget.png` });
});

test('finestra 420×640 senza scroll orizzontale', async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 640 });
  await loaded(page);
  await noHorizontalScroll(page);
  await page.getByLabel('Tema').selectOption('dark');
  await noHorizontalScroll(page);
  await page.getByRole('button', { name: 'Mostra lo storico di alias6' }).click();
  await expect(page.getByRole('img', { name: /Grafico/ }).first()).toBeVisible();
  await noHorizontalScroll(page);
  await page.screenshot({ path: `${SHOTS}/finestra-420x640.png`, fullPage: true });
});
