import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { registerQueryClient } from "./data/organize";

export function createQueryClient(): QueryClient {
  // Data is local (IPC), so no network-style retries or refetch-on-focus; `library-changed` invalidates.
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false, staleTime: Infinity } } });
  registerQueryClient(client);
  return client;
}

export function Root({ client }: { client: QueryClient }) {
  return (
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>
  );
}
