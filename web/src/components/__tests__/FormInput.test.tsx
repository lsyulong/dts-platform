import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { FormInput } from '../ui/FormInput';

describe('FormInput', () => {
  it('renders text input', () => {
    render(<FormInput placeholder="Enter name" />);
    expect(screen.getByPlaceholderText('Enter name')).toBeInTheDocument();
  });

  it('renders label when provided', () => {
    render(<FormInput label="Name" />);
    expect(screen.getByText('Name')).toBeInTheDocument();
  });

  it('shows required asterisk when required', () => {
    render(<FormInput label="Name" required />);
    const asterisk = screen.getByText('*');
    expect(asterisk).toBeInTheDocument();
    expect(asterisk).toHaveClass('text-red-400');
  });

  it('shows error message when error prop provided', () => {
    render(<FormInput label="Name" error="This field is required" />);
    expect(screen.getByText('This field is required')).toBeInTheDocument();
  });

  it('applies error border style when error present', () => {
    const { container } = render(<FormInput error="Invalid" />);
    const input = container.querySelector('input');
    expect(input?.className).toContain('border-red-500');
  });

  it('calls onChange with string value', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<FormInput placeholder="Name" onChange={onChange} />);
    await user.type(screen.getByPlaceholderText('Name'), 'hello');
    expect(onChange).toHaveBeenCalled();
  });

  it('renders textarea when type is textarea', () => {
    const { container } = render(<FormInput type="textarea" placeholder="Description" />);
    expect(container.querySelector('textarea')).toBeInTheDocument();
  });

  it('renders password input', () => {
    const { container } = render(<FormInput type="password" placeholder="Password" />);
    const input = container.querySelector('input[type="password"]');
    expect(input).toBeInTheDocument();
  });

  it('disables input when disabled prop is true', () => {
    render(<FormInput placeholder="Cannot edit" disabled />);
    expect(screen.getByPlaceholderText('Cannot edit')).toBeDisabled();
  });

  it('renders number input', () => {
    const { container } = render(<FormInput type="number" value={3306} />);
    const input = container.querySelector('input[type="number"]');
    expect(input).toBeInTheDocument();
  });

  it('does not render label when not provided', () => {
    const { container } = render(<FormInput placeholder="No label" />);
    expect(container.querySelector('label')).not.toBeInTheDocument();
  });

  it('does not render error message when no error', () => {
    const { container } = render(<FormInput />);
    expect(container.querySelector('.text-xs.text-red-400')).not.toBeInTheDocument();
  });

  it('applies additional className', () => {
    const { container } = render(<FormInput className="w-24" />);
    const input = container.querySelector('input');
    expect(input?.className).toContain('w-24');
  });
});
