# Sourced by dev/bundle-browser.sh. Points the cef crate at GodwinMix's own
# build of CEF, which plays H.264 and AAC, instead of the official one, which
# does not. The builds are releases of https://github.com/psmux/godwinmix-cef,
# one per CEF version, in the official layout with an index.json, so the
# crate's downloader takes them unchanged through CEF_DOWNLOAD_URL.
#
# use_cef_codecs      before the build: finds this platform's archive in the
#                     release for the CEF version browser/Cargo.lock pins, or
#                     fails naming the version and how to build it; removes
#                     an engine in CEF_PATH that is not that archive. A
#                     platform not in CEF_CODECS_PLATFORMS keeps the official
#                     CEF, without H.264 and AAC, and says so.
# check_cef_codecs    after the build: the archive the crate downloaded has
#                     the SHA-256 the release publishes

CEF_CODECS_REPO="${CEF_CODECS_REPO:-psmux/godwinmix-cef}"
# The platforms we publish a build for: all six that CEF ships
# (docs/how-to/build-the-web-page-engine.md).
CEF_CODECS_PLATFORMS="${CEF_CODECS_PLATFORMS:-windows64 windowsarm64 linux64 linuxarm64 macosarm64 macosx64}"

# The CEF version browser/ pins: the +metadata of cef-dll-sys in its lock file.
cef_codecs_version() {
    grep -A1 '^name = "cef-dll-sys"$' "$REPO/browser/Cargo.lock" | sed -n 's/^version = "[^+]*+\(.*\)"$/\1/p'
}

# This machine as CEF names it, and the folder the crate unpacks into.
cef_codecs_platform() {
    case "$(uname -s)/$(uname -m)" in
        Darwin/arm64) echo "macosarm64 cef_macos_aarch64" ;;
        Darwin/x86_64) echo "macosx64 cef_macos_x86_64" ;;
        Linux/x86_64) echo "linux64 cef_linux_x86_64" ;;
        Linux/aarch64) echo "linuxarm64 cef_linux_aarch64" ;;
        MINGW*/x86_64|MSYS*/x86_64|CYGWIN*/x86_64) echo "windows64 cef_windows_x86_64" ;;
        MINGW*/aarch64|MSYS*/aarch64|MINGW*/arm64|MSYS*/arm64) echo "windowsarm64 cef_windows_aarch64" ;;
        *) echo "no CEF platform for $(uname -s) on $(uname -m)" >&2; return 1 ;;
    esac
}

use_cef_codecs() {
    local version platform os_arch
    version="$(cef_codecs_version)"
    [[ -n "$version" ]] || { echo "no cef-dll-sys with a +version in browser/Cargo.lock" >&2; return 1; }
    read -r platform os_arch < <(cef_codecs_platform) || return 1
    if [[ " $CEF_CODECS_PLATFORMS " != *" $platform "* ]]; then
        echo "GodwinMix publishes no CEF with H.264 and AAC for $platform yet; building against"
        echo "the official CEF $version, which plays neither (CEF_CODECS_PLATFORMS=\"$CEF_CODECS_PLATFORMS\")."
        CEF_CODECS_ARCHIVE=""
        return 0
    fi
    # CEF_CODECS_BASE points at another copy of the release, such as a folder
    # served over HTTP, to try a draft before it is published.
    local base="${CEF_CODECS_BASE:-https://github.com/$CEF_CODECS_REPO/releases/download/cef-$version}"
    local index; index="$(curl -fsSL --retry 5 "$base/index.json" 2>/dev/null || true)"
    CEF_CODECS_ARCHIVE="$(grep -o "cef_binary_$version+[^\"]*_${platform}_minimal\.tar\.bz2" <<<"$index" | head -1)"
    if [[ -z "$CEF_CODECS_ARCHIVE" ]]; then
        echo "GodwinMix's CEF $version with H.264 and AAC has no $platform build yet" >&2
        echo "(looked in https://github.com/$CEF_CODECS_REPO/releases/tag/cef-$version)." >&2
        echo "Build and publish it on a $platform machine, then run this again:" >&2
        echo "  git clone https://github.com/$CEF_CODECS_REPO && cd godwinmix-cef" >&2
        echo "  ./build.sh $version && ./publish.sh $version" >&2
        return 1
    fi
    local sha1; sha1="$(curl -fsSL --retry 5 "$base/$CEF_CODECS_ARCHIVE.sha1")"
    CEF_CODECS_SHA256="$(curl -fsSL --retry 5 "$base/$CEF_CODECS_ARCHIVE.sha256")"
    CEF_CODECS_DIR="$CEF_PATH/$version"
    local installed="$CEF_CODECS_DIR/$os_arch/archive.json"
    if [[ -f "$installed" ]] && ! grep -q "\"$sha1\"" "$installed"; then
        echo "replacing the CEF in $CEF_CODECS_DIR/$os_arch, which is not GodwinMix's build with codecs"
        rm -rf "${CEF_CODECS_DIR:?}/$os_arch"
    fi
    export CEF_DOWNLOAD_URL="$base"
    echo "CEF $version for $platform with H.264 and AAC: $base/$CEF_CODECS_ARCHIVE"
}

check_cef_codecs() {
    [[ -n "${CEF_CODECS_ARCHIVE:-}" ]] || return 0
    local archive="$CEF_CODECS_DIR/$CEF_CODECS_ARCHIVE" got
    [[ -f "$archive" ]] || { echo "the build did not leave $archive" >&2; return 1; }
    if command -v sha256sum >/dev/null; then got="$(sha256sum "$archive")"; else got="$(shasum -a 256 "$archive")"; fi
    if [[ "${got%% *}" != "$CEF_CODECS_SHA256" ]]; then
        echo "$archive has SHA-256 ${got%% *}, the release says $CEF_CODECS_SHA256; delete it and run this again" >&2
        return 1
    fi
    echo "CEF archive checked: SHA-256 $CEF_CODECS_SHA256"
}
