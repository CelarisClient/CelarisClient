#!/usr/bin/env bash
#
# Builds signed apt + dnf repositories for the Celaris Launcher so Linux testers
# can install AND update it from their software centre (GNOME Software / Discover)
# or `apt`/`dnf`.
#
# Output:  ./linux-repo/apt/   and  ./linux-repo/dnf/
# Upload the whole ./linux-repo/ to the server so it is reachable at:
#   https://api.celarisclient.de/content/apt/
#   https://api.celarisclient.de/content/dnf/
# (the Rust server serves the `content/` folder statically — put it in
#  content/apt and content/dnf).
#
# Requirements: dpkg-scanpackages, gpg, gzip (apt side); createrepo_c (dnf side).
#   Fedora/Nobara:  sudo dnf install dpkg createrepo_c gnupg2
#
# A signing key is created on first run and stored in ~/.config/com.celaris.launcher.
# KEEP THE PRIVATE KEY SAFE — testers trust the matching public key.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="$ROOT/src-tauri/target/release/bundle"
OUT="$ROOT/linux-repo"
KEYDIR="$HOME/.config/com.celaris.launcher"
KEYNAME="Celaris Packages"
KEYEMAIL="packages@celarisclient.de"
GNUPGHOME_DIR="$KEYDIR/repo-gpg"
APT_ORIGIN="Celaris"
CODENAME="stable"

mkdir -p "$OUT/apt/pool" "$OUT/dnf" "$GNUPGHOME_DIR"
chmod 700 "$GNUPGHOME_DIR"
export GNUPGHOME="$GNUPGHOME_DIR"

# --- 1. Signing key --------------------------------------------------------
# In CI, import the key from the REPO_GPG_PRIVATE_KEY env (ascii-armored secret)
# so packages are signed with the SAME key testers already trust.
if [ -n "${REPO_GPG_PRIVATE_KEY:-}" ]; then
  printf '%s' "$REPO_GPG_PRIVATE_KEY" | gpg --batch --import >/dev/null 2>&1 || true
fi
# Otherwise create one once (local use).
if ! gpg --list-secret-keys "$KEYEMAIL" >/dev/null 2>&1; then
  echo "Creating repo signing key (one time)…"
  cat > "$GNUPGHOME_DIR/keygen" <<EOF
%no-protection
Key-Type: eddsa
Key-Curve: ed25519
Key-Usage: sign
Name-Real: $KEYNAME
Name-Email: $KEYEMAIL
Expire-Date: 0
%commit
EOF
  gpg --batch --gen-key "$GNUPGHOME_DIR/keygen"
  rm -f "$GNUPGHOME_DIR/keygen"
fi
KEYID="$(gpg --list-secret-keys --with-colons "$KEYEMAIL" | awk -F: '/^sec:/{print $5; exit}')"
echo "Signing key: $KEYID"

# Public keys testers import.
gpg --armor --export "$KEYEMAIL" > "$OUT/celaris.asc"
cp "$OUT/celaris.asc" "$OUT/dnf/RPM-GPG-KEY-celaris"

# --- 2. apt repo -------------------------------------------------------------
echo "Building apt repo…"
# Copy with space-free filenames (apt dislikes spaces in pool paths).
while IFS= read -r -d '' f; do
  cp -f "$f" "$OUT/apt/pool/$(basename "$f" | tr ' ' '-')"
done < <(find "$BUNDLE/deb" -name '*.deb' -print0 2>/dev/null)
( cd "$OUT/apt"
  dpkg-scanpackages --multiversion pool > Packages
  gzip -9c Packages > Packages.gz
  # Release file with checksums.
  cat > Release <<EOF
Origin: $APT_ORIGIN
Label: $APT_ORIGIN
Suite: $CODENAME
Codename: $CODENAME
Architectures: amd64
Components: main
Date: $(date -Ru)
EOF
  {
    echo "MD5Sum:"
    for f in Packages Packages.gz; do echo " $(md5sum "$f" | cut -d' ' -f1) $(stat -c%s "$f") $f"; done
    echo "SHA256:"
    for f in Packages Packages.gz; do echo " $(sha256sum "$f" | cut -d' ' -f1) $(stat -c%s "$f") $f"; done
  } >> Release
  gpg --batch --yes --default-key "$KEYEMAIL" --clearsign -o InRelease Release
  gpg --batch --yes --default-key "$KEYEMAIL" -abs -o Release.gpg Release
)

# --- 3. dnf repo -------------------------------------------------------------
echo "Building dnf repo…"
while IFS= read -r -d '' f; do
  cp -f "$f" "$OUT/dnf/$(basename "$f" | tr ' ' '-')"
done < <(find "$BUNDLE/rpm" -name '*.rpm' -print0 2>/dev/null)
if command -v createrepo_c >/dev/null; then
  createrepo_c --update "$OUT/dnf"
  gpg --batch --yes --default-key "$KEYEMAIL" --detach-sign --armor "$OUT/dnf/repodata/repomd.xml"
else
  echo "WARNING: createrepo_c not found — dnf metadata NOT generated."
  echo "         Install it (sudo dnf install createrepo_c) and re-run."
fi

# --- 4. tester drop-in files -------------------------------------------------
cat > "$OUT/celaris.list" <<EOF
# /etc/apt/sources.list.d/celaris.list
deb [signed-by=/usr/share/keyrings/celaris.gpg] https://api.celarisclient.de/content/apt $CODENAME main
EOF

cat > "$OUT/celaris.repo" <<EOF
# /etc/yum.repos.d/celaris.repo
[celaris]
name=Celaris
baseurl=https://api.celarisclient.de/content/dnf
enabled=1
gpgcheck=1
gpgkey=https://api.celarisclient.de/content/dnf/RPM-GPG-KEY-celaris
EOF

echo
echo "Done. Upload '$OUT' to the server's content/ folder (content/apt, content/dnf)."
echo "Public key: $OUT/celaris.asc"
