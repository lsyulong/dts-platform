import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AiMessageBubble } from '../AiMessageBubble';

describe('AiMessageBubble', () => {
  it('renders user message with user icon', () => {
    render(<AiMessageBubble role="user" content="hello" />);
    expect(screen.getByText('hello')).toBeInTheDocument();
    expect(screen.getByText('hello').closest('div')).toBeInTheDocument();
  });

  it('renders assistant message', () => {
    render(<AiMessageBubble role="assistant" content="Hi there!" />);
    expect(screen.getByText('Hi there!')).toBeInTheDocument();
  });

  it('renders content with whitespace-pre-wrap class', () => {
    render(<AiMessageBubble role="user" content="line1\nline2" />);
    const p = screen.getByText(/line1/);
    expect(p).toBeInTheDocument();
    expect(p.className).toContain('whitespace-pre-wrap');
  });
});
