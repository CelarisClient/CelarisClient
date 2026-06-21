# Celaris Launcher

> Demo build. This repository contains the launcher only. The in-game client
> modification is delivered automatically by the backend at runtime and is not
> part of this repository.

---

## English

The Celaris Launcher is a desktop application for launching Minecraft with the
Celaris experience. It manages game profiles, signs you in, downloads everything
that is required and starts the game.

### Features

- Profile management with adjustable memory and game version
- Microsoft sign-in and offline mode, with automatic session refresh
- Automatic Java runtime installation when a suitable one is missing
- Marketplace for mods, resource packs and shaders (powered by Modrinth)
  with search, sorting and one-click installation
- Multi-version play: the client runs on a single Minecraft version but can
  connect to servers across a wide range of versions through ViaFabricPlus
- Friends and presence
- The client modification is fetched and kept up to date automatically from the
  backend, so players always run the latest version without reinstalling

### Building from source

Requirements: a recent Node.js, Rust toolchain and the Tauri prerequisites for
your platform.

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # produce a release build
```

### Antivirus and Windows Defender notice

The launcher may be flagged by Windows Defender, SmartScreen or other antivirus
software, and the download may be blocked. This is a false positive and is
expected for this type of application. The reasons are:

- The executable is not code-signed with a paid certificate. Unsigned
  applications trigger SmartScreen warnings ("unknown publisher").
- The launcher downloads a Java runtime, mods and the client at runtime, and
  then starts another process (the game). Heuristic scanners often treat
  "download and execute" behaviour as suspicious even when it is legitimate.
- New or rarely downloaded files have no reputation yet, which raises the
  warning level further.

How to proceed:

- On the SmartScreen dialog choose "More info" and then "Run anyway".
- If your antivirus quarantines the file, restore it and add an exclusion for
  the launcher's folder.
- If you prefer, build the launcher yourself from this source.

This project is provided as a demo, without warranty.

---

## Deutsch

Der Celaris Launcher ist eine Desktop-Anwendung, um Minecraft mit dem Celaris-
Erlebnis zu starten. Er verwaltet Spielprofile, meldet dich an, lädt alles
Benötigte herunter und startet das Spiel.

### Funktionen

- Profilverwaltung mit einstellbarem Arbeitsspeicher und Spielversion
- Microsoft-Anmeldung und Offline-Modus mit automatischer Sitzungserneuerung
- Automatische Installation einer passenden Java-Laufzeit, falls keine vorhanden
- Marktplatz für Mods, Resource Packs und Shader (über Modrinth) mit Suche,
  Sortierung und Installation per Klick
- Multi-Version: Der Client läuft auf einer Minecraft-Version, kann sich aber
  dank ViaFabricPlus mit Servern vieler verschiedener Versionen verbinden
- Freunde und Online-Status
- Die Client-Modifikation wird automatisch vom Backend geladen und aktuell
  gehalten, sodass Spieler immer die neueste Version nutzen ohne neu zu
  installieren

### Aus dem Quellcode bauen

Voraussetzungen: aktuelles Node.js, Rust-Toolchain und die Tauri-
Voraussetzungen für dein Betriebssystem.

```bash
npm install
npm run tauri dev      # Entwicklungsmodus
npm run tauri build    # Release-Build erstellen
```

### Hinweis zu Antivirus und Windows Defender

Der Launcher kann von Windows Defender, SmartScreen oder anderer Antiviren-
Software markiert und der Download blockiert werden. Das ist ein Fehlalarm und
bei dieser Art von Anwendung normal. Die Gründe:

- Die Anwendung ist nicht mit einem kostenpflichtigen Zertifikat signiert.
  Unsignierte Programme lösen SmartScreen-Warnungen aus ("unbekannter
  Herausgeber").
- Der Launcher lädt zur Laufzeit eine Java-Laufzeit, Mods und den Client herunter
  und startet danach einen weiteren Prozess (das Spiel). Heuristische Scanner
  bewerten "Herunterladen und Ausführen" oft als verdächtig, auch wenn es
  legitim ist.
- Neue oder selten heruntergeladene Dateien haben noch keine Reputation, was die
  Warnstufe zusätzlich erhöht.

So gehst du vor:

- Im SmartScreen-Dialog auf "Weitere Informationen" und dann "Trotzdem
  ausführen" klicken.
- Falls dein Antivirus die Datei in Quarantäne verschiebt, stelle sie wieder her
  und füge eine Ausnahme für den Ordner des Launchers hinzu.
- Alternativ kannst du den Launcher selbst aus diesem Quellcode bauen.

Dieses Projekt wird als Demo bereitgestellt, ohne Gewähr.
