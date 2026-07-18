import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { render } from '@testing-library/react';
import type { ReactElement } from 'react';

export function createTestQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
        staleTime: 0,
      },
      mutations: {
        retry: false,
      },
    },
  });
}

type RenderOptions = {
  route?: string;
  queryClient?: QueryClient;
};

export function renderWithProviders(
  ui: ReactElement,
  { route = '/', queryClient }: RenderOptions = {},
) {
  const testQueryClient = queryClient ?? createTestQueryClient();

  return render(
    <QueryClientProvider client={testQueryClient}>
      <MemoryRouter initialEntries={[route]}>
        {ui}
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
