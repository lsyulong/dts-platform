import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import client from '../api/client';
import { readSseText } from '../lib/readSseText';
import { clearSession } from '../lib/session';

type Message = {
  role: 'user' | 'assistant';
  content: string;
};

export function useAiChat() {
  const { t } = useTranslation();
  const [messages, setMessages] = useState<Message[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isUnavailable, setIsUnavailable] = useState(false);

  const sendMessage = useCallback(
    async (text: string) => {
      if (isLoading || isUnavailable) return;
      const userMsg: Message = { role: 'user', content: text };
      setMessages((prev) => [...prev, userMsg]);
      setIsLoading(true);

      const route = window.location.pathname;

      try {
        const token = localStorage.getItem('token');
        const response = await fetch(
          `${client.defaults.baseURL || '/api/v1'}/ai/chat?message=${encodeURIComponent(text)}&route=${encodeURIComponent(route)}`,
          {
            method: 'POST',
            headers: {
              Authorization: `Bearer ${token}`,
              Accept: 'text/event-stream',
            },
          },
        );

        if (!response.ok) {
          if (response.status === 401) {
            clearSession(token);
            throw new Error(t('errors.unauthorized'));
          }
          const body = (await response.json().catch(() => null)) as { message?: string } | null;
          const message = body?.message || `HTTP ${response.status}`;
          if (/未启用|disabled/i.test(message)) {
            setIsUnavailable(true);
            return;
          }
          throw new Error(message);
        }
        if (!response.body) throw new Error(t('errors.networkError'));
        const assistantMsg: Message = { role: 'assistant', content: '' };
        setMessages((prev) => [...prev, assistantMsg]);

        await readSseText(response.body, (content) =>
          setMessages((prev) =>
            prev.map((message, index) =>
              index === prev.length - 1 ? { role: 'assistant', content } : message,
            ),
          ),
        );
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : String(err);
        setMessages((prev) => [
          ...prev,
          { role: 'assistant', content: `${t('errors.networkError')}: ${errorMsg}` },
        ]);
      } finally {
        setIsLoading(false);
      }
    },
    [isLoading, isUnavailable, t],
  );

  const clearMessages = useCallback(() => {
    setMessages([]);
  }, []);

  return { messages, sendMessage, clearMessages, isLoading, isUnavailable };
}
