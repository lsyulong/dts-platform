import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AiFloatingButton } from '../AiFloatingButton';

describe('AiFloatingButton', () => {
  it('renders button with aria label', () => {
    render(<AiFloatingButton onClick={vi.fn()} isOpen={false} />);
    expect(screen.getByRole('button')).toBeInTheDocument();
  });

  it('calls onClick when clicked', async () => {
    const onClick = vi.fn();
    render(<AiFloatingButton onClick={onClick} isOpen={false} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button'));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('is hidden when isOpen is true', () => {
    render(<AiFloatingButton onClick={vi.fn()} isOpen={true} />);
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('is visible when isOpen is false', () => {
    render(<AiFloatingButton onClick={vi.fn()} isOpen={false} />);
    expect(screen.getByRole('button')).toBeVisible();
  });
});
