import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { LoadingSpinner } from '../ui/LoadingSpinner';

describe('LoadingSpinner', () => {
  it('renders with default size (md)', () => {
    const { container } = render(<LoadingSpinner />);
    const spinner = container.querySelector('.animate-spin');
    expect(spinner).toBeInTheDocument();
  });

  it('renders sm size', () => {
    const { container } = render(<LoadingSpinner size="sm" />);
    const spinner = container.querySelector('.h-6.w-6');
    expect(spinner).toBeInTheDocument();
  });

  it('renders md size', () => {
    const { container } = render(<LoadingSpinner size="md" />);
    const spinner = container.querySelector('.h-8.w-8');
    expect(spinner).toBeInTheDocument();
  });

  it('renders lg size', () => {
    const { container } = render(<LoadingSpinner size="lg" />);
    const spinner = container.querySelector('.h-12.w-12');
    expect(spinner).toBeInTheDocument();
  });

  it('has animate-spin class', () => {
    const { container } = render(<LoadingSpinner />);
    const spinner = container.querySelector('.animate-spin');
    expect(spinner).toBeInTheDocument();
  });

  it('wraps in flex centering container', () => {
    const { container } = render(<LoadingSpinner />);
    const wrapper = container.querySelector('.flex.justify-center');
    expect(wrapper).toBeInTheDocument();
  });
});
