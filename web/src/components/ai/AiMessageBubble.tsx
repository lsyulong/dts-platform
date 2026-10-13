import { Bot, User } from 'lucide-react';

type Props = {
  role: 'user' | 'assistant';
  content: string;
};

export function AiMessageBubble({ role, content }: Props) {
  const isUser = role === 'user';
  return (
    <div className={`flex gap-3 ${isUser ? 'flex-row-reverse' : ''}`}>
      <div
        className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-lg ${
          isUser
            ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
            : 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
        }`}
      >
        {isUser ? <User size={16} /> : <Bot size={16} />}
      </div>
      <div
        className={`max-w-[80%] rounded-lg px-4 py-2.5 text-sm ${
          isUser
            ? 'bg-[var(--color-action)] text-white'
            : 'bg-[var(--color-bg-secondary)] text-[var(--color-text-primary)] border border-[var(--color-border-default)]'
        }`}
      >
        <p className="whitespace-pre-wrap">{content}</p>
      </div>
    </div>
  );
}
