import { useState } from 'react';
import { Send } from 'lucide-react';

type Props = {
  onSend: (message: string) => void;
  disabled?: boolean;
};

export function AiChatInput({ onSend, disabled }: Props) {
  const [input, setInput] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || disabled) return;
    onSend(input.trim());
    setInput('');
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="flex gap-2 border-t border-[var(--color-border-default)] p-4"
    >
      <input
        type="text"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        placeholder="What would you like to know?"
        disabled={disabled}
        className="flex-1 rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] px-4 py-2.5 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-disabled)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)]"
      />
      <button
        type="submit"
        disabled={disabled || !input.trim()}
        aria-label="Send message"
        className="flex h-10 w-10 items-center justify-center rounded-lg bg-[var(--color-action)] text-white transition-colors hover:bg-[var(--color-action-hover)] disabled:opacity-50"
      >
        <Send size={16} />
      </button>
    </form>
  );
}
