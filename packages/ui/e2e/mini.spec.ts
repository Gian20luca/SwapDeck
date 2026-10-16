import { expect, test, type Page } from '@playwright/test';

const SHOTS = 'e2e/screenshots';

async function openMini(page: Page, engine?: 'on' | 'off' | 'observe', theme?: 'dark') {
  await page.setViewportSize({ width: 320, height: 420 });
  await page.goto('/');
  await page.getByLabel('Vista').selectOption('mini');
  if (engine) await page.getByLabel('Motore (demo)').selectOption(engine);
  if (theme) await page.getByLabel('Tema').selectOption(theme);
  const mini = page.getByRole('region', { name: 'Mini-widget account attivo' });
  await expect(mini.getByText('alias6')).toBeVisible();
  return mini;
}

async function noOverflow(mini: ReturnType<Page['getByRole']>) {
  const inner = await mini.evaluate((el) => ({
    x: el.scrollWidth - el.clientWidth,
    y: el.scrollHeight - el.clientHeight,
  }));
  expect(inner.x).toBeLessThanOrEqual(0);
  expect(inner.y).toBeLessThanOrEqual(0);
}

test('mini-widget esteso: limiti per modello, ora dell’ultimo aggiornamento e reset con data', async ({
  page,
}) => {
  const mini = await openMini(page, 'on');
  await expect(mini.getByRole('progressbar', { name: /^Fable di alias6/ })).toBeVisible();
  await expect(mini.getByText(/^Aggiornato alle \d\d:\d\d:\d\d$/)).toBeVisible();
  // «Reset tra 4h 53m · ven 16/10 15:12» oppure «… · oggi 15:12».
  await expect(
    mini.getByText(/^Reset tra .+ · (oggi|domani|\w{3} \d\d\/\d\d) \d\d:\d\d$/).first(),
  ).toBeVisible();
  const box = await mini.boundingBox();
  expect(box!.width).toBeLessThanOrEqual(280);
  // Più alto dei 160 px di base per far posto a Fable e all'ora, ma entro il tetto dell'host (400).
  expect(box!.height).toBeLessThanOrEqual(300);
  await noOverflow(mini);
  await page.screenshot({
    path: `${SHOTS}/mini-widget-esteso.png`,
    clip: { x: 0, y: 0, width: 320, height: 420 },
  });
});

