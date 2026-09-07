// Toast notification store (Design System foundation, Phase D1 —
// promt.md §19). Confirmed via grep across `src/` before building this:
// no "toast" hits anywhere — this is a genuinely new mechanism, not a
// reskin of anything existing.
//
// Mirrors this codebase's own established store convention exactly: a
// class instance holding `$state`, exported as a singleton (see
// `stores/history.svelte.ts`/`stores/systemInfo.svelte.ts` for the same
// shape), rather than inventing a different pattern for the first toast
// mechanism in this app. `.svelte.ts` (not `.ts`) for the same reason every
// other rune-backed store here uses it — `$state` needs it outside a
// `.svelte` file.

export type ToastVariant = "info" | "success" | "warning" | "error";

export interface ToastMessage {
  id: string;
  variant: ToastVariant;
  text: string;
  durationMs: number;
}

const DEFAULT_DURATION_MS = 4000;

class ToastStore {
  /** Reactive list of currently-visible toasts, oldest first. Read this
   * directly from `Toast.svelte` (the one renderer, mounted once in
   * `App.svelte` — same "one dialog instance per store" precedent every
   * other dialog store here follows). */
  messages = $state<ToastMessage[]>([]);

  /** Show a toast; returns its id (e.g. to `dismiss()` it early). A
   * `durationMs` of 0 means "stays until manually dismissed". */
  show(text: string, variant: ToastVariant = "info", durationMs: number = DEFAULT_DURATION_MS): string {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    this.messages = [...this.messages, { id, variant, text, durationMs }];
    if (durationMs > 0) {
      setTimeout(() => this.dismiss(id), durationMs);
    }
    return id;
  }

  info(text: string, durationMs?: number): string {
    return this.show(text, "info", durationMs);
  }

  success(text: string, durationMs?: number): string {
    return this.show(text, "success", durationMs);
  }

  warning(text: string, durationMs?: number): string {
    return this.show(text, "warning", durationMs);
  }

  error(text: string, durationMs?: number): string {
    return this.show(text, "error", durationMs);
  }

  dismiss(id: string): void {
    this.messages = this.messages.filter((m) => m.id !== id);
  }

  clear(): void {
    this.messages = [];
  }
}

export const toastStore = new ToastStore();
