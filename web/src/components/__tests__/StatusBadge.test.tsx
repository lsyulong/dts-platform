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

  it('applies correct color for draft status', () => {
    const { container } = render(<StatusBadge status="draft" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-[var(--color-bg-tertiary)]');
    expect(badge?.className).toContain('text-[var(--color-text-secondary)]');
  });

  it('applies correct color for running status', () => {
    const { container } = render(<StatusBadge status="running" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-blue-500/10');
    expect(badge?.className).toContain('text-blue-400');
  });

  it('applies correct color for completed status', () => {
    const { container } = render(<StatusBadge status="completed" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-emerald-500/10');
    expect(badge?.className).toContain('text-emerald-400');
  });

  it('applies correct color for failed status', () => {
    const { container } = render(<StatusBadge status="failed" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-rose-500/10');
    expect(badge?.className).toContain('text-rose-400');
  });

  it('applies correct color for paused status', () => {
    const { container } = render(<StatusBadge status="paused" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('bg-amber-500/10');
    expect(badge?.className).toContain('text-amber-400');
  });

  it('applies correct color for healthy status', () => {
    const { container } = render(<StatusBadge status="healthy" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('text-emerald-400');
  });

  it('applies correct color for unhealthy status', () => {
    const { container } = render(<StatusBadge status="unhealthy" />);
    const badge = container.querySelector('span');
    expect(badge?.className).toContain('text-rose-400');
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