test('mini-widget compresso: striscia 280×28 senza overflow, colore della soglia, clic per espandere', async ({
  page,
}) => {
  const mini = await openMini(page, 'on');
  await page.getByRole('button', { name: 'Comprimi in una striscia' }).click();
  const strip = page.getByRole('region', { name: 'Mini-widget account attivo' });
  await expect(strip).toHaveClass(/sd-mini--compact/);
  await expect(strip.getByText('alias6')).toBeVisible();
  await expect(strip.getByText('5 ore 66%')).toHaveAttribute('data-level', 'ok');
  const box = await strip.boundingBox();
  expect(box!.width).toBeLessThanOrEqual(280);
  expect(box!.height).toBeLessThanOrEqual(28);
  await noOverflow(strip);
  await expect(strip.getByRole('progressbar')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Espandi il mini-widget' })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Nascondi il mini-widget/ })).toBeVisible();
  await page.screenshot({
    path: `${SHOTS}/mini-widget-striscia.png`,
    clip: { x: 0, y: 0, width: 320, height: 130 },
  });
  // Un clic sulla striscia la riespande.
  await strip.click({ position: { x: 60, y: 14 } });
  await expect(mini.getByRole('progressbar', { name: /^5 ore di alias6/ })).toBeVisible();
});

test('mini-widget compresso, tema scuro: nessun overflow', async ({ page }) => {
  await openMini(page, 'on', 'dark');
  await page.getByRole('button', { name: 'Comprimi in una striscia' }).click();
  const strip = page.getByRole('region', { name: 'Mini-widget account attivo' });
  await expect(strip).toHaveClass(/sd-mini--compact/);
  await noOverflow(strip);
  await page.screenshot({
    path: `${SHOTS}/mini-widget-striscia-scura.png`,
    clip: { x: 0, y: 0, width: 320, height: 130 },
  });
});

test('pulsante Aggiorna: stato «in aggiornamento» e ritorno alla quiete', async ({ page }) => {
  const mini = await openMini(page, 'on');
  const refresh = mini.getByRole('button', { name: /^Aggiorna ora/ });
  await expect(refresh).toHaveAttribute('title', /ogni 3 minuti/);
  await refresh.click();
  await expect(mini.getByRole('button', { name: 'Aggiornamento in corso…' })).toHaveAttribute(
    'aria-busy',
    'true',
  );
  await page.screenshot({
    path: `${SHOTS}/mini-widget-aggiorna.png`,
    clip: { x: 0, y: 0, width: 320, height: 120 },
  });
  await expect(mini.getByRole('button', { name: /^Aggiorna ora/ })).not.toHaveAttribute(
    'aria-busy',
    'true',
  );
});

test('trattini del motore: rosso se acceso, tooltip con mouse e tastiera', async ({ page }) => {
  const mini = await openMini(page, 'on');
  const five = mini.getByRole('note', { name: 'Soglia del motore: 95%' });
  await expect(five).toHaveAttribute('data-tone', 'active');
  const color = await five.evaluate((el) => getComputedStyle(el, '::before').backgroundColor);
  expect(color).toBe('rgb(214, 69, 69)');
  // Il trattino sta al 95% della barra.
  const rail = five.locator('xpath=..');
  const [railBox, markBox] = [await rail.boundingBox(), await five.boundingBox()];
  const center = markBox!.x + markBox!.width / 2;
  expect((center - railBox!.x) / railBox!.width).toBeCloseTo(0.95, 1);
  await expect(page.getByRole('tooltip').filter({ hasText: /^A 95%/ })).toBeHidden();
  await five.hover();
  const tip = page.getByRole('tooltip').filter({
    hasText: /^A 95% il motore passa a un’altra utenza \(riserva con la settimanale più bassa\)$/,
  });
  await expect(tip).toBeVisible();
  await page.screenshot({
    path: `${SHOTS}/mini-widget-trattino-acceso.png`,
    clip: { x: 0, y: 0, width: 320, height: 420 },
  });
  // Tastiera: focus sul trattino dei 7 giorni.
  await page.mouse.move(0, 0);
  const seven = mini.getByRole('note', { name: 'Soglia del motore: 98%' });
  await seven.focus();
  await expect(page.getByRole('tooltip').filter({ hasText: /^A 98%/ })).toBeVisible();
  // Il ritmo atteso ha il suo trattino e il suo tooltip.
  const pace = mini.getByRole('note', { name: /^Ritmo atteso/ }).first();
  await expect(pace).toHaveAttribute('data-kind', 'pace');
  await pace.hover();
  await expect(
    page
      .getByRole('tooltip')
      .filter({ hasText: 'Consumo atteso a quest’ora per arrivare al reset' })
      .first(),
  ).toBeVisible();
  // I limiti per modello non hanno soglia del motore.
  await expect(mini.getByRole('note', { name: /^Soglia del motore/ })).toHaveCount(2);
});

test('trattini del motore: grigi se spento, in osservazione o sospeso', async ({ page }) => {
  const mini = await openMini(page, 'off');
  const five = mini.getByRole('note', { name: 'Soglia del motore: 95%' });
  await expect(five).toHaveAttribute('data-tone', 'muted');
  await five.hover();
  await expect(
    page
      .getByRole('tooltip')
      .filter({ hasText: 'Soglia 95%: nessun cambio automatico, motore spento' }),
  ).toBeVisible();
  await page.screenshot({
    path: `${SHOTS}/mini-widget-trattino-spento.png`,
    clip: { x: 0, y: 0, width: 320, height: 420 },
  });

  await page.getByLabel('Motore (demo)').selectOption('observe');
  await expect(mini.getByText('alias6')).toBeVisible();
  await mini.getByRole('note', { name: 'Soglia del motore: 95%' }).focus();
  await expect(
    page.getByRole('tooltip').filter({ hasText: /in sola osservazione: registra soltanto$/ }),
  ).toBeVisible();

  await page.getByLabel('Motore (demo)').selectOption('conflict');
  await expect(mini.getByText('alias6')).toBeVisible();
  await mini.getByRole('note', { name: 'Soglia del motore: 95%' }).focus();
  await expect(
    page.getByRole('tooltip').filter({ hasText: /sospeso: altro motore attivo$/ }),
  ).toBeVisible();
});

test('dashboard: trattini del motore sull’account attivo, ora del dato e reset con data', async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 900 });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
  await page.getByLabel('Motore (demo)').selectOption('on');
  const active = page
    .getByRole('listitem')
    .filter({ has: page.getByText('alias6', { exact: true }) });
  await expect(active.getByRole('note', { name: 'Soglia del motore: 95%' })).toBeVisible();
  await expect(active.getByRole('note', { name: 'Soglia del motore: 98%' })).toBeVisible();
  // Solo l'account attivo ha i trattini.
  const other = page
    .getByRole('listitem')
    .filter({ has: page.getByText('alias3', { exact: true }) });
  await expect(other.getByRole('note', { name: /^Soglia del motore/ })).toHaveCount(0);
  await expect(active.getByText(/alle \d\d:\d\d:\d\d/)).toBeVisible();
  await expect(active.getByText(/^Reset tra .+ · /).first()).toBeVisible();
  await active.getByRole('note', { name: 'Soglia del motore: 95%' }).hover();
  await page.screenshot({ path: `${SHOTS}/dashboard-trattini.png`, fullPage: true });
});

test('dashboard: «Storico» porta alla scheda Storico con quell’account aperto', async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 640 });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1, name: 'Account' })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Mostra lo storico/ })).toHaveCount(0);
  await page.getByRole('button', { name: 'Apri lo storico di alias3' }).click();
  await expect(page.getByRole('tab', { name: 'Storico' })).toHaveAttribute('aria-selected', 'true');
  const toggle = page.getByRole('button', { name: 'Nascondi lo storico di alias3' });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(toggle).toBeFocused();
  await expect(toggle).toBeInViewport();
  await expect(page.getByRole('button', { name: 'Mostra lo storico di alias6' })).toHaveAttribute(
    'aria-expanded',
    'false',
  );
  await page.screenshot({ path: `${SHOTS}/storico-da-account.png` });
});
