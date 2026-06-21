// Launcher sound system: reliable UI sounds (rendered to WAV data-URIs and played
// through <audio>, which works on WebKitGTK where bare WebAudio is often silent),
// a randomized lofi playlist, and a selectable output device. Settings persist in
// localStorage and are controlled from the Launcher settings screen.

type SoundSettings = {
  uiEnabled: boolean;
  uiVolume: number; // 0..1
  musicEnabled: boolean;
  musicVolume: number; // 0..1
  outputDeviceId: string; // "" = system default
};

const DEFAULTS: SoundSettings = {
  uiEnabled: true,
  uiVolume: 0.35,
  musicEnabled: false,
  musicVolume: 0.35,
  outputDeviceId: "",
};

// Fixed lofi playlist — drop these files into celaris-launcher/public/.
const PLAYLIST = ["/lofi1.mp3", "/lofi2.mp3", "/lofi3.mp3", "/lofi4.mp3", "/lofi5.mp3"];

const KEY = "celaris-sound";

export function loadSound(): SoundSettings {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { ...DEFAULTS };
  }
}

let settings = loadSound();

export function getSound(): SoundSettings {
  return settings;
}

export function saveSound(next: Partial<SoundSettings>) {
  const before = settings;
  settings = { ...settings, ...next };
  localStorage.setItem(KEY, JSON.stringify(settings));
  if (next.outputDeviceId !== undefined && next.outputDeviceId !== before.outputDeviceId) {
    applySink();
  }
  applyMusic();
}

// --- WAV synthesis (so UI sounds are real audio, reliably audible) ----------

/** Builds a tiny mono 16-bit WAV (soft attack + decay) and returns it as a data URI. */
function makeWav(freq: number, durSec: number, type: "sine" | "triangle" | "square" | "saw", amp = 0.7): string {
  const sr = 44100;
  const n = Math.floor(sr * durSec);
  const bytes = 44 + n * 2;
  const buf = new ArrayBuffer(bytes);
  const dv = new DataView(buf);
  const wr = (off: number, s: string) => {
    for (let i = 0; i < s.length; i++) dv.setUint8(off + i, s.charCodeAt(i));
  };
  wr(0, "RIFF");
  dv.setUint32(4, 36 + n * 2, true);
  wr(8, "WAVE");
  wr(12, "fmt ");
  dv.setUint32(16, 16, true);
  dv.setUint16(20, 1, true); // PCM
  dv.setUint16(22, 1, true); // mono
  dv.setUint32(24, sr, true);
  dv.setUint32(28, sr * 2, true);
  dv.setUint16(32, 2, true);
  dv.setUint16(34, 16, true);
  wr(36, "data");
  dv.setUint32(40, n * 2, true);
  for (let i = 0; i < n; i++) {
    const t = i / sr;
    const ph = (freq * t) % 1;
    let w: number;
    switch (type) {
      case "triangle": w = 4 * Math.abs(ph - 0.5) - 1; break;
      case "square": w = ph < 0.5 ? 1 : -1; break;
      case "saw": w = 2 * ph - 1; break;
      default: w = Math.sin(2 * Math.PI * ph);
    }
    // Hann window: smooth bell-shaped fade in AND out → no click, no harshness.
    const env = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / Math.max(1, n - 1));
    const s = Math.max(-1, Math.min(1, w * env * amp));
    dv.setInt16(44 + i * 2, s * 0x7fff, true);
  }
  let bin = "";
  const u8 = new Uint8Array(buf);
  for (let i = 0; i < u8.length; i++) bin += String.fromCharCode(u8[i]);
  return "data:audio/wav;base64," + btoa(bin);
}

// Soft, rounded low "pops" (Hann-windowed sines) — pleasant, never piercing.
const CLICK = makeWav(196, 0.11, "sine", 0.5);
const HOVER = makeWav(262, 0.05, "sine", 0.14);

// A small pool of <audio> elements per sound so rapid plays can overlap.
function makePool(src: string, size: number): HTMLAudioElement[] {
  if (typeof window === "undefined") return [];
  return Array.from({ length: size }, () => {
    const a = new Audio(src);
    a.preload = "auto";
    return a;
  });
}
let clickPool: HTMLAudioElement[] = [];
let hoverPool: HTMLAudioElement[] = [];
let clickIdx = 0;
let hoverIdx = 0;

function ensurePools() {
  if (clickPool.length === 0) clickPool = makePool(CLICK, 4);
  if (hoverPool.length === 0) hoverPool = makePool(HOVER, 3);
}

function applySinkTo(el: HTMLAudioElement) {
  const id = settings.outputDeviceId;
  const anyEl = el as any;
  if (typeof anyEl.setSinkId === "function") {
    anyEl.setSinkId(id || "default").catch(() => {});
  }
}

function play(pool: HTMLAudioElement[], i: number): number {
  if (!settings.uiEnabled || pool.length === 0) return i;
  const el = pool[i % pool.length];
  el.volume = settings.uiVolume;
  applySinkTo(el);
  try {
    el.currentTime = 0;
    void el.play();
  } catch {
    /* ignore */
  }
  return (i + 1) % pool.length;
}

export const sfx = {
  click: () => {
    ensurePools();
    clickIdx = play(clickPool, clickIdx);
  },
  hover: () => {
    ensurePools();
    hoverIdx = play(hoverPool, hoverIdx);
  },
};

// --- Lofi music (randomized playlist) ---------------------------------------

let music: HTMLAudioElement | null = null;
let order: number[] = [];
let pos = 0;

function shuffle<T>(a: T[]): T[] {
  const arr = a.slice();
  for (let i = arr.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
  return arr;
}

function ensureMusic(): HTMLAudioElement | null {
  if (typeof window === "undefined") return null;
  if (!music) {
    music = new Audio();
    music.preload = "auto";
    music.addEventListener("ended", nextTrack);
    music.addEventListener("error", nextTrack); // skip missing files
  }
  return music;
}

function nextTrack() {
  const el = ensureMusic();
  if (!el || !settings.musicEnabled) return;
  pos++;
  if (pos >= order.length) {
    order = shuffleOrder();
    pos = 0;
  }
  el.src = PLAYLIST[order[pos]];
  applySinkTo(el);
  el.volume = settings.musicVolume;
  el.play().catch(() => {});
}

function shuffleOrder(): number[] {
  return shuffle(PLAYLIST.map((_, i) => i));
}

/** Reconciles the music element with the current settings. */
export function applyMusic() {
  const el = ensureMusic();
  if (!el) return;
  el.volume = settings.musicVolume;
  if (settings.musicEnabled) {
    if (order.length === 0 || !el.src) {
      order = shuffleOrder();
      pos = 0;
      el.src = PLAYLIST[order[pos]];
      applySinkTo(el);
    }
    el.play().catch(() => {});
  } else {
    el.pause();
  }
}

/** Routes all audio to the selected output device. */
export function applySink() {
  ensurePools();
  [...clickPool, ...hoverPool].forEach(applySinkTo);
  if (music) applySinkTo(music);
}

/** Lists available audio output devices (best effort; labels need permission). */
export async function enumerateOutputs(): Promise<{ deviceId: string; label: string }[]> {
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    return devices
      .filter((d) => d.kind === "audiooutput")
      .map((d) => ({ deviceId: d.deviceId, label: d.label || "Ausgabegerät" }));
  } catch {
    return [];
  }
}

/** Call once on the first user gesture so autoplay restrictions are satisfied. */
export function unlockAudio() {
  ensurePools();
  if (settings.musicEnabled) applyMusic();
}
