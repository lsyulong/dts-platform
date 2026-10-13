import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';
import { AiMessageBubble } from './AiMessageBubble';
import { AiChatInput } from './AiChatInput';
import { useTranslation } from 'react-i18next';

type Message = {
  role: 'user' | 'assistant';
  content: string;
};

type Props = {
  messages: Message[];
  onSend: (message: string) => void;
  onClose: () => void;
  isLoading: boolean;
  isUnavailable?: boolean;
};

export function AiChatPanel({
  messages,
  onSend,
  onClose,
  isLoading,
  isUnavailable = false,
}: Props) {
  const { t } = useTranslation();
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  return (
    <div className="fixed bottom-3 right-3 z-40 flex h-[min(600px,calc(100dvh-1.5rem))] w-[min(420px,calc(100vw-1.5rem))] flex-col rounded-lg border border-[var(--color-border-default)] bg-[var(--color-bg-primary)] shadow-xl">
      <div className="flex items-center justify-between border-b border-[var(--color-border-default)] px-5 py-4">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-[var(--color-accent-subtle)]">
            <span className="text-lg">🤖</span>
          </div>
          <div>
            <h3 className="text-sm font-semibold text-[var(--color-text-primary)]">DeerFlow AI</h3>
            <p className="text-xs text-[var(--color-text-tertiary)]">{t('ai.subtitle')}</p>
          </div>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label={t('ai.closePanel')}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--color-text-tertiary)] transition-colors hover:bg-[var(--color-bg-secondary)]"
        >
          <X size={16} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-5 space-y-4">
        {messages.length === 0 && (
          <div className="flex h-full items-center justify-center text-center">
            <div>
              <p className="text-sm text-[var(--color-text-secondary)]">{t('ai.welcome')}</p>
              <p className="mt-1 text-xs text-[var(--color-text-disabled)]">{t('ai.welcomeSub')}</p>
            </div>
          </div>
        )}
        {messages.map((msg, i) => (
          <AiMessageBubble key={i} role={msg.role} content={msg.content} />
        ))}
        {isLoading && (
          <div className="flex gap-3">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[var(--color-accent-subtle)] text-[var(--color-accent)]">
              🤖
            </div>
            <div className="flex items-center gap-1 px-4 py-2.5">
              <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-[var(--color-accent)]" />
              <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-[var(--color-accent)] [animation-delay:0.15s]" />
              <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-[var(--color-accent)] [animation-delay:0.3s]" />
            </div>
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {isUnavailable && (
        <p role="status" className="px-5 py-2 text-xs text-[var(--color-text-tertiary)]">
          {t('ai.disabled')}
        </p>
      )}
      <AiChatInput onSend={onSend} disabled={isLoading || isUnavailable} />
    </div>
  );
}
