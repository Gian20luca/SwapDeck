import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { PaceIndicator } from './PaceIndicator.js';

describe('PaceIndicator', () => {
  it('non rende nulla senza expectedPct', () => {
    const { container } = render(<PaceIndicator expectedPct={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('in linea e dura fino al reset', () => {
    render(<PaceIndicator expectedPct={38.9} aheadOfPace={false} willLastToReset />);
    expect(screen.getByText('In linea col ritmo')).toBeInTheDocument();
    expect(screen.getByText(/Atteso 39%/)).toBeInTheDocument();
    expect(screen.getByText('Dovrebbe durare fino al reset')).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Ritmo' })).toHaveAttribute('data-tone', 'ok');
  });

  it('in anticipo che si esaurisce prima del reset mostra la stima', () => {
    render(
      <PaceIndicator
        expectedPct={25.3}
        aheadOfPace
        willLastToReset={false}
        projectedExhaustionAt="2026-10-16T10:26:19Z"
      />,
    );
    expect(screen.getByText('In anticipo sul ritmo')).toBeInTheDocument();
    expect(
      screen.getByText(/Si esaurisce prima del reset · Esaurimento stimato/),
    ).toBeInTheDocument();
    expect(screen.getByRole('group')).toHaveAttribute('data-tone', 'danger');
  });

  it('in anticipo ma che dura: tono di avviso', () => {
    render(<PaceIndicator expectedPct={20} aheadOfPace willLastToReset />);
    expect(screen.getByRole('group')).toHaveAttribute('data-tone', 'warn');
  });
});
