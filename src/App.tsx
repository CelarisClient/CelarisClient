import { useEffect, useRef, useState } from "react";
import { getSound, saveSound, applyMusic, unlockAudio, loadSound, enumerateOutputs } from "./sound";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { enable as autostartEnable, disable as autostartDisable, isEnabled as autostartIsEnabled } from "@tauri-apps/plugin-autostart";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import {
  Button,
  Card,
  PrimaryButton,
  ProgressBar,
  SidebarItem,
  StatusBadge,
  PlayIcon,
  ProfilesIcon,
  ModsIcon,
  SettingsIcon,
  WardrobeIcon,
  ServerIcon,
  NewsIcon,
  FriendsIcon,
  HostingIcon,
  CosmeticsIcon,
  CreditsIcon,
} from "./components/ui";
import { Rinnegan } from "./components/Rinnegan";
import { TitleBar } from "./components/TitleBar";
import { SpaceBackground } from "./components/SpaceBackground";
import { MatrixRain } from "./components/MatrixRain";
import { SkinBody } from "./components/SkinBody";
import { SkinFace } from "./components/SkinFace";

type JavaInstall = { path: string; version: string };

type Profile = {
  name: string;
  minecraft_version: string;
  java_path: string;
  max_ram_mb: number;
  game_dir: string;
  use_celaris_client: boolean;
  use_fabric: boolean;
  jvm_args: string;
  env_vars: string;
};

type Session = { username: string; uuid: string; access_token: string; user_type: string };
type Account = { kind: string; username: string; uuid: string; access_token: string; user_type: string; refresh_token?: string };
type OnlineLogin = { session: Session; refresh_token: string };
type LaunchProgress = { stage: string; message: string; current: number; total: number };
type LaunchError = { stage: string; code: string; message: string };
type DeviceCode = { user_code: string; verification_uri: string; message: string };

type ModHit = {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  downloads: number;
  icon_url: string | null;
};
type InstalledMod = { id: string; filename: string; project_id?: string; title?: string; icon_url?: string; version_id?: string };
type McVersion = { id: string; kind: string; release_time: string };
type SkinInfo = { id: string; name: string; uuid: string; png_base64: string };
type ServerEntry = {
  name: string;
  address: string;
  partner: boolean;
  description: string | null;
  icon: string | null;
  banner: string | null;
};
type ServerStatus = {
  online: boolean;
  players: number;
  max: number;
  icon: string | null;
  motd: string;
  version: string | null;
};
type NewsItem = {
  source: string;
  title: string;
  date: string;
  tag: string;
  body: string;
  full: string;
  image: string | null;
};
type GlobalPack = {
  name: string;
  mc_version: string;
  loader: string;
  description: string | null;
  icon_url: string | null;
  url: string;
};
type ServerModpack = {
  slug: string;
  name: string;
  description: string;
  mc_version: string;
  icon_url: string;
  mods: { project_id: string; name: string }[];
  server_address: string;
};
type UserPresence = { username: string; status: string; server: string | null; playtime_secs: number };
type ChatMsg = { from: string; text: string; system?: boolean };
type SharedShot = { from: string; name: string; data: string };
type View =
  | "play"
  | "news"
  | "friends"
  | "profiles"
  | "mods"
  | "servers"
  | "hosting"
  | "cosmetics"
  | "wardrobe"
  | "resourcepacks"
  | "shaders"
  | "logs"
  | "credits"
  | "settings"
  | "launcher";
type LaunchState = "idle" | "running" | "launched" | "error";

const DEFAULT_PROFILE: Profile = {
  name: "Default",
  minecraft_version: "1.21.11",
  java_path: "java",
  max_ram_mb: 4096,
  game_dir: "",
  use_celaris_client: true,
  use_fabric: false,
  jvm_args: "",
  env_vars: "",
};

/** Celaris Discord invite. */
const DISCORD_INVITE = "https://discord.gg/62RTMCVjQ4";

/** Selectable themes — each swaps the accent palette AND the background.
   `video` (optional) = animated background; `space` = star/planet effects. */
const THEMES: { key: string; name: string; a: string; b: string; bg: string; video?: string; space?: boolean }[] = [
  { key: "celaris", name: "Celaris", a: "#9d5cff", b: "#c46bff", bg: "/celaris-bg.png", space: true },
  { key: "nagatoro", name: "Nagatoro", a: "#ff7eb6", b: "#8a5cff", bg: "/themes/nagatoro.jpg" },
  { key: "beach", name: "Beach / LA", a: "#ff9d5c", b: "#34e3d0", bg: "/themes/beach.jpg" },
  { key: "akatsuki", name: "Pain / Akatsuki", a: "#ff8a3d", b: "#e0454f", bg: "/themes/akatsuki.jpg" },
  { key: "matrix", name: "Matrix", a: "#3be36b", b: "#1aa64a", bg: "" },
  { key: "spiderman", name: "Spider-Man", a: "#ff3b3b", b: "#ff8a1a", bg: "/themes/spiderman.jpg" },
];

