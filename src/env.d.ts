/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Any non-empty value installs `src/mocks/ipc.ts` (browser dev without the Rust backend). */
  readonly VITE_MOCK_IPC?: string;
  /** "1" starts the mock with no library root, to see the first-run screen. */
  readonly VITE_MOCK_FIRST_RUN?: string;
}
