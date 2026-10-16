import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend } from '../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../dev/testUtils.js';
import { MiniWidget } from './MiniWidget.js';

describe('Mini-widget: mappa attiva', () => {
  it('cambia mappa dal selettore, senza aprire la finestra principale', async () => {
    const backend = new FakeBackend();
    const apply = vi.spyOn(backend, 'applyEngineProfile');
    const user = userEvent.setup();
    renderWith(<MiniWidget />, backend);
    const select = await screen.findByLabelText('Mappa');
    expect(select).toHaveValue('personal');
    await user.selectOptions(select, 'eco');
    await waitFor(() => expect(apply).toHaveBeenCalledWith({ op: 'select', id: 'eco' }));
    await waitFor(() => expect(screen.getByLabelText('Mappa')).toHaveValue('eco'));
    expect((await backend.getEngineProfiles()).activeId).toBe('eco');
  });

  it('host senza mappe: nessun selettore', async () => {
    renderWith(<MiniWidget />, bareBackend(new FakeBackend()));
    await screen.findByText(/5 ore/);
    expect(screen.queryByLabelText('Mappa')).toBeNull();
  });
});
