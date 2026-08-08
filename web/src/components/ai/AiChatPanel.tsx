import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';
import { AiMessageBubble } from './AiMessageBubble';
import { AiChatInput } from './AiChatInput';

type Message = {
  role: 'user' | 'assistant';
  content: string;
};

type Props = {
  messages: Message[];
  onSend: (message: string) => void;
  onClose: () => void;
  isLoading: boolean;
};

export function AiChatPanel({ messages, onSend, onClose, isLoading }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  return (
    <div className="fixed bottom-6 right-6 z-50 flex h-[600px] w-[420px] flex-col rounded-2xl border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] shadow-2xl">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-[var(--color-border-default)] px-5 py-4">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-violet-500/10">
            <span className="text-lg">🤖</span>
          </div>
          <div>
            <h3 className="text-sm font-semibold text-[var(--color-text-primary)]">DeerFlow AI</h3>
            <p className="text-xs text-[var(--color-text-tertiary)]">AI Data Assistant</p>
          </div>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close AI Panel"
          className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--color-text-tertiary)] transition-colors hover:bg-[var(--color-bg-secondary)]"
        >
          <X size={16} />
        </button>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-5 space-y-4">
        {messages.length === 0 && (
          <div className="flex h-full items-center justify-center text-center">
            <div>
              <p className="text-sm text-[var(--color-text-secondary)]">
                Hi! I&apos;m DeerFlow AI Assistant
              </p>
              <p className="mt-1 text-xs text-[var(--color-text-disabled)]">
                I can help you analyze data, write SQL, and check task status
              </p>
            </div>
          </div>
        )}
        {messages.map((msg, i) => (
          <AiMessageBubble key={i} role={msg.role} content={msg.content} />
        ))}
        {isLoading && (
          <div className="flex gap-3">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-violet-500/10 text-violet-400">
              🤖
            </div>
            <div className="flex items-center gap-1 px-4 py-2.5">
              <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-violet-400" />
              <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-violet-400 [animation-delay:0.15s]" />
              <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-violet-400 [animation-delay:0.3s]" />
            </div>
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {/* Input */}
      <AiChatInput onSend={onSend} disabled={isLoading} />
    </div>
  );
}
