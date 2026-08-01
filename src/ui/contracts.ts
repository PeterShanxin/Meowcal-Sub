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
  enableFoundryLocal: boolean;
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
  foundryLocal: { model: string | null; timeoutMs: number };
  ocr: OcrConfig;
}

export interface AppSettings {
  sourceLanguage: string;
  targetLanguage: string;
  captureIntervalMs: number;
  overlay: OverlayConfig;
  translation: TranslationConfig;
  lastCaptureRegion?: CaptureRegion | null;
  autoStart: boolean;
  minimizeToTray: boolean;
  startWithWindows: boolean;
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

export interface TauriBridgeApi {
  invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T>;
  isBrowserMode(): boolean;
  event: {
    listen(eventName: string, callback: (event: { payload: unknown }) => void): Promise<() => void>;
    emit(eventName: string, payload: unknown): Promise<void>;
  };
}

declare global {
  interface Window {
    TauriBridge: TauriBridgeApi;
    OcrLanguageTags: {
      isOcrLanguageAvailable(installed: ReadonlySet<string>, selected: string): boolean;
    };
  }
}
