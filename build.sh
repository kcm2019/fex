#!/bin/bash
# Manual build for HeckYeah TV (framework APIs only, minSdk 25).
# This project builds with Android SDK command-line tools directly:
# aapt2 -> javac -> d8 -> zipalign -> apksigner. No Gradle required.
#
# Environment:
#   JAVA_HOME       JDK 17 home. If unset, a local JDK 17 install is
#                   used when one is found in a known location.
#   ANDROID_HOME    Android SDK root (ANDROID_SDK_ROOT also works).
#   KEYSTORE_PASS   Password for keystore.jks.
#   KEY_PASS        Password for the signing key; defaults to
#                   KEYSTORE_PASS when unset.
# If keystore.jks is missing, the script generates a local
# development keystore. When no passwords are provided it uses the
# development default "heckyeah-dev" and prints a warning; do not
# use that default for a signing key you care about.
set -euo pipefail

PROJ="$(cd "$(dirname "$0")" && pwd)"

# --- Java -----------------------------------------------------------------
if [[ -z "${JAVA_HOME:-}" ]]; then
    for cand in /home/hatch/toolchains/jdk-17.0.20.1+1 \
                "$HOME/toolchains/jdk-17.0.20.1+1"; do
        if [[ -d "$cand" ]]; then
            JAVA_HOME="$cand"
            break
        fi
    done
fi
if [[ -n "${JAVA_HOME:-}" ]]; then
    export JAVA_HOME
    export PATH="$JAVA_HOME/bin:$PATH"
fi
for tool in javac keytool; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "ERROR: $tool not found. Install JDK 17 or set JAVA_HOME." >&2
        exit 1
    fi
done

# --- Android SDK ------------------------------------------------------------
SDK=""
if [[ -n "${ANDROID_HOME:-}" ]]; then
    SDK="$ANDROID_HOME"
elif [[ -n "${ANDROID_SDK_ROOT:-}" ]]; then
    SDK="$ANDROID_SDK_ROOT"
elif [[ -d /home/hatch/android-sdk ]]; then
    SDK=/home/hatch/android-sdk
elif [[ -d "$HOME/android-sdk" ]]; then
    SDK="$HOME/android-sdk"
fi
if [[ -z "$SDK" || ! -d "$SDK" ]]; then
    echo "ERROR: Android SDK not found. Set ANDROID_HOME or ANDROID_SDK_ROOT." >&2
    exit 1
fi
BT="$SDK/build-tools/34.0.0"
AJAR="$SDK/platforms/android-34/android.jar"
for tool in aapt2 d8 zipalign apksigner; do
    if [[ ! -x "$BT/$tool" ]]; then
        echo "ERROR: $BT/$tool not found. Install Android SDK build-tools 34.0.0." >&2
        exit 1
    fi
done
if [[ ! -f "$AJAR" ]]; then
    echo "ERROR: $AJAR not found. Install Android SDK platform android-34." >&2
    exit 1
fi

# --- Signing keystore ---------------------------------------------------------
KEYSTORE="$PROJ/keystore.jks"
KEYSTORE_PASS="${KEYSTORE_PASS:-}"
KEY_PASS="${KEY_PASS:-}"
if [[ ! -f "$KEYSTORE" ]]; then
    if [[ -z "$KEYSTORE_PASS" && -z "$KEY_PASS" ]]; then
        KEYSTORE_PASS="heckyeah-dev"
        KEY_PASS="heckyeah-dev"
        echo "WARNING: KEYSTORE_PASS is unset; generating a local development" >&2
        echo "WARNING: keystore with the default password 'heckyeah-dev'." >&2
        echo "WARNING: Set KEYSTORE_PASS and KEY_PASS for a real signing key." >&2
    elif [[ -z "$KEYSTORE_PASS" ]]; then
        KEYSTORE_PASS="$KEY_PASS"
    elif [[ -z "$KEY_PASS" ]]; then
        KEY_PASS="$KEYSTORE_PASS"
    fi
    echo "== generating local development keystore =="
    keytool -genkeypair \
        -keystore "$KEYSTORE" \
        -storepass "$KEYSTORE_PASS" -keypass "$KEY_PASS" \
        -alias heckyeah -keyalg RSA -keysize 2048 -validity 10000 \
        -dname "CN=HeckYeah TV, OU=Development, O=HeckYeah TV, C=US"
else
    if [[ -z "$KEYSTORE_PASS" ]]; then
        echo "ERROR: $KEYSTORE exists but KEYSTORE_PASS is not set." >&2
        exit 1
    fi
    if [[ -z "$KEY_PASS" ]]; then
        KEY_PASS="$KEYSTORE_PASS"
    fi
fi

OUT="$PROJ/build/manual"

rm -rf "$OUT"
mkdir -p "$OUT/res" "$OUT/classes" "$OUT/dex" "$OUT/apk"

echo "== aapt2 compile =="
"$BT/aapt2" compile --dir "$PROJ/src/main/res" -o "$OUT/res.zip"

echo "== aapt2 link (with assets) =="
"$BT/aapt2" link -o "$OUT/apk/unsigned.apk" \
  -I "$AJAR" \
  --manifest "$PROJ/src/main/AndroidManifest.xml" \
  --min-sdk-version 25 --target-sdk-version 25 \
  -A "$PROJ/src/main/assets" \
  "$OUT/res.zip"

echo "== javac (source/target 8, android.jar on -cp, NOT -bootclasspath) =="
find "$PROJ/src/main/java" -name '*.java' > "$OUT/sources.txt"
javac -encoding UTF-8 -source 8 -target 8 -cp "$AJAR" \
  -d "$OUT/classes" @"$OUT/sources.txt"

echo "== d8 (min-api 25) =="
find "$OUT/classes" -name '*.class' > "$OUT/classfiles.txt"
"$BT/d8" --min-api 25 --lib "$AJAR" --output "$OUT/dex" @"$OUT/classfiles.txt"

echo "== add classes.dex =="
cp "$OUT/apk/unsigned.apk" "$OUT/apk/app.apk"
(cd "$OUT/dex" && zip -q -j "$OUT/apk/app.apk" classes.dex)

echo "== zipalign =="
"$BT/zipalign" -f -p 4 "$OUT/apk/app.apk" "$OUT/apk/aligned.apk"

echo "== apksigner =="
"$BT/apksigner" sign \
  --ks "$KEYSTORE" --ks-pass "pass:$KEYSTORE_PASS" --key-pass "pass:$KEY_PASS" \
  --v1-signing-enabled true --v2-signing-enabled true \
  --out "$OUT/apk/HeckYeah.apk" "$OUT/apk/aligned.apk"

"$BT/apksigner" verify --print-certs "$OUT/apk/HeckYeah.apk" | head -12
ls -lh "$OUT/apk/HeckYeah.apk"
echo "BUILD OK"
