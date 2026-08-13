import type { DownloadEvent, UpdateStatus } from "./update-state";

export type AppScreen = "home" | "appearance" | "settings";
export type BusyState = "idle" | "loading" | "warming" | "starting" | "stopping" | "saving";
export type Tone = "neutral" | "accent" | "success" | "warning" | "danger";

export interface CaptureRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface OcrConfig {
  confidenceThreshold: number;
  preprocessingEnabled: boolean;
  grayscale: boolean;
  contrastEnhancement: boolean;
  binarize: boolean;
  enableMultiPass: boolean;
  multiPassCount: number;
  validationStrictness: "permissive" | "moderate" | "strict";
}

export interface OverlayConfig {
  fontSize: number;
  fontFamily: string;
  textColor: string;
  backgroundColor: string;
  offsetY: number;
  maxWidth: number;
  showDiagnostics: boolean;
}

export interface TranslationConfig {
  enableLocalEngine: boolean;
  allowMockFallback: boolean;
  enableContextAware: boolean;
  contextLevel: "off" | "memoryOnly" | "memoryAndRecent";
  contextRecentCount: number;
  contextBudgetPercent: number;
  contextSummaryCooldownMs: number;
  promptMaxSourceChars: number;
  promptMaxContextChars: number;
  contextBufferSize: number;
  contextResetGapMs: number;
  localEngine: { model: string | null; timeoutMs: number };
  ocr: OcrConfig;
}

export interface AppSettings {
  sourceLanguage: string;
  targetLanguage: string;
  captureIntervalMs: number;
  overlay: OverlayConfig;
  translation: TranslationConfig;
  lastCaptureRegion?: CaptureRegion | null;
  minimizeToTray: boolean;
  [key: string]: unknown;
}

export interface EngineStatus {
  phase?: string;
  serviceRunning?: boolean;
  message?: string;
  supportCode?: string;
  installed?: boolean;
  [key: string]: unknown;
}

export interface UiSnapshot {
  screen: AppScreen;
  busy: BusyState;
  settings: AppSettings;
  region: CaptureRegion | null;
  engine: EngineStatus | null;
  ocrLanguages: ReadonlySet<string>;
  running: boolean;
  error: string | null;
  notice: string | null;
  developerMode: boolean;
  update: UpdateStatus;
  /** `null` until the app reports its own version, and always so in browser mode. */
  appVersion: string | null;
}

export type PrimaryAction =
  "setup" | "installOcr" | "repair" | "selectRegion" | "start" | "stop" | "none";

export interface HomePresentation {
  state: "checking" | "notReady" | "ready" | "running" | "attention";
  statusLabel: string;
  title: string;
  description: string;
  action: PrimaryAction;
  actionLabel: string;
  actionIcon: string;
  actionDisabled: boolean;
  supportLine: string;
  supportTone: Tone;
}

/** A newer release the updater has already verified the signature of. */
export interface PendingUpdate {
  version: string;
  notes: string | null;
  /**
   * Download and hand off to the installer. On Windows this never returns
   * normally: the installer requires the app to exit, so the process ends
   * inside this call.
   */
  install(onProgress: (event: DownloadEvent) => void): Promise<void>;
}

export interface UpdatesApi {
  currentVersion(): Promise<string>;
  /** `null` means the endpoint answered and nothing newer exists. */
  check(): Promise<PendingUpdate | null>;
  restart(): Promise<void>;
}

export interface TauriBridgeApi {
  invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T>;
  isBrowserMode(): boolean;
  event: {
    listen(eventName: string, callback: (event: { payload: unknown }) => void): Promise<() => void>;
    emit(eventName: string, payload: unknown): Promise<void>;
  };
  /** Absent outside Tauri: browser mode has no window to control. */
  windowControls?: {
    minimize(): Promise<void>;
    toggleMaximize(): Promise<void>;
    close(): Promise<void>;
    isMaximized(): Promise<boolean>;
  };
  /** Absent outside Tauri: browser mode has no installation to replace. */
  updates?: UpdatesApi;
}

declare global {
  interface Window {
    TauriBridge: TauriBridgeApi;
    OcrLanguageTags: {
      isOcrLanguageAvailable(installed: ReadonlySet<string>, selected: string): boolean;
    };
  }
}
