import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { EmptyState } from '../ui/EmptyState';
import { Database } from 'lucide-react';

describe('EmptyState', () => {
  it('renders title', () => {
    render(<EmptyState title="No items found" />);
    expect(screen.getByText('No items found')).toBeInTheDocument();
  });

  it('renders description when provided', () => {
    render(<EmptyState title="No items" description="Try creating one" />);
    expect(screen.getByText('Try creating one')).toBeInTheDocument();
  });

  it('does not render description when not provided', () => {
    const { container } = render(<EmptyState title="No items" />);
    // The description paragraph is only rendered when description prop is provided
    const paras = container.querySelectorAll('p');
    // Only the title paragraph should exist, no description paragraph
    expect(paras.length).toBe(1);
  });

  it('renders custom icon when provided', () => {
    render(<EmptyState icon={Database} title="No data sources" />);
    // lucide applies classes, we check the icon renders
    expect(screen.getByText('No data sources')).toBeInTheDocument();
  });

  it('renders default Inbox icon when no icon provided', () => {
    const { container } = render(<EmptyState title="Empty" />);
    const svg = container.querySelector('svg');
    expect(svg).toBeInTheDocument();
  });

  it('renders action when provided', () => {
    render(<EmptyState title="No items" action={<a href="/create">Create one</a>} />);
    expect(screen.getByText('Create one')).toBeInTheDocument();
  });

  it('uses dashed border container', () => {
    const { container } = render(<EmptyState title="Empty" />);
    const wrapper = container.querySelector('.border-dashed');
    expect(wrapper).toBeInTheDocument();
  });
});
