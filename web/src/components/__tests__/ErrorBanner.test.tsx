import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ErrorBanner } from '../ui/ErrorBanner';

describe('ErrorBanner', () => {
  it('renders the error message', () => {
    render(<ErrorBanner message="Something failed" />);
    expect(screen.getByText('Something failed')).toBeInTheDocument();
  });

  it('renders retry button when onRetry is provided', () => {
    const onRetry = vi.fn();
    render(<ErrorBanner message="Failed" onRetry={onRetry} />);
    const button = screen.getByText('common.retry');
    expect(button).toBeInTheDocument();
  });

  it('calls onRetry when retry button clicked', async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    render(<ErrorBanner message="Failed" onRetry={onRetry} />);
    await user.click(screen.getByText('common.retry'));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it('renders dismiss button when onDismiss is provided', () => {
    const onDismiss = vi.fn();
    const { container } = render(<ErrorBanner message="Failed" onDismiss={onDismiss} />);
    expect(container.querySelector('[aria-label="common.close"]')).toBeInTheDocument();
  });

  it('does not render retry button when onRetry is not provided', () => {
    render(<ErrorBanner message="Failed" />);
    expect(screen.queryByText('common.retry')).not.toBeInTheDocument();
  });

  it('renders both retry and dismiss buttons together', () => {
    const onRetry = vi.fn();
    const onDismiss = vi.fn();
    render(<ErrorBanner message="Failed" onRetry={onRetry} onDismiss={onDismiss} />);
    expect(screen.getByText('common.retry')).toBeInTheDocument();
    expect(screen.getByText('✕')).toBeInTheDocument();
  });
});
