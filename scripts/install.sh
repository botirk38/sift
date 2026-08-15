#!/usr/bin/env sh
# Install the `sift` and `sift-daemon` binaries from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/botirk38/sift/master/scripts/install.sh | sh
# Resolves the latest release by default (override with SIFT_VERSION=...).
#
# Environment:
#   SIFT_REPO   — owner/repo (default: botirk38/sift)
#   SIFT_VERSION — release version without leading v (default: latest GitHub release)
#   PREFIX      — install directory (default: $HOME/.local)
set -eu

SIFT_REPO="${SIFT_REPO:-botirk38/sift}"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="${BIN_DIR:-$PREFIX/bin}"

# Managed by scripts/release.sh — do not edit manually.
SIFT_DEFAULT_VERSION="0.8.1"

# Optional: pin a version, e.g. export SIFT_VERSION=0.1.2
resolve_version() {
	if [ -n "${SIFT_VERSION:-}" ]; then
		printf '%s\n' "$SIFT_VERSION"
		return
	fi
	# GitHub REST API requires a User-Agent; unauthenticated calls are rate-limited (~60/hr/IP).
	_json=$(
		curl -fsSL \
			-H "Accept: application/vnd.github+json" \
			-H "User-Agent: sift-install-script" \
			"https://api.github.com/repos/${SIFT_REPO}/releases/latest"
	) || _json=""
	_ver=$(printf '%s' "$_json" | tr -d '\n' | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\([^"]*\)".*/\1/p' | head -1)
	if [ -n "$_ver" ]; then
		printf '%s\n' "$_ver"
		return
	fi
	echo "install: could not query releases/latest (rate limit or network); using SIFT_DEFAULT_VERSION=${SIFT_DEFAULT_VERSION} (override with SIFT_VERSION=... )" >&2
	printf '%s\n' "$SIFT_DEFAULT_VERSION"
}

detect_assets() {
	_os=$(uname -s)
	_arch=$(uname -m)
	case "${_os}:${_arch}" in
	Linux:x86_64)
		printf '%s\n' "sift-x86_64-unknown-linux-gnu"
		printf '%s\n' "sift-daemon-x86_64-unknown-linux-gnu"
		;;
	Linux:aarch64)
		printf '%s\n' "sift-aarch64-unknown-linux-gnu"
		printf '%s\n' "sift-daemon-aarch64-unknown-linux-gnu"
		;;
	Darwin:arm64)
		printf '%s\n' "sift-aarch64-apple-darwin"
		printf '%s\n' "sift-daemon-aarch64-apple-darwin"
		;;
	Darwin:x86_64)
		printf '%s\n' "sift-x86_64-apple-darwin"
		printf '%s\n' "sift-daemon-x86_64-apple-darwin"
		;;
	MINGW*|MSYS*|CYGWIN*)
		printf '%s\n' "sift-x86_64-pc-windows-msvc.exe"
		printf '%s\n' "sift-daemon-x86_64-pc-windows-msvc.exe"
		;;
	*)
		echo "install: unsupported OS/arch: ${_os} ${_arch}" >&2
		echo "install: try: cargo install --locked --git https://github.com/${SIFT_REPO}.git sift-grep" >&2
		exit 1
		;;
	esac
}

install_name() {
	case "$1" in
	*.exe)
		case "$1" in
		sift-daemon-*) printf '%s\n' "sift-daemon.exe" ;;
		*) printf '%s\n' "sift.exe" ;;
		esac
		;;
	sift-daemon-*)
		printf '%s\n' "sift-daemon"
		;;
	*)
		printf '%s\n' "sift"
		;;
	esac
}

fallback_cargo() {
	if ! command -v cargo >/dev/null 2>&1; then
		echo "install: curl download failed and cargo not found; install Rust from https://rustup.rs" >&2
		exit 1
	fi
	echo "install: falling back to: cargo install --locked --git https://github.com/${SIFT_REPO}.git sift-grep" >&2
	cargo install --locked --git "https://github.com/${SIFT_REPO}.git" sift-grep || exit 1
	INSTALLED_DAEMON=1
}

install_release_assets() {
	_sift_tmp="${TMPDIR:-/tmp}/sift-install-${SIFT_ASSET}-$$"
	_daemon_tmp="${TMPDIR:-/tmp}/sift-install-${DAEMON_ASSET}-$$"
	_sift_url="https://github.com/${SIFT_REPO}/releases/download/${TAG}/${SIFT_ASSET}"
	_daemon_url="https://github.com/${SIFT_REPO}/releases/download/${TAG}/${DAEMON_ASSET}"
	if ! curl -fsSL -o "$_sift_tmp" "$_sift_url"; then
		rm -f "$_sift_tmp" "$_daemon_tmp"
		return 1
	fi
	if ! curl -fsSL -o "$_daemon_tmp" "$_daemon_url"; then
		rm -f "$_daemon_tmp"
		mv "$_sift_tmp" "${BIN_DIR}/${SIFT_BIN}"
		case "$SIFT_ASSET" in
		*.exe) ;;
		*) chmod +x "${BIN_DIR}/${SIFT_BIN}" ;;
		esac
		echo "install: ${TAG} has no ${DAEMON_ASSET} release asset; installed ${SIFT_BIN} only" >&2
		INSTALLED_DAEMON=0
		return 0
	fi
	mv "$_sift_tmp" "${BIN_DIR}/${SIFT_BIN}"
	mv "$_daemon_tmp" "${BIN_DIR}/${DAEMON_BIN}"
	case "$SIFT_ASSET" in
	*.exe) ;;
	*) chmod +x "${BIN_DIR}/${SIFT_BIN}" "${BIN_DIR}/${DAEMON_BIN}" ;;
	esac
	INSTALLED_DAEMON=1
	return 0
}

VERSION=$(resolve_version)
TAG="v${VERSION}"
ASSETS=$(detect_assets)
SIFT_ASSET=$(printf '%s' "$ASSETS" | sed -n '1p')
DAEMON_ASSET=$(printf '%s' "$ASSETS" | sed -n '2p')
SIFT_BIN=$(install_name "$SIFT_ASSET")
DAEMON_BIN=$(install_name "$DAEMON_ASSET")

if [ -x "${BIN_DIR}/${SIFT_BIN}" ]; then
	_old_ver=$("${BIN_DIR}/${SIFT_BIN}" --version 2>/dev/null || true)
	echo "install: upgrading ${SIFT_BIN} (${_old_ver:-unknown}) → ${TAG}" >&2
fi

mkdir -p "$BIN_DIR"

INSTALLED_DAEMON=0

if ! install_release_assets; then
	fallback_cargo
fi

if [ "$INSTALLED_DAEMON" = "1" ]; then
	echo "Installed sift and sift-daemon to ${BIN_DIR}"
	echo "  ${BIN_DIR}/${SIFT_BIN}"
	echo "  ${BIN_DIR}/${DAEMON_BIN}"
else
	echo "Installed sift to ${BIN_DIR}"
	echo "  ${BIN_DIR}/${SIFT_BIN}"
fi
echo "Ensure ${BIN_DIR} is on your PATH (e.g. export PATH=\"${BIN_DIR}:\$PATH\")"
