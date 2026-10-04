#!/usr/bin/env bash
#
# The person cutout's models and the ONNX Runtime it runs them on, pinned and
# checked, for a checkout or for an installer.
#
#   dev/fetch-models.sh                          models/ and target/onnxruntime/
#   dev/fetch-models.sh --models DIR --runtime DIR [--platform NAME]
#
# NAME is windows, macos, linux-x86_64 or linux-aarch64; it defaults to this
# machine. Downloads are kept in ~/.cache/gmx-models and checked against the
# SHA-256 below every time, so a changed file upstream fails here and not on
# somebody's air.
#
# What it fetches, and why each one may ship in an Apache 2.0 app:
#
#   selfie.onnx   MediaPipe selfie segmentation, Apache 2.0 (Google), via the
#                 onnx-community conversion. The fast model: any CPU.
#   modnet.onnx   MODNet portrait matting, Apache 2.0, the Xenova fp16
#                 conversion. The fine model: for a GPU.
#   ONNX Runtime  MIT (Microsoft). 1.24.4 with DirectML 1.15.4 on Windows,
#                 whose licence lets DirectML.dll ship inside an application;
#                 1.30.0 on macOS (CoreML inside) and Linux.
#
# Robust Video Matting is not fetched: its weights are GPL 3.0. A person who
# wants it adds it themselves; see docs/how-to/replace-the-background.md.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODELS="$REPO/models"
RUNTIME="$REPO/target/onnxruntime"
PLATFORM=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --models) MODELS="$2"; shift 2 ;;
        --runtime) RUNTIME="$2"; shift 2 ;;
        --platform) PLATFORM="$2"; shift 2 ;;
        *) echo "unknown argument $1" >&2; exit 2 ;;
    esac
done
if [[ -z "$PLATFORM" ]]; then
    case "$(uname -s)-$(uname -m)" in
        MINGW*|MSYS*|CYGWIN*) PLATFORM=windows ;;
        Darwin-*) PLATFORM=macos ;;
        Linux-aarch64|Linux-arm64) PLATFORM=linux-aarch64 ;;
        Linux-*) PLATFORM=linux-x86_64 ;;
        *) echo "unknown platform $(uname -s)-$(uname -m); pass --platform" >&2; exit 2 ;;
    esac
fi
CACHE="${GMX_MODELS_CACHE:-$HOME/.cache/gmx-models}"
mkdir -p "$CACHE" "$MODELS" "$RUNTIME"
PY="$(command -v python3 || command -v python)"
# A Windows python3 can be the Store's stand in, which fails when run.
"$PY" -c '' 2>/dev/null || PY="$(command -v python)"

ORT=https://github.com/microsoft/onnxruntime/releases/download/v1.30.0
HF=https://huggingface.co

# name | url | sha256
FILES="
selfie.onnx|$HF/onnx-community/mediapipe_selfie_segmentation/resolve/be49485c8e027524be38591817fc5cd31bd9d00e/onnx/model.onnx|3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad
modnet.onnx|$HF/Xenova/modnet/resolve/fa2fa546052fba4c08921230a26cc69a333fca12/onnx/model_fp16.onnx|25f165da9bfd30830a575f1f0490f1acd995975cb349bc02f3d79332e1fe5cf6
ort-dml.nupkg|https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime.DirectML/1.24.4|57e9f11b73437bef7a309496135d4c1f96b1a8e9ddba60013fa27bfc1d788681
directml.nupkg|https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/1.15.4|4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9
ort-osx-arm64.tgz|$ORT/onnxruntime-osx-arm64-1.30.0.tgz|6ebb5062a934537c352937821f9fe9718e7de1a2db1122a93dd363ffd53a7012
ort-linux-x64.tgz|$ORT/onnxruntime-linux-x64-1.30.0.tgz|a5ed5a3cac51fbb2e90da632ae43d19212faaa20e76484e62bcb7c23ddb3b3fd
ort-linux-aarch64.tgz|$ORT/onnxruntime-linux-aarch64-1.30.0.tgz|e16a27a8ed330bbc698df7330b0cf56e722f354e3bcc92118682c74ef3c3e3da
"

sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

