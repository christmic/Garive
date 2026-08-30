#!/bin/zsh
set -euo pipefail

if (( $# < 1 || $# > 2 )); then
  print -u2 "usage: verify-macos-bundle.sh <Garive.dmg> [local|release]"
  exit 64
fi

dmg_path=$1
audit_mode=${2:-local}
if [[ $audit_mode != local && $audit_mode != release ]]; then
  print -u2 "mode must be local or release"
  exit 64
fi
if [[ ! -f $dmg_path ]]; then
  print -u2 "DMG not found"
  exit 66
fi

audit_mount=$(mktemp -d /tmp/garive-release-audit.XXXXXX)
cleanup() {
  hdiutil detach "$audit_mount" >/dev/null 2>&1 || true
  rmdir "$audit_mount" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

hdiutil verify "$dmg_path"
hdiutil attach -readonly -nobrowse -mountpoint "$audit_mount" "$dmg_path" >/dev/null
app_path="$audit_mount/Garive.app"
binary_path="$app_path/Contents/MacOS/garive-desktop"

codesign --verify --deep --strict --verbose=2 "$app_path"
bundle_id=$(plutil -extract CFBundleIdentifier raw "$app_path/Contents/Info.plist")
[[ $bundle_id == com.garive.desktop ]] || { print -u2 "unexpected bundle identifier"; exit 1; }
[[ -x $binary_path ]] || { print -u2 "bundle executable is missing"; exit 1; }
signature_detail=$(codesign -dvvv "$app_path" 2>&1)
architectures=$(lipo -archs "$binary_path")

if [[ $audit_mode == local ]]; then
  [[ $signature_detail == *"Signature=adhoc"* ]] || { print -u2 "local bundle is not ad-hoc signed"; exit 1; }
  [[ $signature_detail == *"runtime"* ]] || { print -u2 "hardened runtime is missing"; exit 1; }
else
  [[ $signature_detail != *"Signature=adhoc"* ]] || { print -u2 "release bundle is only ad-hoc signed"; exit 1; }
  [[ " $architectures " == *" arm64 "* && " $architectures " == *" x86_64 "* ]] \
    || { print -u2 "release bundle is not universal"; exit 1; }
  spctl --assess --type execute --verbose=4 "$app_path"
  xcrun stapler validate "$app_path"
fi

digest=$(shasum -a 256 "$dmg_path" | awk '{print $1}')
print "Garive macOS bundle verified"
print "mode=$audit_mode"
print "bundle_id=$bundle_id"
print "architectures=$architectures"
print "sha256=$digest"
