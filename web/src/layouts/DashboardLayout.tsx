import { useState } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import Sidebar from '../components/Sidebar';
import Header from '../components/Header';
import { AiFloatingButton } from '../components/ai/AiFloatingButton';
import { AiChatPanel } from '../components/ai/AiChatPanel';
import { useAiChat } from '../hooks/useAiChat';

export default function DashboardLayout() {
  const location = useLocation();
  const [aiOpen, setAiOpen] = useState(false);
  const { messages, sendMessage, isLoading } = useAiChat();

  return (
    <div className="flex h-screen bg-[var(--color-bg-primary)] text-[var(--color-text-primary)]">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <Header />
        <main className="flex-1 overflow-y-auto p-6" key={location.pathname}>
          <div className="page-transition">
            <Outlet />
          </div>
        </main>
      </div>
      <AiFloatingButton onClick={() => setAiOpen(true)} isOpen={aiOpen} />
      {aiOpen && (
        <AiChatPanel
          messages={messages}
          onSend={sendMessage}
          onClose={() => setAiOpen(false)}
          isLoading={isLoading}
        />
      )}
    </div>
  );
}
