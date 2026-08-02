import { languages } from "./languages";

type SupportedSourceLanguage = (typeof languages)[number]["value"];

export const sampleTranslations: Record<SupportedSourceLanguage, readonly string[]> = {
  "zh-CN": [
    "先不提时钟塔，先把门关上。",
    "你听见了吗？外面有人在叫我们。",
    "如果现在出发，我们还能赶上末班车。",
  ],
  "zh-TW": [
    "先別提鐘塔，先把門關上。",
    "你聽見了嗎？外面有人在叫我們。",
    "如果現在出發，我們還趕得上末班車。",
  ],
  "ja-JP": [
    "時計塔の話は後だ、まずドアを閉めろ。",
    "聞こえた？ 外で誰かが僕たちを呼んでる。",
    "今出れば、まだ最終電車に間に合う。",
  ],
  "ko-KR": [
    "시계탑 이야기는 나중에 하고, 일단 문부터 닫아.",
    "들었어? 밖에서 누가 우리를 부르고 있어.",
    "지금 출발하면 막차를 아직 탈 수 있어.",
  ],
  "en-US": [
    "Forget the clock tower for now. Close the door first.",
    "Did you hear that? Someone outside is calling for us.",
    "If we leave now, we can still catch the last train.",
  ],
  "es-ES": [
    "Dejemos la torre del reloj para después. Cierra la puerta primero.",
    "¿Lo has oído? Alguien de fuera nos está llamando.",
    "Si salimos ahora, todavía llegaremos al último tren.",
  ],
  "fr-FR": [
    "La tour de l'horloge attendra. Ferme d'abord la porte.",
    "Tu as entendu ? Quelqu'un dehors nous appelle.",
    "Si on part maintenant, on peut encore attraper le dernier train.",
  ],
  "de-DE": [
    "Der Uhrturm kann warten. Schließ zuerst die Tür.",
    "Hast du das gehört? Draußen ruft jemand nach uns.",
    "Wenn wir jetzt losgehen, erwischen wir noch den letzten Zug.",
  ],
};

function clampRandomValue(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

export function pickSampleTranslation(
  sourceLanguage: string,
  randomValue: number = Math.random(),
): string {
  const samples =
    sampleTranslations[sourceLanguage as SupportedSourceLanguage] ?? sampleTranslations["en-US"];
  const index = Math.min(
    samples.length - 1,
    Math.floor(clampRandomValue(randomValue) * samples.length),
  );
  return samples[index];
}
