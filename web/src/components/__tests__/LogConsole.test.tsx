import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import LogConsole from '../LogConsole';

const sampleLogs = [
  { timestamp: '10:30:45', level: 'INFO', component: 'system', message: 'System started' },
  { timestamp: '10:30:46', level: 'WARN', component: 'db', message: 'Connection pool low' },
  { timestamp: '10:30:47', level: 'ERROR', component: 'task-1', message: 'Sync failed: timeout' },
  { timestamp: '10:30:48', level: 'DEBUG', component: 'api', message: 'Request received' },
  { timestamp: '10:30:49', level: 'INFO', component: 'monitor', message: 'Health check passed' },
];

describe('LogConsole', () => {
  it('renders the default title', () => {
    render(<LogConsole />);
    expect(screen.getByText('dashboard.realtimeLog')).toBeInTheDocument();
  });

  it('renders a custom title', () => {
    render(<LogConsole title="System Logs" />);
    expect(screen.getByText('System Logs')).toBeInTheDocument();
  });

  it('renders empty state when no logs provided', () => {
    render(<LogConsole />);
    expect(screen.getByText('dashboard.waitingForLogs')).toBeInTheDocument();
  });

  it('renders log entries', () => {
    render(<LogConsole logs={sampleLogs} />);
    expect(screen.getByText('System started')).toBeInTheDocument();
    expect(screen.getByText('Connection pool low')).toBeInTheDocument();
    expect(screen.getByText('Sync failed: timeout')).toBeInTheDocument();
  });

  it('renders timestamps for each log', () => {
    render(<LogConsole logs={sampleLogs} />);
    expect(screen.getByText('10:30:45')).toBeInTheDocument();
    expect(screen.getByText('10:30:46')).toBeInTheDocument();
  });

  it('renders log levels in brackets', () => {
    render(<LogConsole logs={sampleLogs} />);
    // The log level is rendered as "[{level}]" which includes the brackets as JSX text
    const infoElements = screen.getAllByText((content) => content.includes('INFO'));
    const warnElements = screen.getAllByText((content) => content.includes('WARN'));
    const errorElements = screen.getAllByText((content) => content.includes('ERROR'));
    expect(infoElements.length).toBeGreaterThan(0);
    expect(warnElements.length).toBeGreaterThan(0);
    expect(errorElements.length).toBeGreaterThan(0);
  });

  it('renders component names', () => {
    render(<LogConsole logs={sampleLogs} />);
    expect(screen.getByText('[system]')).toBeInTheDocument();
    expect(screen.getByText('[db]')).toBeInTheDocument();
    expect(screen.getByText('[task-1]')).toBeInTheDocument();
  });

  describe('filter buttons', () => {
    it('renders ALL filter button as active by default', () => {
      render(<LogConsole logs={sampleLogs} />);
      const allButton = screen.getByText('ALL');
      expect(allButton.className).toContain('bg-[var(--color-bg-tertiary)]');
    });

    it('renders INFO, WARN, ERROR filter buttons', () => {
      render(<LogConsole logs={sampleLogs} />);
      expect(screen.getByText('INFO')).toBeInTheDocument();
      expect(screen.getByText('WARN')).toBeInTheDocument();
      expect(screen.getByText('ERROR')).toBeInTheDocument();
    });

    it('filters logs to INFO level only', async () => {
      const user = userEvent.setup();
      render(<LogConsole logs={sampleLogs} />);

      await user.click(screen.getByText('INFO'));

      // INFO logs should still be visible
      expect(screen.getByText('System started')).toBeInTheDocument();
      expect(screen.getByText('Health check passed')).toBeInTheDocument();
      // WARN, ERROR, DEBUG should be hidden
      expect(screen.queryByText('Connection pool low')).not.toBeInTheDocument();
      expect(screen.queryByText('Sync failed: timeout')).not.toBeInTheDocument();
    });

    it('filters logs to WARN level only', async () => {
      const user = userEvent.setup();
      render(<LogConsole logs={sampleLogs} />);

      await user.click(screen.getByText('WARN'));

      expect(screen.getByText('Connection pool low')).toBeInTheDocument();
      expect(screen.queryByText('System started')).not.toBeInTheDocument();
    });

    it('filters logs to ERROR level only', async () => {
      const user = userEvent.setup();
      render(<LogConsole logs={sampleLogs} />);

      await user.click(screen.getByText('ERROR'));

      expect(screen.getByText('Sync failed: timeout')).toBeInTheDocument();
      expect(screen.queryByText('System started')).not.toBeInTheDocument();
    });

    it('shows empty state when filter matches no logs', async () => {
      const user = userEvent.setup();
      const logs = [
        { timestamp: '10:30:45', level: 'INFO', component: 'system', message: 'System started' },
      ];
      render(<LogConsole logs={logs} />);

      await user.click(screen.getByText('ERROR'));

      expect(screen.getByText('dashboard.waitingForLogs')).toBeInTheDocument();
    });

    it('resets to ALL when clicking ALL button', async () => {
      const user = userEvent.setup();
      render(<LogConsole logs={sampleLogs} />);

      await user.click(screen.getByText('ERROR'));
      await user.click(screen.getByText('ALL'));

      expect(screen.getByText('System started')).toBeInTheDocument();
      expect(screen.getByText('Sync failed: timeout')).toBeInTheDocument();
    });
  });

  describe('level colors', () => {
    it('applies blue color to INFO level', () => {
      render(<LogConsole logs={[{ timestamp: '10:30:45', level: 'INFO', component: 'sys', message: 'test' }]} />);
      const levelSpan = screen.getByText('[INFO]');
      expect(levelSpan.className).toContain('text-blue-400');
    });

    it('applies amber color to WARN level', () => {
      render(<LogConsole logs={[{ timestamp: '10:30:45', level: 'WARN', component: 'sys', message: 'test' }]} />);
      const levelSpan = screen.getByText('[WARN]');
      expect(levelSpan.className).toContain('text-amber-400');
    });

    it('applies rose color to ERROR level', () => {
      render(<LogConsole logs={[{ timestamp: '10:30:45', level: 'ERROR', component: 'sys', message: 'test' }]} />);
      const levelSpan = screen.getByText('[ERROR]');
      expect(levelSpan.className).toContain('text-rose-400');
    });
  });

  describe('maxHeight prop', () => {
    it('sets default maxHeight to 300px', () => {
      const { container } = render(<LogConsole />);
      const scrollDiv = container.querySelector('.overflow-y-auto');
      expect(scrollDiv?.getAttribute('style')).toContain('max-height: 300px');
    });

    it('sets custom maxHeight', () => {
      const { container } = render(<LogConsole maxHeight="200px" />);
      const scrollDiv = container.querySelector('.overflow-y-auto');
      expect(scrollDiv?.getAttribute('style')).toContain('max-height: 200px');
    });
  });
});
