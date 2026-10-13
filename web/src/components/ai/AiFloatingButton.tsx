import { Sparkles } from 'lucide-react';

type Props = {
  onClick: () => void;
  isOpen: boolean;
};

export function AiFloatingButton({ onClick, isOpen }: Props) {
  if (isOpen) return null;
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="Open AI Assistant"
      className="fixed bottom-5 right-5 z-30 flex h-12 w-12 items-center justify-center rounded-lg bg-[var(--color-action)] text-white shadow-lg transition-all hover:bg-[var(--color-action-hover)] hover:scale-105 active:scale-95"
    >
      <Sparkles size={24} />
    </button>
  );
}
