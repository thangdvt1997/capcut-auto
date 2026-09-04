// Hand-rolled i18n module (master prompt §47: "Do NOT hardcode every string
// directly in components" — English + Vietnamese at launch, "make
// architecture ready for more languages"). No third-party i18n library —
// this app already has enough dependencies; the actual feature set needed
// (nested-key lookup, `{{param}}` interpolation, fallback-to-English,
// persisted locale) is small enough to own directly.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.
//
// Call sites only ever use the exported `t()` / `setLocale()` / `locale`
// below. The persistence layer (currently `localStorage`) is isolated
// behind the `LocalePersistence` interface so a future Settings phase can
// swap in a Rust-backed app setting without touching any component.

import enMessages from "../locales/en.json";
import viMessages from "../locales/vi.json";

export type Locale = "en" | "vi";

export const SUPPORTED_LOCALES: readonly Locale[] = ["en", "vi"];
export const DEFAULT_LOCALE: Locale = "en";

const STORAGE_KEY = "aiVideoEditor.locale";

/** A locale catalog: string leaves, arbitrarily nested under namespace keys. */
type Messages = { [key: string]: string | Messages };

const catalogs: Record<Locale, Messages> = {
  en: enMessages as Messages,
  vi: viMessages as Messages,
};

function isLocale(value: string | null): value is Locale {
  return value !== null && (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

/**
 * Where the chosen locale is persisted. Swappable so a later Settings phase
 * (master prompt §46 — Settings will eventually own this as a Rust-backed
 * app setting) can replace `localStorage` without changing `t()`/`setLocale()`
 * call sites anywhere in the app.
 */
export interface LocalePersistence {
  load(): Locale | null;
  save(locale: Locale): void;
}

class LocalStorageLocalePersistence implements LocalePersistence {
  load(): Locale | null {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      return isLocale(stored) ? stored : null;
    } catch {
      // localStorage may be unavailable (private browsing, disabled storage).
      return null;
    }
  }

  save(locale: Locale): void {
    try {
      localStorage.setItem(STORAGE_KEY, locale);
    } catch {
      /* storage may be disabled — locale simply won't survive a restart */
    }
  }
}

let localePersistence: LocalePersistence = new LocalStorageLocalePersistence();

/** Swap the persistence backend (e.g. once Settings has a Rust-backed app
 * setting to read/write instead of `localStorage`). Nothing else in the app
 * needs to change — `t()`/`setLocale()` keep working against whatever
 * backend is installed here. */
export function setLocalePersistence(next: LocalePersistence): void {
  localePersistence = next;
}

/** Detecting OS/browser locale is a nice-to-have — not implemented, default is 'en'. */
function loadInitialLocale(): Locale {
  return localePersistence.load() ?? DEFAULT_LOCALE;
}

function lookup(messages: Messages, key: string): string | undefined {
  const parts = key.split(".");
  let node: string | Messages = messages;
  for (const part of parts) {
    if (typeof node === "string") return undefined;
    const next: string | Messages | undefined = node[part];
    if (next === undefined) return undefined;
    node = next;
  }
  return typeof node === "string" ? node : undefined;
}

function interpolate(template: string, params: Record<string, string | number>): string {
  return template.replace(/\{\{(\w+)\}\}/g, (match, name: string) => {
    const value = params[name];
    return value === undefined ? match : String(value);
  });
}

class I18nStore {
  /** Reactive current-locale state (Svelte 5 rune) — read this directly, or
   * call `t()`, from any component/template; both register as a dependency
   * of whatever reactive context reads them. */
  locale = $state<Locale>(loadInitialLocale());

  setLocale(next: Locale): void {
    this.locale = next;
    localePersistence.save(next);
  }

  /** Look up a dot-path key (e.g. `"mediaLibrary.importButton"`) in the
   * active locale, falling back to English if the active locale is missing
   * that key, and finally to the raw key itself if neither has it (so a
   * missing translation is visibly wrong in the UI rather than blank). */
  t(key: string, params?: Record<string, string | number>): string {
    const template = lookup(catalogs[this.locale], key) ?? lookup(catalogs[DEFAULT_LOCALE], key) ?? key;
    return params ? interpolate(template, params) : template;
  }
}

const i18n = new I18nStore();

export function t(key: string, params?: Record<string, string | number>): string {
  return i18n.t(key, params);
}

export function setLocale(next: Locale): void {
  i18n.setLocale(next);
}

/** Reactive accessor for the current locale (e.g. to drive a language
 * switcher's selected value). Reading `currentLocale()` inside a component's
 * template/`$derived`/`$effect` tracks `i18n.locale` correctly. */
export function currentLocale(): Locale {
  return i18n.locale;
}
