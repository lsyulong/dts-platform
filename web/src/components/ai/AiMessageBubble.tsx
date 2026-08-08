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
          isUser ? 'bg-blue-500/10 text-blue-400' : 'bg-violet-500/10 text-violet-400'
        }`}
      >
        {isUser ? <User size={16} /> : <Bot size={16} />}
      </div>
      <div
        className={`max-w-[80%] rounded-2xl px-4 py-2.5 text-sm ${
          isUser
            ? 'bg-blue-600 text-white'
            : 'bg-[var(--color-bg-secondary)] text-[var(--color-text-primary)] border border-[var(--color-border-default)]'
        }`}
      >
        <p className="whitespace-pre-wrap">{content}</p>
      </div>
    </div>
  );
}
