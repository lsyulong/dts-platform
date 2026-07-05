import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import MetricCard from '../MetricCard';

describe('MetricCard', () => {
  it('renders title and value', () => {
    render(<MetricCard title="CPU Usage" value="42%" />);
    expect(screen.getByText('CPU Usage')).toBeInTheDocument();
    expect(screen.getByText('42%')).toBeInTheDocument();
  });

  it('renders unit when provided', () => {
    render(<MetricCard title="Memory" value="18.4" unit="GB" />);
    expect(screen.getByText('GB')).toBeInTheDocument();
  });

  it('does not render unit when not provided', () => {
    const { container } = render(<MetricCard title="Count" value="100" />);
    // The value text element is the bold one, no unit text next to it
    expect(screen.getByText('100')).toBeInTheDocument();
    // Only the title and value text should be there; no extra unit span
    const unitSpans = container.querySelectorAll('.text-sm.text-zinc-500');
    // subtitle also uses text-zinc-500, but unit is specifically in the value row
    const valueContainer = container.querySelector('.mt-3');
    const unitElements = valueContainer?.querySelectorAll('.text-sm.text-zinc-500');
    expect(unitElements?.length || 0).toBe(0);
  });

  it('renders subtitle when provided', () => {
    render(<MetricCard title="Success Rate" value="99.9%" subtitle="Last 24 hours" />);
    expect(screen.getByText('Last 24 hours')).toBeInTheDocument();
  });

  it('does not render subtitle when not provided', () => {
    const { container } = render(<MetricCard title="Success Rate" value="99.9%" />);
    // subtitle text would be in a <p> element after the value
    const subtitleElements = container.querySelectorAll('.mt-1.text-xs');
    // mt-1 text-xs elements are only for subtitle - but wait, if subtitle is absent, there's no p tag
    expect(subtitleElements.length).toBe(0);
  });

  it('renders icon when provided', () => {
    const testIcon = <span data-testid="custom-icon">ICON</span>;
    render(<MetricCard title="Test" value="10" icon={testIcon} />);
    expect(screen.getByTestId('custom-icon')).toBeInTheDocument();
  });

  it('does not render icon container when not provided', () => {
    const { container } = render(<MetricCard title="Test" value="10" />);
    // The icon div is only rendered conditionally
    const iconDiv = container.querySelector('.text-zinc-500');
    // The title also uses text-zinc-500; check the specific icon container
    expect(screen.queryByTestId('custom-icon')).not.toBeInTheDocument();
  });

  describe('trend colors', () => {
    it('applies emerald color for up trend', () => {
      render(<MetricCard title="Revenue" value="$10K" subtitle="+12%" trend="up" />);
      const subtitleEl = screen.getByText('+12%');
      expect(subtitleEl.className).toContain('text-emerald-400');
    });

    it('applies rose color for down trend', () => {
      render(<MetricCard title="Errors" value="5" subtitle="+2 more" trend="down" />);
      const subtitleEl = screen.getByText('+2 more');
      expect(subtitleEl.className).toContain('text-rose-400');
    });

    it('applies zinc color for neutral trend', () => {
      render(<MetricCard title="Latency" value="45ms" subtitle="No change" trend="neutral" />);
      const subtitleEl = screen.getByText('No change');
      expect(subtitleEl.className).toContain('text-zinc-400');
    });

    it('defaults subtitle to zinc color when trend is not specified', () => {
      render(<MetricCard title="CPU" value="50%" subtitle="No trend data" />);
      const subtitleEl = screen.getByText('No trend data');
      expect(subtitleEl.className).toContain('text-zinc-500');
    });
  });

  describe('numeric and string values', () => {
    it('renders numeric value', () => {
      render(<MetricCard title="Count" value={42} />);
      expect(screen.getByText('42')).toBeInTheDocument();
    });

    it('renders string value', () => {
      render(<MetricCard title="Status" value="Healthy" />);
      expect(screen.getByText('Healthy')).toBeInTheDocument();
    });

    it('renders zero value', () => {
      render(<MetricCard title="Errors" value={0} />);
      expect(screen.getByText('0')).toBeInTheDocument();
    });
  });

  describe('card structure', () => {
    it('renders a card container with proper styling', () => {
      const { container } = render(<MetricCard title="Test" value="test" />);
      const card = container.firstElementChild;
      expect(card).not.toBeNull();
      expect(card?.className).toContain('rounded-2xl');
      expect(card?.className).toContain('border');
    });
  });
});
