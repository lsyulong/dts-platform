import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusBadge } from '../ui/StatusBadge';

describe('StatusBadge', () => {
  it('renders with status text from i18n mock', () => {
    render(<StatusBadge status="running" />);
    expect(screen.getByText('tasks.status.running')).toBeInTheDocument();
  });

  it('renders custom label when provided', () => {
    render(<StatusBadge status="running" label="In Progress" />);
    expect(screen.getByText('In Progress')).toBeInTheDocument();
  });

  it.each(['active', 'disabled'])('renders the account status label for %s', (status) => {
    render(<StatusBadge status={status} />);
    expect(screen.getByText(`system.statusLabels.${status}`)).toBeInTheDocument();
    expect(screen.queryByText(`tasks.status.${status}`)).not.toBeInTheDocument();
  });

  it.each(['active', 'disabled'])('keeps an explicit label for account status %s', (status) => {
    render(<StatusBadge status={status} label="Custom account label" />);
    expect(screen.getByText('Custom account label')).toBeInTheDocument();
    expect(screen.queryByText(`system.statusLabels.${status}`)).not.toBeInTheDocument();
  });

  it('applies correct color for draft status', () => {
    const { container } = render(<StatusBadge status="draft" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-[var(--color-bg-tertiary)]');
    expect(badge?.className).toContain('text-[var(--color-text-secondary)]');
  });

  it('applies correct color for running status', () => {
    const { container } = render(<StatusBadge status="running" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-[var(--color-accent-subtle)]');
    expect(badge?.className).toContain('text-[var(--color-accent)]');
  });

  it('applies correct color for completed status', () => {
    const { container } = render(<StatusBadge status="completed" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-emerald-500/10');
    expect(badge?.className).toContain('text-[var(--color-success)]');
  });

  it('applies correct color for failed status', () => {
    const { container } = render(<StatusBadge status="failed" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-rose-500/10');
    expect(badge?.className).toContain('text-[var(--color-error)]');
  });

  it('applies correct color for paused status', () => {
    const { container } = render(<StatusBadge status="paused" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-amber-500/10');
    expect(badge?.className).toContain('text-[var(--color-warning)]');
  });

  it('applies correct color for healthy status', () => {
    const { container } = render(<StatusBadge status="healthy" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('text-[var(--color-success)]');
  });

  it('applies correct color for unhealthy status', () => {
    const { container } = render(<StatusBadge status="unhealthy" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('text-[var(--color-error)]');
  });

  it('falls back to default color for unknown status', () => {
    const { container } = render(<StatusBadge status="unknown_status" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('text-[var(--color-text-secondary)]');
  });

  it('uses sm size by default', () => {
    const { container } = render(<StatusBadge status="draft" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('text-xs');
  });

  it('uses md size when specified', () => {
    const { container } = render(<StatusBadge status="draft" size="md" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('text-sm');
  });
});
