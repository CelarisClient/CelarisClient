# Celaris — Auto-Update veröffentlichen

Beide Ebenen liefern Updates über **`https://api.celarisclient.de/content/…`**
(der Rust-Server liefert den `content/`-Ordner statisch aus). Du legst die Dateien
einfach in den `content`-Ordner des Rust-Servers (Infynix File-Manager).

---

## 1. In-Game-Mod aktualisieren (sofort bei allen, kein Launcher-Update)

Der Launcher lädt bei **jedem Spielstart** die neuste Mod-Jar.

1. Neue Client-Jar bauen:
   ```bash
   cd celaris-client && ./gradlew build
   # Ergebnis: build/libs/Celaris-<version>.jar
   ```
2. Auf den Rust-Server in `content/celaris/` hochladen:
   - `Celaris-<version>.jar`
   - `version.json`:
     ```json
     { "version": "0.2.0", "file": "Celaris-0.2.0.jar" }
     ```
3. Fertig. Beim nächsten Spielstart ziehen alle Clients die neue Jar automatisch
   (Vergleich über `version` — alte Jars werden lokal entfernt).

> Erreichbar unter `https://api.celarisclient.de/content/celaris/version.json`.

---

## 2. Launcher selbst aktualisieren (signiert)

Der Launcher prüft beim Start `…/content/launcher/latest.json` und installiert
ein neueres, **signiertes** Build automatisch (dann Neustart).

**Einmalig:** Der Signing-Key liegt in `celaris-launcher/.tauri/celaris-updater.key`
(privat, gitignored — **niemals teilen/verlieren**). Der Public Key steht schon in
`tauri.conf.json`.

**Pro Release:**
1. Version hochzählen in `src-tauri/tauri.conf.json` (`"version"`) **und**
   `src-tauri/Cargo.toml` (`version`).
2. Signiert bauen:
   ```bash
   cd celaris-launcher
   export TAURI_SIGNING_PRIVATE_KEY="$(cat .tauri/celaris-updater.key)"
   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
   npm run tauri build
   ```
3. Im Bundle-Ordner (`src-tauri/target/release/bundle/…`) entstehen pro Plattform
   das Installer-Paket **und** eine `.sig`-Datei, plus eine `latest.json`.
4. Auf den Rust-Server in `content/launcher/` hochladen:
   - die Installer (z. B. `.AppImage`, `.deb`, `.msi`/`.exe`, `.dmg`)
   - die `latest.json`
5. In der `latest.json` müssen die `url`-Felder auf die hochgeladenen Dateien zeigen,
   z. B. `https://api.celarisclient.de/content/launcher/Celaris-Launcher_0.2.0_amd64.AppImage`.
   (Die `signature` füllt Tauri automatisch ein.)

> Beim nächsten Start sehen Nutzer „Update … wird geladen…" und der Launcher
> aktualisiert sich selbst.

---

## Server-Ordnerstruktur (im `content/` des Rust-Servers)
```
content/
├─ celaris/
│  ├─ version.json
│  └─ Celaris-<version>.jar
├─ launcher/
│  ├─ latest.json
│  └─ <installer-dateien>
├─ partners.json     (optional)
├─ modpacks.json     (optional)
└─ news.json         (optional)
```
