import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";

export function createQueryClient(): QueryClient {
  // Data is local (IPC), so no network-style retries or refetch-on-focus; `library-changed` invalidates.
  return new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false, staleTime: Infinity } } });
}

export function Root({ client }: { client: QueryClient }) {
  return (
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>
  );
}
