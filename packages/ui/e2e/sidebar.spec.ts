import { expect, test, type Page } from '@playwright/test';

/** Larghezza tipica della barra laterale di VS Code (la vista SwapDeck dell'estensione). */
const WIDTH = 280;
const SHOTS = 'e2e/screenshots';

/**
 * Niente scroll orizzontale: né della pagina né di un contenitore interno, e nessun elemento
 * visibile che esca dal bordo destro.
 */
async function noHorizontalScroll(page: Page, where: string) {
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
  expect(problems, where).toEqual([]);
}

async function tab(page: Page, name: string) {
  await page.getByRole('tab', { name }).click();
  await expect(page.getByRole('tab', { name })).toHaveAttribute('aria-selected', 'true');
}

test(`app completa a ${WIDTH} px (barra laterale di VS Code) senza scroll orizzontale`, async ({
  page,
}) => {
  test.setTimeout(90_000); // prima apertura: Vite prepara le dipendenze
  await page.setViewportSize({ width: WIDTH, height: 800 });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
  // Tutte le schede raggiungibili (vanno a capo, nessuna nascosta oltre il bordo).
  for (const name of ['Account', 'Cartelle', 'Regole', 'Storico', 'Strumenti', 'Impostazioni']) {
    await expect(page.getByRole('tab', { name })).toBeInViewport();
  }
  await noHorizontalScroll(page, 'Account');
  await page.screenshot({ path: `${SHOTS}/sidebar-account-${WIDTH}.png` });

  await page.getByRole('button', { name: 'Gestisci l’account alias1' }).click();
  await expect(page.getByRole('region', { name: 'Gestione di alias1' })).toBeVisible();
  await noHorizontalScroll(page, 'dettaglio account');
  await page.getByRole('button', { name: /Indietro/ }).click();

  await tab(page, 'Cartelle');
  await expect(page.getByRole('heading', { name: 'Cartelle mappate' })).toBeVisible();
  await noHorizontalScroll(page, 'Cartelle');

  await tab(page, 'Regole');
  await expect(page.getByRole('heading', { name: 'Regole del motore' })).toBeVisible();
  await noHorizontalScroll(page, 'Regole');
  await page.screenshot({ path: `${SHOTS}/sidebar-regole-${WIDTH}.png`, fullPage: true });

  await tab(page, 'Storico');
  await expect(page.getByRole('heading', { name: 'Storico' })).toBeVisible();
  await expect(page.getByRole('img', { name: /Grafico/ }).first()).toBeVisible();
  await noHorizontalScroll(page, 'Storico');

  await tab(page, 'Strumenti');
  await expect(page.getByRole('heading', { name: 'Strumenti' })).toBeVisible();
  await noHorizontalScroll(page, 'Strumenti');

  await tab(page, 'Impostazioni');
  await expect(page.getByRole('heading', { name: 'Impostazioni' })).toBeVisible();
  await noHorizontalScroll(page, 'Impostazioni');

  await page.getByRole('button', { name: 'Notifiche…' }).click();
  await expect(page.getByRole('region', { name: 'Notifiche' })).toBeVisible();
  await page.getByLabel('Non disturbare a orari').check();
  await noHorizontalScroll(page, 'Notifiche');
  await page.screenshot({ path: `${SHOTS}/sidebar-notifiche-${WIDTH}.png`, fullPage: true });
});
