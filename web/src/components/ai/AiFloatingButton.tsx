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
      className="fixed bottom-6 right-6 z-50 flex h-14 w-14 items-center justify-center rounded-2xl bg-violet-600 text-white shadow-lg shadow-violet-500/30 transition-all hover:bg-violet-500 hover:scale-105 active:scale-95"
    >
      <Sparkles size={24} />
    </button>
  );
}