# Fetch one file into the cache, checked. Answers its path.
fetch() {
    local line url want got path
    line="$(grep "^$1|" <<<"$FILES")"
    url="$(cut -d'|' -f2 <<<"$line")"
    want="$(cut -d'|' -f3 <<<"$line")"
    path="$CACHE/$1"
    if [[ ! -f "$path" || "$(sha "$path")" != "$want" ]]; then
        echo "fetching $1" >&2
        curl -sSfL --retry 5 -o "$path.part" "$url"
        mv "$path.part" "$path"
    fi
    got="$(sha "$path")"
    [[ "$got" == "$want" ]] || { echo "FAIL $1: sha256 $got, expected $want" >&2; exit 1; }
    echo "$path"
}

# Copy members out of an archive: archive, then pairs of (member suffix, file).
extract() {
    "$PY" - "$@" <<'PY'
import sys, zipfile, tarfile, os, shutil
archive, out, pairs = sys.argv[1], sys.argv[2], sys.argv[3:]
want = dict(zip(pairs[::2], pairs[1::2]))
def put(name, data):
    path = os.path.join(out, name)
    with open(path, "wb") as f:
        f.write(data)
    os.chmod(path, 0o755)
found = set()
if archive.endswith((".nupkg", ".zip")):
    z = zipfile.ZipFile(archive)
    for i in z.infolist():
        for suffix, name in want.items():
            if i.filename.endswith(suffix):
                put(name, z.read(i)); found.add(suffix)
else:
    t = tarfile.open(archive)
    for m in t.getmembers():
        for suffix, name in want.items():
            if m.name.endswith(suffix) and m.isfile():
                put(name, t.extractfile(m).read()); found.add(suffix)
missing = set(want) - found
if missing:
    sys.exit(f"{archive} has no {', '.join(sorted(missing))}")
PY
}

cp "$(fetch selfie.onnx)" "$MODELS/selfie.onnx"
cp "$(fetch modnet.onnx)" "$MODELS/modnet.onnx"
cat >"$MODELS/LICENSES.md" <<'TXT'
# The models in this folder

* `selfie.onnx`: MediaPipe selfie segmentation, Apache License 2.0, Copyright
  Google LLC. ONNX conversion by onnx-community on Hugging Face.
* `modnet.onnx`: MODNet portrait matting, Apache License 2.0, Copyright
  Zhanghan Ke and contributors. ONNX conversion (fp16) by Xenova on Hugging
  Face.

The Apache License 2.0 is at https://www.apache.org/licenses/LICENSE-2.0.
A model you add here keeps its own licence.
TXT

case "$PLATFORM" in
    windows)
        extract "$(fetch ort-dml.nupkg)" "$RUNTIME" \
            runtimes/win-x64/native/onnxruntime.dll onnxruntime.dll \
            runtimes/win-x64/native/onnxruntime_providers_shared.dll onnxruntime_providers_shared.dll \
            LICENSE ONNXRUNTIME-LICENSE.txt ThirdPartyNotices.txt ONNXRUNTIME-ThirdPartyNotices.txt
        extract "$(fetch directml.nupkg)" "$RUNTIME" \
            bin/x64-win/DirectML.dll DirectML.dll LICENSE.txt DIRECTML-LICENSE.txt \
            ThirdPartyNotices.txt DIRECTML-ThirdPartyNotices.txt
        ;;
    macos)
        extract "$(fetch ort-osx-arm64.tgz)" "$RUNTIME" \
            lib/libonnxruntime.1.30.0.dylib libonnxruntime.dylib \
            1.30.0/LICENSE ONNXRUNTIME-LICENSE.txt ThirdPartyNotices.txt ONNXRUNTIME-ThirdPartyNotices.txt
        ;;
    linux-x86_64|linux-aarch64)
        arch="${PLATFORM#linux-}"; [[ "$arch" == x86_64 ]] && arch=x64
        extract "$(fetch "ort-linux-$arch.tgz")" "$RUNTIME" \
            lib/libonnxruntime.so.1.30.0 libonnxruntime.so \
            lib/libonnxruntime_providers_shared.so libonnxruntime_providers_shared.so \
            1.30.0/LICENSE ONNXRUNTIME-LICENSE.txt ThirdPartyNotices.txt ONNXRUNTIME-ThirdPartyNotices.txt
        ;;
    *) echo "unknown platform $PLATFORM" >&2; exit 2 ;;
esac
echo "models in $MODELS ($(du -sh "$MODELS" | cut -f1)), runtime for $PLATFORM in $RUNTIME ($(du -sh "$RUNTIME" | cut -f1))"
