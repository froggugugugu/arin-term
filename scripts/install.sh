#!/bin/bash
# Installs Arin on a Mac (Apple silicon) into /Applications.
#
#   curl -fsSL https://raw.githubusercontent.com/froggugugugu/arin-term/main/scripts/install.sh | bash
#   (the URL follows REPO below; where the repository is published is not decided yet)
#
# Ported from tadcap's scripts/install.sh. Releases are meant to be Developer ID signed and notarized
# (.github/workflows/release.yml), so Gatekeeper accepts them. curl does not mark what it downloads with
# com.apple.quarantine, and the mark is removed below anyway, so an ad-hoc signed build also opens without the
# "is damaged" alert.
# Unlike tadcap, a running Arin is not stopped: it holds the user's shells. The app is replaced in place and the new
# version starts the next time Arin is opened.
# Nothing runs until the last line calls main, so a download cut off halfway does nothing.
#
# Options (environment variables, e.g. `... | ARIN_INSTALL_NO_OPEN=1 bash`):
#   ARIN_INSTALL_VERSION=0.2.0  install this release instead of the latest
#   ARIN_INSTALL_APP_DIR=DIR    install into DIR (default /Applications, or ~/Applications when that is not writable)
#   ARIN_INSTALL_ZIP=FILE       install from a downloaded Arin-<version>-arm64.zip instead of GitHub
#   ARIN_INSTALL_NO_OPEN=1      do not open the app at the end
set -euo pipefail

# The GitHub repository the releases come from (owner/name). Change only this line when it moves.
REPO="froggugugugu/arin-term"
WORK=""
# https only, TLS 1.2 or later, redirects included.
CURL=(curl --proto '=https' --tlsv1.2)

say() { printf '%s\n' "$*"; }
fail() { printf 'arin: %s\n' "$*" >&2; exit 1; }
cleanup() { if [ -n "$WORK" ]; then rm -rf "$WORK"; fi; }

# Apple silicon, also when the terminal runs under Rosetta (uname -m then says x86_64).
is_apple_silicon() { [ "$(uname -m)" = arm64 ] || [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; }

# Whether the app at $1 is running (the executable is Contents/MacOS/arin; see CFBundleExecutable).
app_running() {
  ps -axo command= | awk -v exe="$1/Contents/MacOS/arin" 'index($0, exe) == 1 { found = 1 } END { exit !found }'
}

# releases/latest redirects to releases/tag/v<version>.
latest_version() {
  local url
  url="$("${CURL[@]}" -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")" || return 1
  printf '%s\n' "${url##*/v}"
}

# The sha256 GitHub recorded for asset $2 of release v$1. A download that differs is not installed.
release_digest() {
  local json="$WORK/release.json" i=0 name
  "${CURL[@]}" -fsSL -H 'Accept: application/vnd.github+json' -o "$json" "https://api.github.com/repos/$REPO/releases/tags/v$1" || return 1
  while name="$(plutil -extract "assets.$i.name" raw -o - "$json" 2>/dev/null)"; do
    if [ "$name" = "$2" ]; then
      plutil -extract "assets.$i.digest" raw -o - "$json" 2>/dev/null
      return
    fi
    i=$((i + 1))
  done
  return 1
}

main() {
  [ "$(uname -s)" = Darwin ] || fail "Arin は macOS 専用です。"
  is_apple_silicon || fail "配布版は Apple silicon の Mac 向けです。Intel の Mac ではソースからビルドしてください: https://github.com/$REPO"

  local app_dir="${ARIN_INSTALL_APP_DIR:-}"
  if [ -z "$app_dir" ]; then
    if [ -w /Applications ]; then app_dir=/Applications; else app_dir="$HOME/Applications"; fi
  fi
  local target="$app_dir/Arin.app"

  WORK="$(mktemp -d)"
  trap cleanup EXIT

  local zip="${ARIN_INSTALL_ZIP:-}" version
  if [ -n "$zip" ]; then
    [ -f "$zip" ] || fail "$zip がありません。"
  else
    version="${ARIN_INSTALL_VERSION:-}"
    if [ -z "$version" ]; then
      version="$(latest_version)" || fail "最新版を調べられませんでした。ネットワークを確かめてください。"
    fi
    version="${version#v}"
    case "$version" in
      "" | *[!0-9.]*) fail "版を読み取れませんでした: $version" ;;
    esac
    local asset="Arin-$version-arm64.zip"
    zip="$WORK/$asset"
    say "Arin $version をダウンロードしています..."
    "${CURL[@]}" -fL --progress-bar -o "$zip" "https://github.com/$REPO/releases/download/v$version/$asset" \
      || fail "Arin $version をダウンロードできませんでした。"
    local want sum
    want="$(release_digest "$version" "$asset")" \
      || fail "ダウンロードを照合する sha256 を GitHub から取れませんでした。入れませんでした。"
    sum="$(shasum -a 256 "$zip")"
    [ "$want" = "sha256:${sum%% *}" ] || fail "ダウンロードしたファイルが公開されたものと一致しません（sha256）。入れませんでした。"
  fi

  # --noqtn: a zip downloaded with a browser must not pass its quarantine mark on to the app.
  ditto -x -k --noqtn "$zip" "$WORK/unpacked" || fail "$zip を展開できませんでした。"
  local app="$WORK/unpacked/Arin.app" got
  [ -d "$app" ] || fail "$zip に Arin.app が入っていません。"
  codesign --verify --deep --strict "$app" >/dev/null 2>&1 \
    || fail "Arin.app の署名を確かめられませんでした。ダウンロードが壊れている可能性があるので、入れませんでした。"
  got="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist" 2>/dev/null || true)"

  mkdir -p "$app_dir" || fail "$app_dir を作れませんでした。"
  local running=0
  if app_running "$target"; then running=1; fi
  if [ -e "$target" ]; then
    mv "$target" "$WORK/previous.app" || fail "$target を置き換えられませんでした。"
  fi
  if ! mv "$app" "$target"; then
    if [ -e "$WORK/previous.app" ]; then mv "$WORK/previous.app" "$target"; fi
    fail "$target に入れられませんでした。"
  fi
  xattr -dr com.apple.quarantine "$target" 2>/dev/null || true
  say "Arin ${got:-} を $target に入れました。"

  if [ "$running" = 1 ]; then
    say "起動中の Arin はそのまま動いています。Arin を終了して開き直すと、新しい版になります。"
  elif [ "${ARIN_INSTALL_NO_OPEN:-}" != 1 ]; then
    open "$target" || true
  fi
}

main
