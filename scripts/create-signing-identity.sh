#!/usr/bin/env bash
# Create a self-signed code-signing identity so macOS permissions survive rebuilds.
#
# Why this exists: macOS ties an Accessibility or Microphone grant to the app's code
# signature. Tauri signs ad-hoc ("-"), which produces a new signature every build, so
# each rebuild looks like a different app and the permission you granted stops
# applying — while often still showing as switched on in System Settings.
#
# Signing with a stable identity fixes that. This is for local development; it is not
# a Developer ID and does nothing for Gatekeeper on anyone else's machine.
#
# Run once. Expect two prompts from macOS: one to add the certificate to your login
# keychain, one the first time codesign uses the key (choose Always Allow).
set -euo pipefail

IDENTITY="${VOXABLE_SIGN_IDENTITY:-Voxable Dev}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -q "$IDENTITY"; then
  echo "Identity \"$IDENTITY\" already exists — nothing to do."
  exit 0
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

echo "Creating a self-signed code-signing certificate: $IDENTITY"
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
  -subj "/CN=$IDENTITY" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" 2>/dev/null

openssl pkcs12 -export -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
  -out "$WORK/identity.p12" -passout pass:

# -T /usr/bin/codesign lets codesign use the key without a prompt every single build.
security import "$WORK/identity.p12" -k "$KEYCHAIN" -P "" -T /usr/bin/codesign

# codesign refuses a certificate it cannot build a trust chain for, so trust it in the
# user domain. This is the step that shows a password prompt.
echo "macOS will now ask for your password to trust the certificate."
security add-trusted-cert -r trustRoot -k "$KEYCHAIN" "$WORK/cert.pem"

echo
security find-identity -v -p codesigning | grep "$IDENTITY" || {
  echo "The identity was not created. Sign in to the login keychain and try again." >&2
  exit 1
}

cat <<EOF

Done. Now:

  1. ./scripts/build-mac.sh          # signs with "$IDENTITY" automatically
  2. tccutil reset Accessibility com.voxable.app
     tccutil reset Microphone com.voxable.app
  3. Launch Voxable and grant both permissions once.

Step 2 clears the stale grants that were attached to the old ad-hoc signatures. From
here on, rebuilds keep the permissions.
EOF