function hexToRgba(hex: string, alpha: number): string {
  const h = hex.replace("#", "");
  const n = parseInt(h, 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

function applyTheme(key: string) {
  const t = THEMES.find((x) => x.key === key) ?? THEMES[0];
  const r = document.documentElement.style;
  r.setProperty("--accent", t.a);
  r.setProperty("--accent-2", t.b);
  r.setProperty("--play-grad", `linear-gradient(135deg, ${t.a} 0%, ${t.b} 100%)`);
  r.setProperty("--accent-soft", hexToRgba(t.a, 0.14));
  r.setProperty("--accent-glow", hexToRgba(t.a, 0.4));
  localStorage.setItem("celaris-theme", t.key);
}

const STEPS = [
  { key: "resolve", label: "Auflösen" },
  { key: "download", label: "Laden" },
  { key: "inject", label: "Installieren" },
  { key: "launch", label: "Starten" },
];

const ERROR_MESSAGES: Record<string, string> = {
  ManifestUnreachable: "Mojang-Server nicht erreichbar — prüfe deine Internetverbindung.",
  ManifestInvalid: "Das Versions-Manifest ist beschädigt.",
  VersionNotFound: "Diese Minecraft-Version existiert nicht.",
  VersionHashMismatch: "Versions-Datei beschädigt (Integritätsprüfung fehlgeschlagen).",
  VersionJsonInvalid: "Versions-Daten ungültig.",
  AssetIndexUnreachable: "Assets konnten nicht geladen werden (Server nicht erreichbar).",
  AssetIndexInvalid: "Der Asset-Index ist beschädigt.",
  FabricMetaUnreachable: "Fabric-Server nicht erreichbar.",
  FabricMetaInvalid: "Ungültige Antwort vom Fabric-Server.",
  DownloadFailed: "Ein Download ist fehlgeschlagen — prüfe deine Verbindung.",
  Sha1Mismatch: "Beschädigter Download (Prüfsumme stimmt nicht).",
  NativesExtractFailed: "Native Bibliotheken konnten nicht entpackt werden.",
  FabricLoaderMissing: "Der Fabric-Loader fehlt nach der Installation.",
  FabricApiMissing: "Die Fabric API fehlt nach der Installation.",
  ModMissing: "Ein Mod fehlt nach der Installation.",
  GameDirError: "Das Spielverzeichnis konnte nicht angelegt werden.",
  SpawnFailed: "Minecraft konnte nicht gestartet werden — prüfe den Java-Pfad.",
  ProcessExitedEarly: "Minecraft ist beim Start abgestürzt — siehe Log.",
};

const STAGE_LABELS: Record<string, string> = {
  Resolve: "Auflösen",
  Download: "Laden",
  Inject: "Installieren",
  Launch: "Starten",
};

const STATUS_TITLES: Record<string, string> = {
  resolve: "Version wird aufgelöst…",
  download: "Dateien werden geladen…",
  inject: "Wird installiert…",
  launch: "Minecraft wird gestartet…",
};

function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export default function App() {
  const [view, setView] = useState<View>("play");

  const [profiles, setProfiles] = useState<Profile[]>([DEFAULT_PROFILE]);
  const profilesLoaded = useRef(false);
  const [active, setActive] = useState(0);

  const [javaInstalls, setJavaInstalls] = useState<JavaInstall[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [progress, setProgress] = useState<LaunchProgress | null>(null);
  const [launchState, setLaunchState] = useState<LaunchState>("idle");
  const [launchError, setLaunchError] = useState<LaunchError | null>(null);

  const [session, setSession] = useState<Session | null>(null);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [accountMenu, setAccountMenu] = useState(false);
  const [offlineName, setOfflineName] = useState("");
  const [autostart, setAutostart] = useState(false);
  // Export dialog: which profile + which categories to include.
  const [exportTarget, setExportTarget] = useState<Profile | null>(null);
  const [exportOpts, setExportOpts] = useState({
    mods: true, resourcepacks: true, shaderpacks: true, config: true, options: true,
  });
  const [sound, setSound] = useState(() => loadSound());
  const [audioOutputs, setAudioOutputs] = useState<{ deviceId: string; label: string }[]>([]);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [theme, setTheme] = useState<string>(() => localStorage.getItem("celaris-theme") ?? "celaris");
  const [loginBusy, setLoginBusy] = useState(false);
  const [deviceCode, setDeviceCode] = useState<DeviceCode | null>(null);
  const [loginError, setLoginError] = useState<string | null>(null);

  const [modsTab, setModsTab] = useState<"browse" | "installed">("browse");
  const [modQuery, setModQuery] = useState("");
  const [modResults, setModResults] = useState<ModHit[]>([]);
  const [modUpdates, setModUpdates] = useState<Set<string>>(new Set());
  const [updatingMod, setUpdatingMod] = useState<string | null>(null);
  const [modPage, setModPage] = useState(0);
  const [modSort, setModSort] = useState("relevance");
  const [searching, setSearching] = useState(false);
  const [installed, setInstalled] = useState<InstalledMod[]>([]);
  const [installing, setInstalling] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  // ResourcePack + Shader marketplaces (per-profile, reuse active selector).
  const [packKind, setPackKind] = useState<"resourcepack" | "shader">("resourcepack");
  const [packTab, setPackTab] = useState<"browse" | "installed">("browse");
  const [packQuery, setPackQuery] = useState("");
  const [packSort, setPackSort] = useState("relevance");
  const [packResults, setPackResults] = useState<ModHit[]>([]);
  const [packInstalled, setPackInstalled] = useState<InstalledMod[]>([]);
  const [packSearching, setPackSearching] = useState(false);
  const [packInstalling, setPackInstalling] = useState<string | null>(null);
  const [hasShaderLoader, setHasShaderLoader] = useState(false);

  const [versions, setVersions] = useState<McVersion[]>([]);
  const [appVersions, setAppVersions] = useState<{
    launcher: string;
    launcher_latest: string;
    update_available: boolean;
    update_url: string;
    client: string;
  }>({ launcher: "", launcher_latest: "", update_available: false, update_url: "", client: "" });
  const [showSnapshots, setShowSnapshots] = useState(false);

  const [skins, setSkins] = useState<SkinInfo[]>([]);
  const [grabQuery, setGrabQuery] = useState("");
  const [grabbing, setGrabbing] = useState(false);
  const [skinMsg, setSkinMsg] = useState<string | null>(null);
  const [appliedSkin, setAppliedSkin] = useState<{ dataUrl: string; slim: boolean } | null>(null);
  const [skinReload, setSkinReload] = useState(0);
  const [equippedSkinId, setEquippedSkinId] = useState<string>(() => localStorage.getItem("celaris-equipped-skin") ?? "");

  const [partners, setPartners] = useState<ServerEntry[]>([]);
  const [userServers, setUserServers] = useState<ServerEntry[]>([]);
  const [serverStatus, setServerStatus] = useState<Record<string, ServerStatus>>({});
  const [srvName, setSrvName] = useState("");
  const [srvAddr, setSrvAddr] = useState("");
  const [serverMsg, setServerMsg] = useState<string | null>(null);

  const [news, setNews] = useState<NewsItem[]>([]);
  const [newsLoading, setNewsLoading] = useState(false);
  const [openArticle, setOpenArticle] = useState<NewsItem | null>(null);

  const [socialConnected, setSocialConnected] = useState(false);
  const [presence, setPresence] = useState<UserPresence[]>([]);
  const [chat, setChat] = useState<ChatMsg[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [shots, setShots] = useState<SharedShot[]>([]);
  const [friends, setFriends] = useState<{ accepted: string[]; incoming: string[]; outgoing: string[] }>({
    accepted: [],
    incoming: [],
    outgoing: [],
  });
  const [addFriendInput, setAddFriendInput] = useState("");
  const playStartRef = useRef<number | null>(null);
  const chatEndRef = useRef<HTMLDivElement>(null);


  const [globalPacks, setGlobalPacks] = useState<GlobalPack[]>([]);
  const [installingPack, setInstallingPack] = useState<string | null>(null);
  const [serverPacks, setServerPacks] = useState<ServerModpack[]>([]);
  const [installingSrvPack, setInstallingSrvPack] = useState<string | null>(null);

  const logEndRef = useRef<HTMLDivElement>(null);

  const profile = profiles[active] ?? DEFAULT_PROFILE;

  // The skin currently equipped via the wardrobe (its local PNG drives the 3D
  // body + 2D face, so they always match what's actually worn — no CDN lag).
  const equippedSkin = skins.find((s) => s.id === equippedSkinId);
  const equippedSkinDataUrl = equippedSkin ? `data:image/png;base64,${equippedSkin.png_base64}` : undefined;

  function updateSound(patch: Partial<ReturnType<typeof getSound>>) {
    saveSound(patch);
    setSound(getSound());
  }

  // Start lofi music + unlock audio on the first user gesture.
  useEffect(() => {
    const unlock = () => unlockAudio();
    document.addEventListener("pointerdown", unlock, { once: true });
    applyMusic();
    enumerateOutputs().then(setAudioOutputs).catch(() => {});
  }, []);

  // Self-update: on launch, check the endpoint and install a newer signed build.
  useEffect(() => {
    (async () => {
      try {
        const update = await checkUpdate();
        if (update) {
          setUpdateMsg(`Update ${update.version} wird geladen…`);
          await update.downloadAndInstall();
          setUpdateMsg("Update installiert — Neustart…");
          await relaunch();
        }
      } catch {
        /* offline / no update published yet — ignore */
      }
    })();
  }, []);

  useEffect(() => {
    loadWardrobe(); // load skins early so the equipped skin shows on Play

    invoke<Profile[]>("get_profiles")
      .then((p) => {
        if (p.length > 0) setProfiles(p);
      })
      .catch(() => {})
      .finally(() => {
        // Only start auto-persisting AFTER the initial load, so we never
        // overwrite the saved file with the default placeholder profile.
        profilesLoaded.current = true;
      });

    invoke<Account[]>("get_accounts")
      .then((a) => {
        setAccounts(a);
        if (a.length > 0) setSession({ username: a[0].username, uuid: a[0].uuid, access_token: a[0].access_token, user_type: a[0].user_type });
      })
      .catch(() => {});

    autostartIsEnabled().then(setAutostart).catch(() => {});
    applyTheme(localStorage.getItem("celaris-theme") ?? "celaris");

    // App is mounted → reveal the main window and dismiss the splash screen.
    (async () => {
      try {
        await getCurrentWindow().show();
        const sp = await Window.getByLabel("splashscreen");
        await sp?.close();
      } catch (e) {
        console.error("splash handoff", e);
      }
    })();

    const u1 = listen<string>("launch-log", (e) => setLogs((l) => [...l, e.payload]));
    const u2 = listen<LaunchProgress>("launch-progress", (e) => setProgress(e.payload));
    const u3 = listen<LaunchError>("launch-error", (e) => {
      setLaunchError(e.payload);
      setLaunchState("error");
    });
    const u4 = listen<DeviceCode>("auth-device-code", (e) => setDeviceCode(e.payload));
    const s1 = listen<{ online: UserPresence[] }>("social-presence", (e) => setPresence(e.payload.online ?? []));
    const s2 = listen<{ from: string; text: string }>("social-chat", (e) =>
      setChat((c) => [...c, { from: e.payload.from, text: e.payload.text }].slice(-200))
    );
    const s3 = listen<{ message: string }>("social-system", (e) =>
      setChat((c) => [...c, { from: "", text: e.payload.message, system: true }].slice(-200))
    );
    const s4 = listen<SharedShot>("social-screenshot", (e) => setShots((s) => [e.payload, ...s].slice(0, 30)));
    const s5 = listen("social-disconnected", () => {
      setSocialConnected(false);
      setPresence([]);
    });
    const s6 = listen<{ accepted: string[]; incoming: string[]; outgoing: string[] }>("social-friends", (e) =>
      setFriends(e.payload)
    );
    const s7 = listen<{ from: string }>("social-friend-request", (e) =>
      setChat((c) => [...c, { from: "", text: `Freundschaftsanfrage von ${e.payload.from}`, system: true }].slice(-200))
    );
    return () => {
      u1.then((f) => f());
      u2.then((f) => f());
      u3.then((f) => f());
      u4.then((f) => f());
      s1.then((f) => f());
      s2.then((f) => f());
      s3.then((f) => f());
      s4.then((f) => f());
      s5.then((f) => f());
      s6.then((f) => f());
      s7.then((f) => f());
    };
  }, []);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  // Persist profiles automatically whenever they change (after the first load),
  // so a restart never loses them — no manual "Speichern" needed.
  useEffect(() => {
    if (!profilesLoaded.current) return;
    invoke("save_profiles", { profiles }).catch(() => {});
  }, [profiles]);

  // Disable the webview right-click menu (reload/inspect) so it feels like a real app.
  useEffect(() => {
    const block = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", block);
    return () => document.removeEventListener("contextmenu", block);
  }, []);

  function patch<K extends keyof Profile>(key: K, value: Profile[K]) {
    setProfiles((ps) => ps.map((p, i) => (i === active ? { ...p, [key]: value } : p)));
  }

  async function detectJava() {
    const installs = await invoke<JavaInstall[]>("detect_java");
    setJavaInstalls(installs);
    if (installs.length > 0) patch("java_path", installs[0].path);
  }

  async function saveProfiles() {
    await invoke("save_profiles", { profiles });
    setLogs((l) => [...l, "Profile gespeichert."]);
  }

  function addProfile() {
    setProfiles((ps) => [...ps, { ...DEFAULT_PROFILE, name: `Profil ${ps.length + 1}` }]);
    setActive(profiles.length);
    setView("settings");
  }

  // Deletes a profile (keeps at least one). Auto-persists via the profiles effect.
  function deleteProfile(i: number) {
    setProfiles((ps) => {
      if (ps.length <= 1) return ps; // never delete the last profile
      const next = ps.filter((_, idx) => idx !== i);
      setActive((a) => Math.min(a, next.length - 1));
      return next;
    });
  }

  async function importModpack() {
    const path = await open({
      multiple: false,
      filters: [{ name: "Modpack", extensions: ["mrpack", "celarispack", "zip"] }],
    });
    if (!path || typeof path !== "string") return;
    setImporting(true);
    try {
      const imported = await invoke<Profile>("import_modpack", { path });
      const next = [...profiles, imported];
      setProfiles(next);
      setActive(next.length - 1);
      await invoke("save_profiles", { profiles: next });
      setLogs((l) => [...l, `Modpack importiert: ${imported.name}`]);
    } catch (e) {
      setLogs((l) => [...l, `Import fehlgeschlagen: ${String(e)}`]);
    } finally {
      setImporting(false);
    }
  }

  // Opens the "what to export?" dialog for a profile.
  function exportProfile(p: Profile) {
    setExportOpts({ mods: true, resourcepacks: true, shaderpacks: true, config: true, options: true });
    setExportTarget(p);
  }

  // Confirms the export: pick a destination, then write the selected categories.
  async function doExport() {
    const p = exportTarget;
    if (!p) return;
    const dest = await save({
      defaultPath: `${p.name}.celarispack`,
      filters: [{ name: "Celaris Pack", extensions: ["celarispack"] }],
    });
    if (!dest) return;
    setExportTarget(null);
    try {
      await invoke("export_celarispack", { profile: p, dest, opts: exportOpts });
      setLogs((l) => [...l, `Exportiert nach ${dest}`]);
    } catch (e) {
      setLogs((l) => [...l, `Export fehlgeschlagen: ${String(e)}`]);
    }
  }

  function renderExportModal() {
    type K = keyof typeof exportOpts;
    const rows: [K, string][] = [
      ["mods", "Mods"],
      ["resourcepacks", "Resourcepacks"],
      ["shaderpacks", "Shader"],
      ["config", "Konfiguration (config/)"],
      ["options", "Einstellungen (options.txt)"],
    ];
    const ov: React.CSSProperties = { position: "fixed", inset: 0, background: "rgba(5,7,12,0.7)", display: "flex", alignItems: "center", justifyContent: "center", zIndex: 1000 };
    const card: React.CSSProperties = { background: "#12161f", border: "1px solid #232a38", borderRadius: 12, padding: 22, width: 360, maxWidth: "90vw" };
    const toggle = (k: K) => setExportOpts((o) => ({ ...o, [k]: !o[k] }));
    return (
      <div style={ov} onClick={() => setExportTarget(null)}>
        <div style={card} onClick={(e) => e.stopPropagation()}>
          <h3 style={{ margin: "0 0 4px", color: "#e9edf6" }}>Exportieren: {exportTarget?.name}</h3>
          <div style={{ color: "#aab2c0", fontSize: 13, marginBottom: 14 }}>Was soll in den Pack?</div>
          {rows.map(([k, label]) => (
            <label key={k} style={{ display: "flex", alignItems: "center", gap: 10, padding: "7px 0", color: "#e9edf6", cursor: "pointer" }}>
              <input type="checkbox" checked={exportOpts[k]} onChange={() => toggle(k)} />
              {label}
            </label>
          ))}
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 16 }}>
            <Button className="btn--ghost" onClick={() => setExportTarget(null)}>Abbrechen</Button>
            <PrimaryButton onClick={doExport}>Exportieren …</PrimaryButton>
          </div>
        </div>
      </div>
    );
  }

  async function loadGlobalPacks() {
    invoke<GlobalPack[]>("list_global_modpacks").then(setGlobalPacks).catch(() => setGlobalPacks([]));
    invoke<ServerModpack[]>("list_server_modpacks").then(setServerPacks).catch(() => setServerPacks([]));
  }

  async function installServerPack(pack: ServerModpack) {
    setInstallingSrvPack(pack.slug);
    try {
      const created = await invoke<Profile>("install_server_modpack", { slug: pack.slug });
      const next = await invoke<Profile[]>("get_profiles");
      setProfiles(next.length > 0 ? next : [...profiles, created]);
      setActive(Math.max(0, (next.length > 0 ? next : profiles).findIndex((p) => p.name === created.name)));
      setLogs((l) => [...l, `Celaris-Modpack installiert: ${created.name}`]);
    } catch (e) {
      setLogs((l) => [...l, `Installation fehlgeschlagen: ${String(e)}`]);
    } finally {
      setInstallingSrvPack(null);
    }
  }

  async function installGlobalPack(pack: GlobalPack) {
    setInstallingPack(pack.url);
    try {
      const imported = await invoke<Profile>("install_global_modpack", { url: pack.url });
      const next = [...profiles, imported];
      setProfiles(next);
      setActive(next.length - 1);
      await invoke("save_profiles", { profiles: next });
      setLogs((l) => [...l, `Modpack installiert: ${imported.name}`]);
    } catch (e) {
      setLogs((l) => [...l, `Installation fehlgeschlagen: ${String(e)}`]);
    } finally {
      setInstallingPack(null);
    }
  }

  async function loadVersions() {
    try {
      setVersions(await invoke<McVersion[]>("list_versions"));
    } catch {
      /* keep free-text fallback */
    }
  }

  // --- Wardrobe ---
  async function loadWardrobe() {
    try {
      setSkins(await invoke<SkinInfo[]>("list_wardrobe"));
    } catch {
      setSkins([]);
    }
  }

  async function grabSkin() {
    if (!grabQuery.trim()) return;
    setGrabbing(true);
    setSkinMsg(null);
    try {
      await invoke<SkinInfo>("grab_skin", { query: grabQuery.trim() });
      setGrabQuery("");
      await loadWardrobe();
    } catch (e) {
      setSkinMsg(String(e));
    } finally {
      setGrabbing(false);
    }
  }

  async function importSkin() {
    const path = await open({
      multiple: false,
      filters: [{ name: "Skin (PNG)", extensions: ["png"] }],
    });
    if (!path || typeof path !== "string") return;
    const label = path.split(/[/\\]/).pop()?.replace(/\.png$/i, "") ?? "Skin";
    try {
      await invoke("import_skin", { path, label });
      await loadWardrobe();
    } catch (e) {
      setSkinMsg(String(e));
    }
  }

  async function removeSkin(id: string) {
    try {
      await invoke("remove_skin", { id });
      await loadWardrobe();
    } catch {
      /* ignore */
    }
  }

  /**
   * Returns a fresh Microsoft access token, refreshing it first for Microsoft
   * accounts (cached tokens expire ~1h → otherwise skin upload / social 401).
   */
  async function freshAccessToken(): Promise<string | null> {
    let token = session?.access_token ?? null;
    const active = accounts.find((a) => a.uuid === session?.uuid);
    if (!active || active.kind !== "microsoft") {
      return token;
    }
    if (!active.refresh_token) {
      setLogs((l) => [...l, "⚠ Kein Refresh-Token gespeichert — bitte EINMAL neu mit Microsoft anmelden (danach bleibst du dauerhaft eingeloggt)."]);
      return token;
    }
    try {
      const fresh = await invoke<OnlineLogin>("refresh_account", { refreshToken: active.refresh_token });
      token = fresh.session.access_token;
      const updated = accounts.map((a) =>
        a.uuid === active.uuid
          ? { ...a, ...fresh.session, refresh_token: fresh.refresh_token || a.refresh_token }
          : a
      );
      await persistAccounts(updated);
      setSession(fresh.session);
    } catch (e) {
      setLogs((l) => [...l, `⚠ Token-Refresh fehlgeschlagen: ${String(e)} — ggf. einmal neu anmelden.`]);
    }
    return token;
  }

  async function applySkin(id: string, slim = false) {
    if (!session || session.user_type !== "msa") {
      setSkinMsg("Skin anwenden benötigt einen Microsoft-Account (wähle ihn oben aus).");
      return;
    }
    setSkinMsg("Skin wird angewendet…");
    try {
      const token = (await freshAccessToken()) ?? session.access_token;
      await invoke("apply_skin", { id, accessToken: token, slim });
      // Show the applied skin on the 3D body immediately (local PNG = no CDN lag).
      const applied = skins.find((s) => s.id === id);
      if (applied?.png_base64) {
        setAppliedSkin({ dataUrl: `data:image/png;base64,${applied.png_base64}`, slim });
      }
      setEquippedSkinId(id);
      localStorage.setItem("celaris-equipped-skin", id);
      setSkinReload(Date.now());
      setSkinMsg("Skin angewendet ✓ — ausgerüstet. (Im Spiel kann es 1–2 Min dauern, ggf. neu verbinden.)");
    } catch (e) {
      setSkinMsg(String(e));
    }
  }

  // --- Servers ---
  async function loadServers() {
    const [p, u] = await Promise.all([
      invoke<ServerEntry[]>("list_partner_servers").catch(() => [] as ServerEntry[]),
      invoke<ServerEntry[]>("get_servers").catch(() => [] as ServerEntry[]),
    ]);
    setPartners(p);
    setUserServers(u);
    // Ping each server for its banner (favicon) + player count.
    const seen = new Set<string>();
    [...p, ...u].forEach((s) => {
      const addr = s.address?.trim();
      if (!addr || seen.has(addr.toLowerCase())) return;
      seen.add(addr.toLowerCase());
      invoke<ServerStatus>("ping_server", { address: addr })
        .then((st) => setServerStatus((m) => ({ ...m, [addr]: st })))
        .catch(() => {});
    });
  }

  // Banner/icon (data URL) for a server: live favicon → partner-provided icon → none.
  function serverBanner(s: ServerEntry): string | null {
    const st = serverStatus[s.address];
    if (st?.icon) return st.icon;
    if (s.icon) return s.icon.startsWith("data:") ? s.icon : `data:image/png;base64,${s.icon}`;
    return null;
  }

  async function addServer() {
    if (!srvName.trim() || !srvAddr.trim()) return;
    const next = [
      ...userServers,
      { name: srvName.trim(), address: srvAddr.trim(), partner: false, description: null, icon: null, banner: null },
    ];
    setUserServers(next);
    setSrvName("");
    setSrvAddr("");
    await invoke("save_servers", { servers: next });
  }

  async function removeServer(idx: number) {
    const next = userServers.filter((_, i) => i !== idx);
    setUserServers(next);
    await invoke("save_servers", { servers: next });
  }

  async function syncServers() {
    setServerMsg(null);
    try {
      const count = await invoke<number>("sync_servers", { profile });
      setServerMsg(`${count} Server ins Spiel übernommen (Profil „${profile.name}").`);
    } catch (e) {
      setServerMsg(`Fehlgeschlagen: ${String(e)}`);
    }
  }

  async function loadNews() {
    setNewsLoading(true);
    try {
      setNews(await invoke<NewsItem[]>("fetch_news"));
    } catch {
      setNews([]);
    } finally {
      setNewsLoading(false);
    }
  }

  // --- Social / friends ---
  async function connectSocial() {
    try {
      // Refresh the (possibly expired) MS token first, else the server replies
      // "Anmeldung erforderlich".
      const token = await freshAccessToken();
      await invoke("social_connect", { username: playerName, uuid: session?.uuid ?? null, accessToken: token });
      setSocialConnected(true);
    } catch (e) {
      setChat((c) => [...c, { from: "", text: `Verbindung fehlgeschlagen: ${String(e)}`, system: true }]);
    }
  }

  async function addFriend(name?: string) {
    const target = (name ?? addFriendInput).trim();
    if (!target) return;
    try {
      await invoke("social_friend_add", { username: target });
      setAddFriendInput("");
      setChat((c) => [...c, { from: "", text: `Freundschaftsanfrage an ${target} gesendet ✓`, system: true }]);
    } catch (e) {
      setChat((c) => [...c, { from: "", text: `Konnte Anfrage nicht senden: ${String(e)}`, system: true }]);
    }
  }
  async function acceptFriend(name: string) {
    await invoke("social_friend_accept", { username: name }).catch(() => {});
  }
  async function removeFriend(name: string) {
    await invoke("social_friend_remove", { username: name }).catch(() => {});
  }


  function reportPresence() {
    if (!socialConnected) return;
    const inGame = launchState === "launched";
    const status = inGame ? `Im Spiel · ${profile.name}` : "Im Launcher";
    const playtime = inGame && playStartRef.current ? Math.floor((Date.now() - playStartRef.current) / 1000) : 0;
    invoke("social_set_presence", { status, server: null, playtimeSecs: playtime }).catch(() => {});
  }

  async function sendChat() {
    if (!chatInput.trim()) return;
    try {
      await invoke("social_send_chat", { text: chatInput.trim() });
      setChatInput("");
    } catch (e) {
      setChat((c) => [...c, { from: "", text: String(e), system: true }]);
    }
  }

  async function shareScreenshot() {
    try {
      const name = await invoke<string>("social_share_screenshot", { profile });
      setChat((c) => [...c, { from: "", text: `Screenshot geteilt: ${name}`, system: true }]);
    } catch (e) {
      setChat((c) => [...c, { from: "", text: `Screenshot: ${String(e)}`, system: true }]);
    }
  }

  function accountToSession(a: Account): Session {
    return { username: a.username, uuid: a.uuid, access_token: a.access_token, user_type: a.user_type };
  }

  async function persistAccounts(next: Account[]) {
    setAccounts(next);
    try {
      await invoke("save_accounts", { accounts: next });
    } catch (e) {
      console.error("save_accounts", e);
    }
  }

  async function addOffline() {
    const name = offlineName.trim();
    if (!name) return;
    const sess = await invoke<Session>("offline_session", { username: name });
    const acc: Account = { kind: "offline", username: sess.username, uuid: sess.uuid, access_token: sess.access_token, user_type: sess.user_type };
    await persistAccounts([...accounts.filter((a) => a.uuid !== acc.uuid), acc]);
    setSession(accountToSession(acc));
    setOfflineName("");
  }

  async function addMicrosoft() {
    setLoginBusy(true);
    setLoginError(null);
    try {
      const login = await invoke<OnlineLogin>("microsoft_login");
      const sess = login.session;
      const acc: Account = {
        kind: "microsoft",
        username: sess.username,
        uuid: sess.uuid,
        access_token: sess.access_token,
        user_type: sess.user_type,
        refresh_token: login.refresh_token,
      };
      await persistAccounts([...accounts.filter((a) => a.uuid !== acc.uuid), acc]);
      setSession(accountToSession(acc));
      setAccountMenu(false);
    } catch (err) {
      setLoginError(String(err));
    } finally {
      setLoginBusy(false);
    }
  }

  function selectAccount(a: Account) {
    setSession(accountToSession(a));
    setAccountMenu(false);
  }

  async function removeAccount(a: Account) {
    const next = accounts.filter((x) => x.uuid !== a.uuid);
    await persistAccounts(next);
    if (session && session.uuid === a.uuid) {
      setSession(next.length ? accountToSession(next[0]) : null);
    }
  }

  async function start(server?: string) {
    setView("play");
    setLaunchState("running");
    setLaunchError(null);
    setLogs([]);
    setProgress(null);

    // For Microsoft accounts, refresh the (likely expired) token first so the
    // user stays logged in across restarts without re-authenticating.
    let launchSession = session;
    const active = accounts.find((a) => a.uuid === session?.uuid);
    if (active && active.kind === "microsoft" && active.refresh_token) {
      try {
        const fresh = await invoke<OnlineLogin>("refresh_account", { refreshToken: active.refresh_token });
        launchSession = fresh.session;
        const updated = accounts.map((a) =>
          a.uuid === active.uuid
            ? { ...a, ...fresh.session, refresh_token: fresh.refresh_token || a.refresh_token }
            : a
        );
        await persistAccounts(updated);
        setSession(fresh.session);
      } catch (e) {
        console.error("token refresh failed, using cached session", e);
      }
    }

    try {
      await invoke("launch", { profile, session: launchSession, server: server ?? null });
      setLaunchState("launched");
    } catch {
      setLaunchState((s) => (s === "running" ? "error" : s));
    }
  }

  /** Launch the active profile and connect straight to a server. */
  function joinServer(address: string) {
    if (address && address.trim()) start(address.trim());
  }

  const playerName = session ? session.username : profile.name || "Player";
  const stepIndex = progress ? STEPS.findIndex((s) => s.key === progress.stage) : -1;
  const downloadPct =
    progress && progress.stage === "download" && progress.total > 0
      ? Math.round((progress.current / progress.total) * 100)
      : null;

  // The profile whose mod list the Mods view is editing.
  const modProf = profiles[active] ?? profiles[0] ?? DEFAULT_PROFILE;

  async function loadInstalled() {
    try {
      setInstalled(await invoke<InstalledMod[]>("list_installed_mods", { profile: modProf.name }));
    } catch {
      setInstalled([]);
    }
  }

  // Check (on demand) which installed mods have a newer version — never auto-update.
  async function checkModUpdates() {
    try {
      const upd = await invoke<string[]>("check_mod_updates", {
        profile: modProf.name,
        mcVersion: modProf.minecraft_version,
      });
      setModUpdates(new Set(upd));
    } catch {
      setModUpdates(new Set());
    }
  }

  async function updateMod(filename: string) {
    setUpdatingMod(filename);
    try {
      await invoke("update_mod", { profile: modProf.name, mcVersion: modProf.minecraft_version, filename });
      await loadInstalled();
      await checkModUpdates();
    } catch (e) {
      setLogs((l) => [...l, `Update fehlgeschlagen: ${String(e)}`]);
    } finally {
      setUpdatingMod(null);
    }
  }

  async function searchMods(page = 0) {
    setSearching(true);
    try {
      const hits = await invoke<ModHit[]>("search_mods", {
        query: modQuery,
        mcVersion: modProf.minecraft_version,
        offset: page * 30,
        sort: modSort,
      });
      setModResults(hits);
      setModPage(page);
    } catch (e) {
      setModResults([]);
      setLogs((l) => [...l, `Suche fehlgeschlagen: ${String(e)}`]);
    } finally {
      setSearching(false);
    }
  }

  async function installMod(hit: ModHit) {
    setInstalling(hit.project_id);
    try {
      await invoke("install_mod", {
        projectId: hit.project_id,
        mcVersion: modProf.minecraft_version,
        profile: modProf.name,
        title: hit.title,
        iconUrl: hit.icon_url,
      });
      await loadInstalled();
    } catch (e) {
      setLogs((l) => [...l, `Installation fehlgeschlagen: ${String(e)}`]);
    } finally {
      setInstalling(null);
    }
  }

  async function removeMod(filename: string) {
    try {
      await invoke("remove_mod", { filename, profile: modProf.name });
      await loadInstalled();
    } catch (e) {
      setLogs((l) => [...l, `Entfernen fehlgeschlagen: ${String(e)}`]);
    }
  }

  // Opens the per-profile mods folder so users can drop in their own .jar mods.
  async function openModsFolder() {
    try {
      const dir = await invoke<string>("mods_dir", { profile: modProf, open: true });
      setLogs((l) => [...l, `Mods-Ordner: ${dir}`]);
    } catch (e) {
      setLogs((l) => [...l, `Mods-Ordner konnte nicht geöffnet werden: ${String(e)}`]);
    }
  }

  function packCmds(kind: "resourcepack" | "shader") {
    return kind === "resourcepack"
      ? { search: "search_resourcepacks", install: "install_resourcepack", list: "list_resourcepacks", remove: "remove_resourcepack" }
      : { search: "search_shaders", install: "install_shader", list: "list_shaders", remove: "remove_shader" };
  }

  async function loadPackInstalled() {
    invoke<InstalledMod[]>(packCmds(packKind).list, { profile: modProf.name })
      .then(setPackInstalled)
      .catch(() => setPackInstalled([]));
  }

  async function searchPacks(q: string = packQuery) {
    setPackSearching(true);
    try {
      setPackResults(await invoke<ModHit[]>(packCmds(packKind).search, { query: q, mcVersion: modProf.minecraft_version, sort: packSort }));
    } catch {
      setPackResults([]);
    } finally {
      setPackSearching(false);
    }
  }

  async function installPack(hit: ModHit) {
    setPackInstalling(hit.project_id);
    try {
      await invoke(packCmds(packKind).install, { projectId: hit.project_id, mcVersion: modProf.minecraft_version, profile: modProf.name, title: hit.title, iconUrl: hit.icon_url });
      await loadPackInstalled();
    } catch (e) {
      setLogs((l) => [...l, `Installation fehlgeschlagen: ${String(e)}`]);
    } finally {
      setPackInstalling(null);
    }
  }

  async function removePack(filename: string) {
    try {
      await invoke(packCmds(packKind).remove, { filename, profile: modProf.name });
      await loadPackInstalled();
    } catch (e) {
      setLogs((l) => [...l, `Entfernen fehlgeschlagen: ${String(e)}`]);
    }
  }

  useEffect(() => {
    if (view === "mods") { loadInstalled(); searchMods(0); }
    if (view === "wardrobe") loadWardrobe();
    if (view === "servers") loadServers();
    if (view === "news") loadNews();
    if (view === "profiles") loadGlobalPacks();
    if (view === "friends" && !socialConnected) connectSocial();
    if (view === "resourcepacks" || view === "shaders") {
      setPackQuery("");
      loadPackInstalled();
      searchPacks("");
    }
    if (view === "shaders") {
      invoke<boolean>("profile_has_shaders", { profile: modProf.name }).then(setHasShaderLoader).catch(() => setHasShaderLoader(false));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, packKind]);

  // Reload the installed list when the selected mod-profile changes.
  useEffect(() => {
    if (view === "mods") loadInstalled();
    if (view === "resourcepacks" || view === "shaders") loadPackInstalled();
    if (view === "shaders") {
      invoke<boolean>("profile_has_shaders", { profile: modProf.name }).then(setHasShaderLoader).catch(() => setHasShaderLoader(false));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  // When the "Installed" mods tab opens, check for available updates (on demand).
  useEffect(() => {
    if (view === "mods" && modsTab === "installed") checkModUpdates();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, modsTab, active]);

  useEffect(() => {
    if (launchState === "launched") playStartRef.current = Date.now();
    reportPresence();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [launchState, socialConnected]);

  useEffect(() => {
    if (!socialConnected) return;
    const id = setInterval(reportPresence, 30000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [socialConnected, launchState]);

  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chat]);


  useEffect(() => {
    loadVersions();
    invoke<typeof appVersions>("version_info")
      .then(setAppVersions)
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Match installed mods by the Modrinth project id recorded at install time
  // (exact), falling back to the internal id for manually-added jars.
  const installedIds = new Set(installed.flatMap((m) => [m.project_id, m.id].filter(Boolean) as string[]));
  const visibleVersions = versions.filter((v) => showSnapshots || v.kind === "release");

  return (
    <>
      {theme === "matrix" ? (
        <MatrixRain />
      ) : (
        <SpaceBackground
          bg={THEMES.find((t) => t.key === theme)?.bg}
          space={!!THEMES.find((t) => t.key === theme)?.space}
        />
      )}
      {updateMsg && <div className="update-toast">{updateMsg}</div>}
      <div className="app-shell">
        <TitleBar />

      {/* ----- Sidebar ----- */}
      <aside className="sidebar">
        <div className="brand">
          <img className="brand-mark-img" src="/celaris-logo.png" alt="Celaris" />
          <div className="brand-name">
            CELARIS
            <small>LAUNCHER</small>
          </div>
        </div>

        <div className="nav-label">Menü</div>
        <SidebarItem icon={<PlayIcon />} label="Play" active={view === "play"} onClick={() => setView("play")} />
        <SidebarItem icon={<NewsIcon />} label="Zeitung" active={view === "news"} onClick={() => setView("news")} />
        <SidebarItem icon={<FriendsIcon />} label="Freunde" active={view === "friends"} onClick={() => setView("friends")} />
        <SidebarItem icon={<ProfilesIcon />} label="Profile" active={view === "profiles"} onClick={() => setView("profiles")} />
        <SidebarItem icon={<ModsIcon />} label="Mods" active={view === "mods"} onClick={() => setView("mods")} />
        <SidebarItem
          icon={<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M9 21V9"/></svg>}
          label="Texturen"
          active={view === "resourcepacks"}
          onClick={() => { setPackKind("resourcepack"); setView("resourcepacks"); }}
        />
        <SidebarItem
          icon={<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/></svg>}
          label="Shader"
          active={view === "shaders"}
          onClick={() => { setPackKind("shader"); setView("shaders"); }}
        />
        <SidebarItem icon={<ServerIcon />} label="Server" active={view === "servers"} onClick={() => setView("servers")} />
        <SidebarItem icon={<HostingIcon />} label="Hosting" active={view === "hosting"} onClick={() => setView("hosting")} />
        <SidebarItem icon={<CosmeticsIcon />} label="Cosmetics" active={view === "cosmetics"} onClick={() => setView("cosmetics")} />
        <SidebarItem icon={<WardrobeIcon />} label="Garderobe" active={view === "wardrobe"} onClick={() => setView("wardrobe")} />
        <SidebarItem
          icon={
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              <rect x="3" y="4" width="18" height="16" rx="2" />
              <path d="M7 9l3 3-3 3M13 15h4" />
            </svg>
          }
          label="Log"
          active={view === "logs"}
          onClick={() => setView("logs")}
        />
        <SidebarItem icon={<SettingsIcon />} label="Launcher" active={view === "launcher"} onClick={() => setView("launcher")} />
        <SidebarItem icon={<CreditsIcon />} label="Credits" active={view === "credits"} onClick={() => setView("credits")} />

        <div className="sidebar-spacer" />
        <div className="sidebar-foot">
          <div>Launcher {appVersions.launcher || "…"}</div>
          <div>Client {appVersions.client || "…"}</div>
        </div>
      </aside>

      {/* ----- Top bar ----- */}
      <header className="topbar">
        {appVersions.update_available && (
          <button
            className="update-pill"
            title="Launcher-Update herunterladen"
            onClick={() => invoke("open_external", { url: appVersions.update_url }).catch(() => {})}
          >
            ↑ Update verfügbar: {appVersions.launcher_latest} — Download
          </button>
        )}
        <span className="chip">
          <span className="chip-key">Ver</span>
          <strong>{profile.minecraft_version}</strong>
        </span>
        <span className="chip">
          <span className="chip-key">RAM</span>
          <strong>{(profile.max_ram_mb / 1024).toFixed(1)} GB</strong>
        </span>
        <div className="account-wrap">
          <div className="account-chip" onClick={() => setAccountMenu((o) => !o)} title="Accounts verwalten">
            {equippedSkin ? (
              <div className="account-avatar" style={{ overflow: "hidden", padding: 0 }}>
                <SkinFace png={equippedSkin.png_base64} size={30} />
              </div>
            ) : (
              <img
                className="account-avatar"
                style={{ objectFit: "cover", imageRendering: "pixelated" }}
                src={`https://mc-heads.net/avatar/${encodeURIComponent(playerName || "MHF_Steve")}/64?ts=${skinReload}`}
                alt=""
              />
            )}
            <div className="account-meta">
              <b>{playerName}</b>
              <span>
                {session ? (session.user_type === "msa" ? "MICROSOFT" : "OFFLINE") : loginBusy ? "ANMELDEN…" : "KEIN ACCOUNT"}
              </span>
            </div>
            <span className="account-caret">▾</span>
          </div>
          {accountMenu && (
            <div className="account-menu">
              <div className="account-menu-title">Accounts</div>
              {accounts.length === 0 && <div className="account-menu-empty">Noch keine Accounts.</div>}
              {accounts.map((a, i) => (
                <div key={i} className={`account-row ${session?.uuid === a.uuid ? "active" : ""}`}>
                  <img
                    className="account-avatar sm"
                    style={{ objectFit: "cover", imageRendering: "pixelated" }}
                    src={`https://mc-heads.net/avatar/${encodeURIComponent(a.username || "MHF_Steve")}/64`}
                    alt=""
                  />
                  <div className="account-row-meta" onClick={() => selectAccount(a)}>
                    <b>{a.username}</b>
                    <span>{a.kind === "microsoft" ? "Microsoft" : "Offline"}</span>
                  </div>
                  <button className="account-x" onClick={() => removeAccount(a)} title="Entfernen">✕</button>
                </div>
              ))}
              <div className="account-add">
                <input
                  value={offlineName}
                  onChange={(e) => setOfflineName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && addOffline()}
                  placeholder="Offline-Name…"
                />
                <button onClick={addOffline}>+ Offline</button>
              </div>
              <button className="account-ms" onClick={addMicrosoft} disabled={loginBusy}>
                {loginBusy ? "Anmelden…" : "+ Microsoft-Account"}
              </button>
              {loginError && <div className="account-err">{loginError}</div>}
            </div>
          )}
        </div>
      </header>

      {/* ----- Main content ----- */}
      <main className="main" key={view}>
        {view === "play" && renderPlay()}
        {view === "news" && renderNews()}
        {view === "friends" && renderFriends()}
        {view === "profiles" && renderProfiles()}
        {view === "mods" && renderMods()}
        {view === "servers" && renderServers()}
        {view === "hosting" && renderHosting()}
        {view === "cosmetics" && renderCosmetics()}
        {view === "credits" && renderCredits()}
        {view === "logs" && renderLogs()}
        {(view === "resourcepacks" || view === "shaders") && renderPackMarket()}
        {view === "wardrobe" && renderWardrobe()}
        {view === "settings" && renderSettings()}
        {view === "launcher" && renderLauncher()}
      </main>
      </div>
      {exportTarget && renderExportModal()}
    </>
  );

  /* ----------------------------------------------------------------------- */

  function renderPlay() {
    return (
      <div className="play view">
        <SkinBody
          username={playerName}
          skinDataUrl={appliedSkin?.dataUrl ?? equippedSkinDataUrl}
          slim={appliedSkin?.slim}
          reloadKey={skinReload}
        />
        <div className="play-hero">
          <span className="play-eyebrow">{launchState === "idle" ? "Bereit zum Start" : "Sitzung"}</span>
          <h1 className="play-title">{profile.name}</h1>
          <div className="play-summary">
            <span className="chip"><span className="chip-key">MC</span><strong>{profile.minecraft_version}</strong></span>
            <span className="chip"><span className="chip-key">RAM</span><strong>{(profile.max_ram_mb / 1024).toFixed(1)} GB</strong></span>
            {profile.use_celaris_client ? (
              <StatusBadge tone="accent" dot>Celaris + Fabric</StatusBadge>
            ) : (
              <StatusBadge tone="muted" dot>Vanilla</StatusBadge>
            )}
          </div>
          {profile.use_celaris_client && (
            <div className="version-note">
              <span className="version-note__icon">⇄</span>
              <span className="version-note__text">
                <strong>Multi-Version dank ViaFabricPlus.</strong> Der Client läuft auf 1.21.11,
                verbindet sich aber per Auto-Erkennung mit Servern von 1.7 bis 26.2.
              </span>
            </div>
          )}

          {launchState === "idle" ? (
            <div className="play-cta">
              <button className="play-btn" onClick={() => start()}>SPIELEN</button>
              <span style={{ color: "var(--text-faint)", fontSize: "0.82rem" }}>
                {session ? "Microsoft" : "Offline"} · {playerName}
              </span>
            </div>
          ) : (
            renderLaunchScreen()
          )}

          {deviceCode && (
            <div className="device-code">
              Öffne <a href={deviceCode.verification_uri} target="_blank">{deviceCode.verification_uri}</a> und gib den Code ein:
              <div className="code">{deviceCode.user_code}</div>
            </div>
          )}
          {loginError && (
            <div className="banner banner--error" style={{ marginTop: "var(--s4)" }}>
              <span className="banner-title">Login fehlgeschlagen</span>
              <span className="banner-msg">{loginError}</span>
            </div>
          )}
        </div>
      </div>
    );
  }

  function renderLaunchScreen() {
    return (
      <div className="launch-screen">
        {launchState === "error" && launchError ? (
          <div className="banner banner--error">
            <span className="banner-title">Fehler beim {STAGE_LABELS[launchError.stage] ?? launchError.stage}</span>
            <span className="banner-msg">{ERROR_MESSAGES[launchError.code] ?? launchError.message}</span>
            <details>
              <summary>Details</summary>
              <code>[{launchError.stage}/{launchError.code}] {launchError.message}</code>
            </details>
            <div style={{ marginTop: "var(--s3)" }}>
              <Button onClick={() => setLaunchState("idle")}>Zurück</Button>
            </div>
          </div>
        ) : (
          <>
            <div className="ls-title">
              {launchState === "launched" ? "Gestartet" : STATUS_TITLES[progress?.stage ?? ""] ?? "Wird vorbereitet…"}
            </div>

            <div className="stage-indicator">
              {STEPS.map((step, i) => {
                const done = launchState === "launched" || (stepIndex >= 0 && i < stepIndex);
                const activeStep = launchState === "running" && i === stepIndex;
                return (
                  <div key={step.key} className={`stage ${done ? "done" : ""} ${activeStep ? "active" : ""}`}>
                    <span className="stage-dot">{done ? "✓" : i + 1}</span>
                    <span className="stage-name">{step.label}</span>
                  </div>
                );
              })}
            </div>

            {launchState === "launched" ? (
              <div className="banner banner--success">
                <span className="banner-title">Minecraft gestartet ✓</span>
                <span className="banner-msg">Viel Spaß. Du kannst dieses Fenster offen lassen.</span>
                <div style={{ marginTop: "var(--s3)" }}>
                  <Button onClick={() => setLaunchState("idle")}>Zurück</Button>
                </div>
              </div>
            ) : (
              <>
                <ProgressBar value={downloadPct} />
                <div className="launch-status">
                  <span>{progress?.message ?? "…"}</span>
                  {downloadPct !== null && <span className="pct">{downloadPct}%</span>}
                </div>
              </>
            )}
          </>
        )}
      </div>
    );
  }

  function renderProfiles() {
    return (
      <div className="view">
        <div className="view-head">
          <div>
            <h2 className="view-title">Profile</h2>
            <div className="view-sub">Wähle ein Profil, importiere ein Modpack oder lege ein neues an.</div>
          </div>
          <div style={{ display: "flex", gap: "var(--s2)" }}>
            <Button onClick={importModpack} disabled={importing}>
              {importing ? "Importiert…" : "⬇ Modpack importieren"}
            </Button>
            <Button onClick={addProfile}>+ Neues Profil</Button>
          </div>
        </div>

        {serverPacks.length > 0 && (
          <>
            <div className="nav-label" style={{ padding: "0 0 var(--s2)" }}>Celaris-Modpacks</div>
            <div className="grid grid--cards stagger" style={{ marginBottom: "var(--s6)" }}>
              {serverPacks.map((p) => (
                <Card key={p.slug} className="modcard">
                  <div className="modcard-head">
                    {p.icon_url ? (
                      <img className="modcard-icon" src={p.icon_url} alt="" />
                    ) : (
                      <div className="modcard-icon modcard-icon--ph">{p.name.charAt(0)}</div>
                    )}
                    <div className="modcard-titles">
                      <div className="modcard-title">{p.name}</div>
                      <div className="modcard-author">
                        {p.mc_version}{p.mods.length ? ` · ${p.mods.length} Mods` : ""}
                      </div>
                    </div>
                  </div>
                  {p.description && <div className="modcard-desc">{p.description}</div>}
                  <div className="modcard-foot">
                    <PrimaryButton disabled={installingSrvPack === p.slug} onClick={() => installServerPack(p)}>
                      {installingSrvPack === p.slug ? "Installiert…" : "Installieren"}
                    </PrimaryButton>
                  </div>
                </Card>
              ))}
            </div>
          </>
        )}

        {globalPacks.length > 0 && (
          <>
            <div className="nav-label" style={{ padding: "0 0 var(--s2)" }}>Globale Modpacks</div>
            <div className="grid grid--cards stagger" style={{ marginBottom: "var(--s6)" }}>
              {globalPacks.map((p) => (
                <Card key={p.url} className="modcard">
                  <div className="modcard-head">
                    {p.icon_url ? (
                      <img className="modcard-icon" src={p.icon_url} alt="" />
                    ) : (
                      <div className="modcard-icon modcard-icon--ph">{p.name.charAt(0)}</div>
                    )}
                    <div className="modcard-titles">
                      <div className="modcard-title">{p.name}</div>
                      <div className="modcard-author">{p.mc_version}{p.loader ? ` · ${p.loader}` : ""}</div>
                    </div>
                  </div>
                  {p.description && <div className="modcard-desc">{p.description}</div>}
                  <div className="modcard-foot">
                    <PrimaryButton disabled={installingPack === p.url} onClick={() => installGlobalPack(p)}>
                      {installingPack === p.url ? "Installiert…" : "Installieren"}
                    </PrimaryButton>
                  </div>
                </Card>
              ))}
            </div>
            <div className="nav-label" style={{ padding: "0 0 var(--s2)" }}>Deine Profile</div>
          </>
        )}

        <div className="grid grid--cards stagger">
          {profiles.map((p, i) => (
            <Card
              key={i}
              interactive
              className={`profile-card ${i === active ? "selected" : ""}`}
              onClick={() => setActive(i)}
            >
              <div className="profile-card-head">
                <div>
                  <div className="profile-name">{p.name}</div>
                  <div className="profile-version">Minecraft {p.minecraft_version}</div>
                </div>
                {i === active && <StatusBadge tone="accent" dot>Aktiv</StatusBadge>}
              </div>

              <div className="profile-stats">
                <div className="stat"><div className="k">RAM</div><div className="v">{(p.max_ram_mb / 1024).toFixed(1)} GB</div></div>
                <div className="stat"><div className="k">Loader</div><div className="v">{p.use_celaris_client || p.use_fabric ? "Fabric" : "Vanilla"}</div></div>
              </div>

              <div className="profile-card-foot">
                {p.use_celaris_client ? (
                  <StatusBadge tone="indigo">Celaris Client</StatusBadge>
                ) : p.use_fabric ? (
                  <StatusBadge tone="accent">Modpack</StatusBadge>
                ) : (
                  <StatusBadge tone="muted">Vanilla</StatusBadge>
                )}
                <Button
                  className="btn--ghost"
                  style={{ marginLeft: "auto" }}
                  onClick={(e) => {
                    e.stopPropagation();
                    setActive(i);
                    setView("settings");
                  }}
                >
                  Einstellungen
                </Button>
                <Button
                  className="btn--ghost"
                  onClick={(e) => {
                    e.stopPropagation();
                    exportProfile(p);
                  }}
                >
                  Export
                </Button>
                {profiles.length > 1 && (
                  <Button
                    className="btn--ghost"
                    title="Profil löschen"
                    onClick={(e) => {
                      e.stopPropagation();
                      if (confirm(`Profil „${p.name}" wirklich löschen?`)) deleteProfile(i);
                    }}
                  >
                    Löschen
                  </Button>
                )}
                {i === active && (
                  <PrimaryButton
                    onClick={(e) => {
                      e.stopPropagation();
                      start();
                    }}
                  >
                    Spielen
                  </PrimaryButton>
                )}
              </div>
            </Card>
          ))}

          <Card interactive className="profile-card" onClick={addProfile} style={{ display: "grid", placeItems: "center", color: "var(--text-faint)", minHeight: 180 }}>
            <div style={{ textAlign: "center" }}>
              <div style={{ fontSize: "2rem", lineHeight: 1 }}>+</div>
              <div style={{ marginTop: 8, fontFamily: "var(--font-mono)", fontSize: "0.72rem", letterSpacing: 1 }}>NEUES PROFIL</div>
            </div>
          </Card>
        </div>
      </div>
    );
  }

  function renderMods() {
    return (
      <div className="view">
        <div className="view-head">
          <div>
            <h2 className="view-title">Mods</h2>
            <div className="view-sub">Modrinth-Marketplace · Fabric · {modProf.minecraft_version}</div>
          </div>
          <div style={{ display: "flex", gap: "var(--s3)", alignItems: "center" }}>
            <label className="mod-profile-pick">
              <span>Profil</span>
              <select value={active} onChange={(e) => setActive(Number(e.target.value))}>
                {profiles.map((p, i) => (
                  <option key={i} value={i}>{p.name}</option>
                ))}
              </select>
            </label>
            <Button className="btn--ghost" onClick={openModsFolder} title="Eigene .jar-Mods hier ablegen">📁 Mods-Ordner</Button>
            <div className="tabs">
              <button className={`tab ${modsTab === "browse" ? "on" : ""}`} onClick={() => setModsTab("browse")}>Marketplace</button>
              <button className={`tab ${modsTab === "installed" ? "on" : ""}`} onClick={() => setModsTab("installed")}>
                Installiert{installed.length > 0 ? ` (${installed.length})` : ""}
              </button>
            </div>
          </div>
        </div>

        {modsTab === "browse" ? (
          <>
            <form
              className="search"
              onSubmit={(e) => {
                e.preventDefault();
                searchMods();
              }}
            >
              <input
                className="input"
                placeholder="Mods suchen (z. B. Sodium, JEI, Lithium)…"
                value={modQuery}
                onChange={(e) => setModQuery(e.target.value)}
              />
              <select className="input" style={{ flex: "0 0 auto", width: "auto", minWidth: "9rem" }} value={modSort}
                onChange={(e) => { setModSort(e.target.value); }}>
                <option value="relevance">Relevanz</option>
                <option value="newest">Neueste</option>
                <option value="updated">Aktualisiert</option>
                <option value="downloads">Downloads</option>
              </select>
              <PrimaryButton type="submit" disabled={searching}>{searching ? "Sucht…" : "Suchen"}</PrimaryButton>
            </form>

            {modResults.length === 0 ? (
              <Card>
                <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
                  {searching ? "Suche läuft…" : "Suche nach Fabric-Mods für deine Version und installiere sie mit einem Klick."}
                </div>
              </Card>
            ) : (
              <div className="grid grid--cards stagger">
                {modResults.map((m) => {
                  const isInstalled = installedIds.has(m.slug) || installedIds.has(m.project_id);
                  return (
                    <Card key={m.project_id} className="modcard">
                      <div className="modcard-head">
                        {m.icon_url ? (
                          <img className="modcard-icon" src={m.icon_url} alt="" />
                        ) : (
                          <div className="modcard-icon modcard-icon--ph">{m.title.charAt(0)}</div>
                        )}
                        <div className="modcard-titles">
                          <div className="modcard-title">{m.title}</div>
                          <div className="modcard-author">von {m.author} · {formatDownloads(m.downloads)} ⬇</div>
                        </div>
                      </div>
                      <div className="modcard-desc">{m.description}</div>
                      <div className="modcard-foot">
                        {isInstalled ? (
                          <StatusBadge tone="success" dot>Installiert</StatusBadge>
                        ) : (
                          <PrimaryButton
                            disabled={installing === m.project_id}
                            onClick={() => installMod(m)}
                          >
                            {installing === m.project_id ? "Installiert…" : "Installieren"}
                          </PrimaryButton>
                        )}
                      </div>
                    </Card>
                  );
                })}
              </div>
            )}
            {(modResults.length > 0 || modPage > 0) && (
              <div style={{ display: "flex", gap: 8, justifyContent: "center", alignItems: "center", marginTop: "var(--s4)" }}>
                <Button className="btn--ghost" disabled={modPage === 0 || searching} onClick={() => searchMods(modPage - 1)}>← Zurück</Button>
                <span style={{ color: "var(--text-faint)" }}>Seite {modPage + 1}</span>
                <Button className="btn--ghost" disabled={modResults.length < 30 || searching} onClick={() => searchMods(modPage + 1)}>Weiter →</Button>
              </div>
            )}
          </>
        ) : installed.length === 0 ? (
          <Card>
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
              Noch keine Mods installiert. Wechsle zum Marketplace und installiere welche.
            </div>
          </Card>
        ) : (
          <div className="grid stagger" style={{ gap: "var(--s3)" }}>
            {installed.map((m) => {
              const name = m.title || m.id;
              const canUpdate = modUpdates.has(m.filename);
              return (
                <div key={m.filename} className="mod-row">
                  {m.icon_url ? (
                    <img className="mod-icon" src={m.icon_url} alt="" />
                  ) : (
                    <div className="mod-icon">{name.charAt(0).toUpperCase()}</div>
                  )}
                  <div className="mod-info">
                    <div className="mod-name">{name}</div>
                    <div className="mod-meta">{m.filename}</div>
                  </div>
                  {canUpdate && (
                    <PrimaryButton disabled={updatingMod === m.filename} onClick={() => updateMod(m.filename)}>
                      {updatingMod === m.filename ? "Aktualisiert…" : "↑ Update"}
                    </PrimaryButton>
                  )}
                  <Button className="btn--ghost" onClick={() => removeMod(m.filename)}>Entfernen</Button>
                </div>
              );
            })}
          </div>
        )}
      </div>
    );
  }

  function renderFriends() {
    const fmtTime = (s: number) => {
      const h = Math.floor(s / 3600);
      const m = Math.floor((s % 3600) / 60);
      return h > 0 ? `${h}h ${m}m` : `${m}m`;
    };
    return (
      <div className="view friends">
        <div className="view-head">
          <div>
            <h2 className="view-title">Freunde</h2>
            <div className="view-sub">Freunde · Presence · Chat · Screenshots</div>
          </div>
          <div style={{ display: "flex", gap: "var(--s2)", alignItems: "center" }}>
            <form className="search" style={{ margin: 0 }} onSubmit={(e) => { e.preventDefault(); addFriend(); }}>
              <input
                className="input"
                placeholder="Freund hinzufügen…"
                value={addFriendInput}
                onChange={(e) => setAddFriendInput(e.target.value)}
                disabled={!socialConnected}
              />
              <Button type="submit" disabled={!socialConnected}>+ Hinzufügen</Button>
            </form>
            <StatusBadge tone={socialConnected ? "success" : "muted"} dot>
              {socialConnected ? "Verbunden" : "Getrennt"}
            </StatusBadge>
          </div>
        </div>

        <div className="friends-grid">
          <Card className="presence-panel">
            {friends.incoming.length > 0 && (
              <>
                <div className="nav-label" style={{ padding: "0 0 var(--s2)" }}>Anfragen ({friends.incoming.length})</div>
                {friends.incoming.map((name, i) => (
                  <div key={`in${i}`} className="presence-row">
                    <div className="presence-avatar">{name.charAt(0).toUpperCase()}</div>
                    <div className="presence-info"><div className="presence-name">{name}</div></div>
                    <button className="iconbtn iconbtn--ok" title="Annehmen" onClick={() => acceptFriend(name)}>✓</button>
                    <button className="iconbtn iconbtn--no" title="Ablehnen" onClick={() => removeFriend(name)}>✕</button>
                  </div>
                ))}
              </>
            )}

            {friends.outgoing.length > 0 && (
              <>
                <div className="nav-label" style={{ padding: "var(--s3) 0 var(--s2)" }}>Gesendete Anfragen ({friends.outgoing.length})</div>
                {friends.outgoing.map((name, i) => (
                  <div key={`out${i}`} className="presence-row">
                    <div className="presence-avatar">{name.charAt(0).toUpperCase()}</div>
                    <div className="presence-info">
                      <div className="presence-name">{name}</div>
                      <div className="presence-status">ausstehend…</div>
                    </div>
                    <button className="iconbtn iconbtn--no" title="Anfrage zurückziehen" onClick={() => removeFriend(name)}>✕</button>
                  </div>
                ))}
              </>
            )}

            <div className="nav-label" style={{ padding: "var(--s3) 0 var(--s2)" }}>Freunde ({friends.accepted.length})</div>
            {friends.accepted.length === 0 ? (
              <div style={{ color: "var(--text-faint)", fontSize: "0.82rem" }}>Noch keine Freunde — füge oben jemanden hinzu.</div>
            ) : (
              friends.accepted.map((name, i) => {
                const p = presence.find((x) => x.username === name);
                return (
                  <div key={`fr${i}`} className="presence-row">
                    <div className="presence-avatar">
                      {name.charAt(0).toUpperCase()}
                      <span className={`pres-dot ${p ? "on" : ""}`} />
                    </div>
                    <div className="presence-info">
                      <div className="presence-name">{name}</div>
                      <div className="presence-status">
                        {p ? p.status + (p.playtime_secs > 0 ? ` · ${fmtTime(p.playtime_secs)}` : "") : "offline"}
                      </div>
                    </div>
                    <button className="iconbtn iconbtn--no" title="Entfernen" onClick={() => removeFriend(name)}>✕</button>
                  </div>
                );
              })
            )}

            {presence.filter((u) => u.username !== playerName && !friends.accepted.includes(u.username)).length > 0 && (
              <>
                <div className="nav-label" style={{ padding: "var(--s4) 0 var(--s2)" }}>Online</div>
                {presence
                  .filter((u) => u.username !== playerName && !friends.accepted.includes(u.username))
                  .map((u, i) => (
                    <div key={`on${i}`} className="presence-row">
                      <div className="presence-avatar">{u.username.charAt(0).toUpperCase()}</div>
                      <div className="presence-info">
                        <div className="presence-name">{u.username}</div>
                        <div className="presence-status">{u.status}</div>
                      </div>
                      {!friends.outgoing.includes(u.username) && (
                        <button className="iconbtn iconbtn--add" title="Freund hinzufügen" onClick={() => addFriend(u.username)}>+</button>
                      )}
                    </div>
                  ))}
              </>
            )}
          </Card>

          <Card className="chat-panel">
            <div className="chat-log">
              {chat.length === 0 ? (
                <div style={{ color: "var(--text-faint)", fontSize: "0.85rem", textAlign: "center", marginTop: "var(--s5)" }}>
                  Noch keine Nachrichten.
                </div>
              ) : (
                chat.map((m, i) =>
                  m.system ? (
                    <div key={i} className="chat-system">{m.text}</div>
                  ) : (
                    <div key={i} className={`chat-msg ${m.from === playerName ? "chat-msg--me" : ""}`}>
                      <span className="chat-from">{m.from}</span>
                      {m.text}
                    </div>
                  )
                )
              )}
              <div ref={chatEndRef} />
            </div>
            <form className="chat-input" onSubmit={(e) => { e.preventDefault(); sendChat(); }}>
              <input
                className="input"
                placeholder={socialConnected ? "Nachricht…" : "Nicht verbunden"}
                disabled={!socialConnected}
                value={chatInput}
                onChange={(e) => setChatInput(e.target.value)}
              />
              <Button type="button" onClick={shareScreenshot} disabled={!socialConnected} title="Letzten Screenshot teilen">📷</Button>
              <PrimaryButton type="submit" disabled={!socialConnected}>Senden</PrimaryButton>
            </form>
          </Card>
        </div>

        {shots.length > 0 && (
          <>
            <div className="nav-label" style={{ padding: "var(--s5) 0 var(--s2)" }}>Geteilte Screenshots</div>
            <div className="shots-grid">
              {shots.map((s, i) => (
                <figure key={i} className="shot">
                  <img src={`data:image/png;base64,${s.data}`} alt="" />
                  <figcaption>{s.from} · {s.name}</figcaption>
                </figure>
              ))}
            </div>
          </>
        )}
      </div>
    );
  }

  function renderNews() {
    const today = new Date().toLocaleDateString("de-DE", {
      weekday: "long",
      year: "numeric",
      month: "long",
      day: "numeric",
    });
    // A FULL release only (e.g. "26.2") — Mojang tags snapshots/RCs/pre-releases
    // all as "snapshot", so exclude those by title.
    const isBig = (n: NewsItem) => {
      const t = (n.tag || "").toLowerCase();
      return (
        n.source === "minecraft" &&
        t === "release" &&
        !/(snapshot|pre[-\s]?release|candidate|\brc\b)/i.test(n.title)
      );
    };
    const dateVal = (n: NewsItem) => Date.parse(n.date || "") || 0;
    // Full releases first (→ top-left lead slot), then newest first by date.
    const sorted = [...news].sort((a, b) => {
      const r = (isBig(b) ? 1 : 0) - (isBig(a) ? 1 : 0);
      return r !== 0 ? r : dateVal(b) - dateVal(a);
    });
    return (
      <div className="view news">
        <header className="news-masthead">
          <div className="news-rule" />
          <h1 className="news-title">The Celaris Times</h1>
          <div className="news-rule news-rule--thick" />
          <div className="news-sub">Minecraft &amp; Celaris Client · {today}</div>
        </header>

        {newsLoading ? (
          <Card>
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>Lädt Ausgabe…</div>
          </Card>
        ) : news.length === 0 ? (
          <Card>
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
              Keine Meldungen verfügbar (Quellen nicht erreichbar).
            </div>
          </Card>
        ) : (
          <div className="news-grid">
            {[0, 1].map((col) => (
              <div className="news-col" key={col}>
                {sorted
                  .map((n, i) => [n, i] as const)
                  .filter(([, i]) => i % 2 === col)
                  .map(([n, i]) => (
                    <article
                      key={i}
                      className={`news-item news-item--${n.source} ${i === 0 ? "news-item--lead" : ""}`}
                      onClick={() => setOpenArticle(n)}
                      style={{
                        ...(isBig(n)
                          ? { border: "1px solid var(--accent, #9d5cff)", boxShadow: "0 0 0 1px rgba(157,92,255,0.35), 0 6px 24px rgba(157,92,255,0.18)" }
                          : {}),
                        ...(n.image
                          ? {
                              backgroundImage: `linear-gradient(180deg, rgba(10,13,20,0.82), rgba(10,13,20,0.95)), url(${n.image})`,
                              backgroundSize: "cover",
                              backgroundPosition: "center",
                            }
                          : {}),
                      }}
                    >
                      {n.image && i === 0 && <img className="news-img" src={n.image} alt="" />}
                      {isBig(n) && (
                        <div style={{ display: "inline-block", background: "var(--accent, #9d5cff)", color: "#fff", fontWeight: 800, fontSize: 11, letterSpacing: 1, padding: "2px 8px", borderRadius: 5, marginBottom: 6 }}>
                          ★ NEUES RELEASE
                        </div>
                      )}
                      <div className="news-kicker">{n.source === "celaris" ? "Celaris" : "Minecraft"} · {n.tag}</div>
                      <h2 className="news-headline">{n.title}</h2>
                      {n.date && <div className="news-date">{n.date}</div>}
                      <p className="news-body">{n.body}</p>
                      <span className="news-more">Weiterlesen →</span>
                    </article>
                  ))}
              </div>
            ))}
          </div>
        )}

        {openArticle && (
          <div className="modal-overlay" onClick={() => setOpenArticle(null)}>
            <div className="modal" onClick={(e) => e.stopPropagation()}>
              <button className="modal-close" onClick={() => setOpenArticle(null)} aria-label="Schließen">✕</button>
              {openArticle.image && <img className="modal-img" src={openArticle.image} alt="" />}
              <div className="news-kicker">
                {openArticle.source === "celaris" ? "Celaris" : "Minecraft"} · {openArticle.tag}
              </div>
              <h2 className="modal-headline">{openArticle.title}</h2>
              {openArticle.date && <div className="news-date">{openArticle.date}</div>}
              <p className="modal-body">{openArticle.full || openArticle.body}</p>
            </div>
          </div>
        )}
      </div>
    );
  }

  function renderCosmetics() {
    return (
      <div className="view coming-soon">
        <div className="coming-soon-box">
          <h1 className="coming-soon-title">COMING SOON</h1>
        </div>
      </div>
    );
  }

  function renderPackMarket() {
    const isShader = packKind === "shader";
    const title = isShader ? "Shader" : "Texturpakete";
    const sub = isShader ? "Modrinth · Iris/OptiFine" : "Modrinth · Ressourcenpakete";
    const installedIds2 = new Set(
      packInstalled.flatMap((m) => [m.project_id, m.id].filter(Boolean) as string[]).map((s) => s.toLowerCase())
    );
    if (isShader && !hasShaderLoader) {
      return (
        <div className="view">
          <div className="view-head">
            <div>
              <h2 className="view-title">{title}</h2>
              <div className="view-sub">{sub} · {modProf.name}</div>
            </div>
            <label className="mod-profile-pick">
              <span>Profil</span>
              <select value={active} onChange={(e) => setActive(Number(e.target.value))}>
                {profiles.map((p, i) => (<option key={i} value={i}>{p.name}</option>))}
              </select>
            </label>
          </div>
          <Card>
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
              Dieses Profil hat keinen Shader-Loader. Installiere <b>Iris</b> (oder OptiFine) über die <b>Mods</b>-Seite, dann erscheinen hier die Shader.
            </div>
          </Card>
        </div>
      );
    }
    return (
      <div className="view">
        <div className="view-head">
          <div>
            <h2 className="view-title">{title}</h2>
            <div className="view-sub">{sub} · {modProf.minecraft_version}</div>
          </div>
          <div style={{ display: "flex", gap: "var(--s3)", alignItems: "center" }}>
            <label className="mod-profile-pick">
              <span>Profil</span>
              <select value={active} onChange={(e) => setActive(Number(e.target.value))}>
                {profiles.map((p, i) => (<option key={i} value={i}>{p.name}</option>))}
              </select>
            </label>
            <div className="tabs">
              <button className={`tab ${packTab === "browse" ? "on" : ""}`} onClick={() => setPackTab("browse")}>Marketplace</button>
              <button className={`tab ${packTab === "installed" ? "on" : ""}`} onClick={() => setPackTab("installed")}>
                Installiert{packInstalled.length > 0 ? ` (${packInstalled.length})` : ""}
              </button>
            </div>
          </div>
        </div>

        {packTab === "browse" ? (
          <>
            <form className="search" onSubmit={(e) => { e.preventDefault(); searchPacks(); }}>
              <input
                className="input"
                placeholder={isShader ? "Shader suchen (z. B. Complementary, BSL)…" : "Texturpakete suchen (z. B. Faithful)…"}
                value={packQuery}
                onChange={(e) => setPackQuery(e.target.value)}
              />
              <select className="input" style={{ flex: "0 0 auto", width: "auto", minWidth: "9rem" }} value={packSort}
                onChange={(e) => setPackSort(e.target.value)}>
                <option value="relevance">Relevanz</option>
                <option value="newest">Neueste</option>
                <option value="updated">Aktualisiert</option>
                <option value="downloads">Downloads</option>
              </select>
              <PrimaryButton type="submit" disabled={packSearching}>{packSearching ? "Sucht…" : "Suchen"}</PrimaryButton>
            </form>
            {packResults.length === 0 ? (
              <Card>
                <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
                  {packSearching ? "Suche läuft…" : "Suche und installiere mit einem Klick — pro Profil."}
                </div>
              </Card>
            ) : (
              <div className="grid grid--cards stagger">
                {packResults.map((m) => {
                  const isInst = installedIds2.has(m.project_id.toLowerCase()) || installedIds2.has(m.slug.toLowerCase());
                  return (
                    <Card key={m.project_id} className="modcard">
                      <div className="modcard-head">
                        {m.icon_url ? <img className="modcard-icon" src={m.icon_url} alt="" /> : <div className="modcard-icon modcard-icon--ph">{m.title.charAt(0)}</div>}
                        <div className="modcard-titles">
                          <div className="modcard-title">{m.title}</div>
                          <div className="modcard-author">von {m.author} · {formatDownloads(m.downloads)} ⬇</div>
                        </div>
                      </div>
                      <div className="modcard-desc">{m.description}</div>
                      <div className="modcard-foot">
                        {isInst ? (
                          <StatusBadge tone="success" dot>Installiert</StatusBadge>
                        ) : (
                          <PrimaryButton disabled={packInstalling === m.project_id} onClick={() => installPack(m)}>
                            {packInstalling === m.project_id ? "Installiert…" : "Installieren"}
                          </PrimaryButton>
                        )}
                      </div>
                    </Card>
                  );
                })}
              </div>
            )}
          </>
        ) : packInstalled.length === 0 ? (
          <Card>
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
              Noch nichts installiert.
            </div>
          </Card>
        ) : (
          <div className="grid stagger" style={{ gap: "var(--s3)" }}>
            {packInstalled.map((m) => {
              const name = m.title || m.id;
              return (
                <div key={m.filename} className="mod-row">
                  {m.icon_url ? (
                    <img className="mod-icon" src={m.icon_url} alt="" />
                  ) : (
                    <div className="mod-icon">{name.charAt(0).toUpperCase()}</div>
                  )}
                  <div className="mod-info">
                    <div className="mod-name">{name}</div>
                    <div className="mod-meta">{m.filename}</div>
                  </div>
                  <Button className="btn--ghost" onClick={() => removePack(m.filename)}>Entfernen</Button>
                </div>
              );
            })}
          </div>
        )}
      </div>
    );
  }

  function renderLogs() {
    return (
      <div className="view">
        <div className="view-head">
          <div>
            <h2 className="view-title">Log</h2>
            <div className="view-sub">Ausgabe der aktiven Instanz · {profile.name}</div>
          </div>
          <div className="row" style={{ gap: "var(--s2)" }}>
            <Button onClick={() => navigator.clipboard.writeText(logs.join("\n")).catch(() => {})}>Kopieren</Button>
            <Button onClick={() => setLogs([])}>Leeren</Button>
          </div>
        </div>
        <pre className="logbox logbox--page">
          {logs.length === 0 ? "Keine Ausgabe — starte eine Instanz, um Logs zu sehen." : logs.join("\n")}
          <div ref={logEndRef} />
        </pre>
      </div>
    );
  }

  function renderCredits() {
    const credits = [
      { role: "Founder & Lead Developer", name: "Rimuru", note: "Architektur, Launcher, Client & Server.", lead: true },
      { role: "Developer", name: "Dein Name?", note: "Werde Teil des Teams." },
      { role: "Designer", name: "Dein Name?", note: "UI, Cosmetics & Branding." },
      { role: "Community", name: "Du", note: "Feedback, Tests & Ideen." },
    ];
    return (
      <div className="view credits">
        <div className="credits-hero">
          <Rinnegan size={300} />
          <h1 className="credits-title">CELARIS</h1>
          <div className="credits-tag">Crafted with the Rinnegan · {new Date().getFullYear()}</div>
        </div>
        <div className="credits-grid">
          {credits.map((c, i) => (
            <div key={i} className={`credit-card ${c.lead ? "lead" : ""}`}>
              <div className="credit-role">{c.role}</div>
              <div className="credit-name">{c.name}</div>
              <div className="credit-note">{c.note}</div>
            </div>
          ))}
        </div>
        <div className="credits-cta">
          <PrimaryButton onClick={() => openUrl(DISCORD_INVITE)}>Discord beitreten</PrimaryButton>
          <Button onClick={() => openUrl(DISCORD_INVITE)}>Im Team bewerben</Button>
        </div>
        <div className="credits-cta-note">
          Bewerbungen, Feedback & Updates laufen über unseren Discord.
        </div>
      </div>
    );
  }

  function renderHosting() {
    return (
      <div className="view">
        <div className="view-head">
          <div>
            <h2 className="view-title">Hosting</h2>
            <div className="view-sub">Eigene Minecraft-Server – bald über einen Hosting-Partner.</div>
          </div>
          <StatusBadge tone="warning">Bald verfügbar</StatusBadge>
        </div>
        <Card>
          <div style={{ textAlign: "center", padding: "var(--s6) var(--s5)" }}>
            <div style={{ fontSize: "2.4rem" }}>🏗️</div>
            <h3 style={{ fontFamily: "var(--font-display)", margin: "var(--s3) 0 var(--s2)" }}>Server-Hosting kommt bald</h3>
            <p style={{ color: "var(--text-dim)", maxWidth: 480, margin: "0 auto" }}>
              Hier mietest du künftig direkt aus Celaris einen Minecraft-Server bei unserem Hosting-Partner – inklusive Panel und Datei-Verwaltung.
            </p>
            <div style={{ display: "flex", gap: "var(--s2)", justifyContent: "center", marginTop: "var(--s5)" }}>
              <PrimaryButton disabled>Server mieten</PrimaryButton>
              <Button disabled>Pterodactyl-Panel</Button>
              <Button disabled>Datei-Manager</Button>
            </div>
          </div>
        </Card>
        <div className="grid grid--cards" style={{ marginTop: "var(--s5)" }}>
          {[
            { t: "1-Klick-Server", d: "Version & Modpack aus Celaris direkt deployen." },
            { t: "Panel", d: "Start/Stop, Konsole, Backups – via Pterodactyl." },
            { t: "Datei-Manager", d: "Configs, Welten und Mods direkt verwalten." },
          ].map((f, i) => (
            <Card key={i} style={{ opacity: 0.6 }}>
              <div style={{ fontWeight: 600, marginBottom: 6 }}>{f.t}</div>
              <div style={{ color: "var(--text-dim)", fontSize: "0.88rem" }}>{f.d}</div>
            </Card>
          ))}
        </div>
      </div>
    );
  }

  function renderServers() {
    return (
      <div className="view">
        <div className="view-head">
          <div>
            <h2 className="view-title">Server</h2>
            <div className="view-sub">Partner-Server immer oben · eigene Server · ins Spiel übernehmen.</div>
          </div>
          <PrimaryButton onClick={syncServers}>🎮 Ins Spiel übernehmen</PrimaryButton>
        </div>

        {serverMsg && (
          <div className="banner banner--success" style={{ marginBottom: "var(--s4)" }}>
            <span className="banner-msg">{serverMsg}</span>
          </div>
        )}

        {partners.length > 0 && (
          <>
            <div className="nav-label" style={{ padding: "0 0 var(--s2)" }}>Partner</div>
            <div className="grid" style={{ gap: "var(--s2)", marginBottom: "var(--s5)" }}>
              {partners.map((s, i) => (
                <div key={i} style={{ marginBottom: "var(--s2)" }}>
                  {s.banner && (
                    <img
                      src={s.banner}
                      alt=""
                      style={{ width: "100%", maxHeight: 90, objectFit: "cover", borderRadius: "10px 10px 0 0", display: "block" }}
                    />
                  )}
                  <div className="server-row server-row--partner" style={s.banner ? { borderRadius: "0 0 10px 10px" } : undefined}>
                    {serverBanner(s) ? (
                      <img className="server-badge" src={serverBanner(s)!} alt="" style={{ objectFit: "cover" }} />
                    ) : (
                      <div className="server-badge">★</div>
                    )}
                    <div className="server-info">
                      <div className="server-name">{s.name}</div>
                      <div className="server-addr">
                        {s.address}{s.description ? ` · ${s.description}` : ""}
                        {serverStatus[s.address]?.online ? ` · 🟢 ${serverStatus[s.address].players}/${serverStatus[s.address].max}` : ""}
                      </div>
                    </div>
                    <StatusBadge tone="accent" dot>Partner</StatusBadge>
                    <PrimaryButton onClick={() => joinServer(s.address)}>▶ Beitreten</PrimaryButton>
                  </div>
                </div>
              ))}
            </div>
          </>
        )}

        <div className="nav-label" style={{ padding: "0 0 var(--s2)" }}>Eigene Server</div>
        <form
          className="search"
          onSubmit={(e) => {
            e.preventDefault();
            addServer();
          }}
        >
          <input className="input" placeholder="Name" value={srvName} onChange={(e) => setSrvName(e.target.value)} style={{ flex: "0 0 30%" }} />
          <input className="input" placeholder="Adresse (z. B. mc.example.net)" value={srvAddr} onChange={(e) => setSrvAddr(e.target.value)} />
          <PrimaryButton type="submit">+ Hinzufügen</PrimaryButton>
        </form>

        {userServers.length === 0 ? (
          <Card>
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
              Noch keine eigenen Server.
            </div>
          </Card>
        ) : (
          <div className="grid" style={{ gap: "var(--s2)" }}>
            {userServers.map((s, i) => (
              <div key={i} className="server-row">
                {serverBanner(s) ? (
                  <img className="server-badge server-badge--user" src={serverBanner(s)!} alt="" style={{ objectFit: "cover" }} />
                ) : (
                  <div className="server-badge server-badge--user">{s.name.charAt(0).toUpperCase()}</div>
                )}
                <div className="server-info">
                  <div className="server-name">{s.name}</div>
                  <div className="server-addr">
                    {s.address}
                    {serverStatus[s.address]?.online ? ` · 🟢 ${serverStatus[s.address].players}/${serverStatus[s.address].max} online` : serverStatus[s.address] ? " · 🔴 offline" : ""}
                  </div>
                </div>
                <PrimaryButton onClick={() => joinServer(s.address)}>▶ Beitreten</PrimaryButton>
                <Button className="btn--ghost" onClick={() => removeServer(i)}>Entfernen</Button>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  function renderWardrobe() {
    return (
      <div className="view">
        <div className="view-head">
          <div>
            <h2 className="view-title">Garderobe</h2>
            <div className="view-sub">Skins per Name/UUID holen, importieren und speichern.</div>
          </div>
          <Button onClick={importSkin}>⬇ PNG importieren</Button>
        </div>

        <form
          className="search"
          onSubmit={(e) => {
            e.preventDefault();
            grabSkin();
          }}
        >
          <input
            className="input"
            placeholder="Spielername oder UUID (Skin-Grabber)…"
            value={grabQuery}
            onChange={(e) => setGrabQuery(e.target.value)}
          />
          <PrimaryButton type="submit" disabled={grabbing}>{grabbing ? "Holt…" : "Holen"}</PrimaryButton>
        </form>

        {skinMsg && (
          <div className="banner banner--error" style={{ marginBottom: "var(--s4)" }}>
            <span className="banner-msg">{skinMsg}</span>
          </div>
        )}

        {skins.length === 0 ? (
          <Card>
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "var(--s5)" }}>
              Noch keine Skins — hol dir einen per Name/UUID oder importiere ein PNG.
            </div>
          </Card>
        ) : (
          <div className="grid grid--cards stagger">
            {skins.map((s) => {
              const equipped = s.id === equippedSkinId;
              return (
              <Card key={s.id} className={`skincard ${equipped ? "skincard--equipped" : ""}`}>
                {equipped && <div className="skincard-tag">Ausgerüstet ✓</div>}
                <div className="skincard-head">
                  <SkinFace png={s.png_base64} size={72} />
                  <div className="skincard-meta">
                    <div className="skincard-name">{s.name}</div>
                    {s.uuid && <div className="skincard-uuid">{s.uuid.slice(0, 8)}…</div>}
                  </div>
                </div>
                <div className="skincard-foot">
                  <Button className="btn--ghost" onClick={() => removeSkin(s.id)}>Entfernen</Button>
                  <PrimaryButton onClick={() => applySkin(s.id)}>{equipped ? "Erneut anwenden" : "Anwenden"}</PrimaryButton>
                </div>
              </Card>
              );
            })}
          </div>
        )}
      </div>
    );
  }

  function renderSettings() {
    return (
      <div className="view" style={{ maxWidth: 620 }}>
        <div className="view-head">
          <div>
            <h2 className="view-title">Spiel-Einstellungen</h2>
            <div className="view-sub">Eigene Einstellungen für Profil „{profile.name}" (Version, Java, RAM, Args).</div>
          </div>
          <Button onClick={() => setView("profiles")}>← Profile</Button>
        </div>

        <Card>
          <div className="field">
            <label>Profilname</label>
            <input className="input" value={profile.name} onChange={(e) => patch("name", e.target.value)} />
          </div>
          <div className="field">
            <label>Minecraft-Version</label>
            {visibleVersions.length > 0 ? (
              <>
                <select
                  value={profile.minecraft_version}
                  onChange={(e) => patch("minecraft_version", e.target.value)}
                >
                  {visibleVersions.some((v) => v.id === profile.minecraft_version) ? null : (
                    <option value={profile.minecraft_version}>{profile.minecraft_version} (aktuell)</option>
                  )}
                  {visibleVersions.map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.id}{v.kind !== "release" ? `  ·  ${v.kind}` : ""}
                    </option>
                  ))}
                </select>
                <label className="checkbox" style={{ marginTop: 6 }}>
                  <input type="checkbox" checked={showSnapshots} onChange={(e) => setShowSnapshots(e.target.checked)} />
                  Snapshots & Aprilscherz-Versionen anzeigen
                </label>
              </>
            ) : (
              <input className="input" value={profile.minecraft_version} onChange={(e) => patch("minecraft_version", e.target.value)} />
            )}
          </div>
          <div className="field">
            <label>Java</label>
            <div className="row">
              <input className="input" value={profile.java_path} onChange={(e) => patch("java_path", e.target.value)} />
              <Button onClick={detectJava}>Erkennen</Button>
            </div>
            {javaInstalls.length > 0 && (
              <select value={profile.java_path} onChange={(e) => patch("java_path", e.target.value)} style={{ marginTop: 8 }}>
                {javaInstalls.map((j) => (
                  <option key={j.path} value={j.path}>{j.version} — {j.path}</option>
                ))}
              </select>
            )}
          </div>
          <div className="field">
            <label>Arbeitsspeicher · {(profile.max_ram_mb / 1024).toFixed(1)} GB</label>
            <input type="range" min={1024} max={16384} step={512} value={profile.max_ram_mb} onChange={(e) => patch("max_ram_mb", Number(e.target.value))} />
          </div>
          <div className="field">
            <label>Spielverzeichnis (optional)</label>
            <input className="input" placeholder="leer = eigener Ordner pro Profil (parallele Instanzen)" value={profile.game_dir} onChange={(e) => patch("game_dir", e.target.value)} />
            <div className="hint">Leer lassen, damit jedes Profil ein eigenes Verzeichnis bekommt und mehrere Instanzen gleichzeitig laufen können.</div>
          </div>
          <div className="field">
            <label>Java-Argumente (optional)</label>
            <textarea
              className="input"
              rows={2}
              spellCheck={false}
              placeholder="-XX:+UseG1GC -XX:+ParallelRefProcEnabled"
              value={profile.jvm_args}
              onChange={(e) => patch("jvm_args", e.target.value)}
              style={{ fontFamily: "var(--mono, monospace)", resize: "vertical" }}
            />
            <div className="hint">Zusätzliche JVM-Flags, durch Leerzeichen/Zeilenumbruch getrennt. Werden nach -Xmx übergeben.</div>
          </div>
          <div className="field">
            <label>Umgebungsvariablen (optional)</label>
            <textarea
              className="input"
              rows={2}
              spellCheck={false}
              placeholder={"KEY=VALUE\n__GL_THREADED_OPTIMIZATIONS=1"}
              value={profile.env_vars}
              onChange={(e) => patch("env_vars", e.target.value)}
              style={{ fontFamily: "var(--mono, monospace)", resize: "vertical" }}
            />
            <div className="hint">Eine <code>KEY=VALUE</code>-Zuweisung pro Zeile. Zeilen mit <code>#</code> werden ignoriert.</div>
          </div>

          <label className="switch">
            <input type="checkbox" checked={profile.use_celaris_client} onChange={(e) => patch("use_celaris_client", e.target.checked)} />
            <span className="track" />
            <span className="switch-label">Celaris Client (Fabric) verwenden</span>
          </label>

          <label className="switch">
            <input
              type="checkbox"
              checked={profile.use_celaris_client || profile.use_fabric}
              disabled={profile.use_celaris_client}
              onChange={(e) => patch("use_fabric", e.target.checked)}
            />
            <span className="track" />
            <span className="switch-label">
              Fabric-Loader verwenden (für eigene Mods, ohne Celaris)
              {profile.use_celaris_client ? " — durch Celaris bereits aktiv" : ""}
            </span>
          </label>
          <div style={{ color: "var(--text-faint)", fontSize: "0.8rem", marginTop: "var(--s1)" }}>
            {profile.use_celaris_client
              ? "Profil startet mit Celaris-Client (inkl. Fabric)."
              : profile.use_fabric
                ? "Profil startet mit Fabric (ohne Celaris) — lege eigene Mods im Mods-Ordner ab."
                : "Profil startet als reines Vanilla Minecraft."}
          </div>

          <div style={{ display: "flex", gap: "var(--s2)", marginTop: "var(--s4)" }}>
            <PrimaryButton onClick={saveProfiles}>Speichern</PrimaryButton>
            <Button onClick={() => setView("play")}>Fertig</Button>
          </div>
        </Card>
      </div>
    );
  }

  async function toggleAutostart() {
    try {
      if (autostart) {
        await autostartDisable();
        setAutostart(false);
      } else {
        await autostartEnable();
        setAutostart(true);
      }
    } catch (e) {
      console.error("autostart toggle failed", e);
    }
  }

  function renderLauncher() {
    return (
      <div className="view" style={{ maxWidth: 640 }}>
        <div className="view-head">
          <div>
            <h2 className="view-title">Launcher-Einstellungen</h2>
            <div className="view-sub">Gelten für den ganzen Launcher (nicht pro Profil).</div>
          </div>
        </div>

        <Card>
          <div className="nav-label" style={{ padding: "0 0 var(--s3)" }}>Theme</div>
          <div className="theme-swatches">
            {THEMES.map((t) => (
              <button
                key={t.key}
                className={`theme-swatch ${theme === t.key ? "active" : ""}`}
                onClick={() => { setTheme(t.key); applyTheme(t.key); }}
                title={t.name}
              >
                <span className="theme-dot" style={{ background: `linear-gradient(135deg, ${t.a}, ${t.b})` }} />
                {t.name}
              </button>
            ))}
          </div>
          <div className="hint" style={{ marginTop: "var(--s3)" }}>Akzentfarbe des Launchers (mehr Themes folgen).</div>
        </Card>

        <div style={{ marginTop: "var(--s4)" }} />
        <Card>
          <label className="switch-row">
            <input type="checkbox" checked={autostart} onChange={toggleAutostart} />
            <span className="switch-label">Mit dem System starten (Autostart)</span>
          </label>
          <div className="hint">Celaris startet automatisch nach dem Hochfahren.</div>
        </Card>

        <div style={{ marginTop: "var(--s4)" }} />
        <Card>
          <div className="nav-label" style={{ padding: "0 0 var(--s3)" }}>Musik</div>
          <label className="switch-row">
            <input
              type="checkbox"
              checked={sound.musicEnabled}
              onChange={(e) => updateSound({ musicEnabled: e.target.checked })}
            />
            <span className="switch-label">Lofi-Hintergrundmusik</span>
          </label>
          <label style={{ display: "block", marginTop: "var(--s3)" }}>Lautstärke Musik · {Math.round(sound.musicVolume * 100)}%</label>
          <input
            type="range" min={0} max={1} step={0.05} value={sound.musicVolume}
            onChange={(e) => updateSound({ musicVolume: Number(e.target.value) })}
          />
          <label style={{ marginTop: "var(--s3)" }}>Ausgabegerät</label>
          <select
            value={sound.outputDeviceId}
            onChange={(e) => updateSound({ outputDeviceId: e.target.value })}
          >
            <option value="">System-Standard</option>
            {audioOutputs.map((d) => (
              <option key={d.deviceId} value={d.deviceId}>{d.label}</option>
            ))}
          </select>
        </Card>

        <div style={{ marginTop: "var(--s4)" }} />
        <Card>
          <div className="nav-label" style={{ padding: "0 0 var(--s3)" }}>Rechtliches</div>
          <div className="legal-links">
            <button className="legal-link" onClick={() => openUrl("https://celarisclient.de/nutzungsrichtlinien")}>
              <span>Nutzungsbedingungen</span><span className="legal-ext">↗</span>
            </button>
            <button className="legal-link" onClick={() => openUrl("https://celarisclient.de/datenschutz")}>
              <span>Datenschutzerklärung</span><span className="legal-ext">↗</span>
            </button>
            <button className="legal-link" onClick={() => openUrl("https://celarisclient.de/impressum")}>
              <span>Impressum</span><span className="legal-ext">↗</span>
            </button>
          </div>
        </Card>

        <div style={{ marginTop: "var(--s4)" }} />
        <Card>
          <div className="nav-label" style={{ padding: "0 0 var(--s3)" }}>Support &amp; Info</div>
          <div className="row" style={{ gap: "var(--s2)" }}>
            <Button onClick={() => openUrl(DISCORD_INVITE)}>Discord / Support</Button>
            <Button onClick={() => openUrl("https://discord.gg/62RTMCVjQ4")}>Bug melden</Button>
          </div>
          <div className="hint" style={{ marginTop: "var(--s3)" }}>Celaris v0.1 · Fabric · Minecraft 1.21.11</div>
        </Card>
      </div>
    );
  }
}
