#!/usr/bin/env bash
set -euo pipefail
root_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
arch=${1:-amd64}
destination=${2:-sing-box}
readarray -t release_info < <(python3 - "$root_dir/sing-box-release.json" "$arch" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1]))
print(manifest['version'])
print(manifest['sha256'][sys.argv[2]])
PY
)
if [[ ${#release_info[@]} != 2 ]]; then
    echo "Unsupported architecture or invalid release manifest" >&2
    exit 1
fi
version=${release_info[0]}
checksum=${release_info[1]}
mkdir -p -- "$(dirname -- "$destination")"
staging_dir=$(mktemp -d "$(dirname -- "$destination")/.sing-box-download.XXXXXXXX")
trap 'rm -rf -- "$staging_dir"' EXIT
curl --fail --location --retry 3 --max-time 180 \
    "https://github.com/SagerNet/sing-box/releases/download/v${version}/sing-box-${version}-linux-${arch}.tar.gz" \
    --output "$staging_dir/archive.tar.gz"
echo "$checksum  $staging_dir/archive.tar.gz" | sha256sum --check --status
tar -xzf "$staging_dir/archive.tar.gz" -C "$staging_dir" "sing-box-${version}-linux-${arch}/sing-box"
chmod 755 "$staging_dir/sing-box-${version}-linux-${arch}/sing-box"
mv -- "$staging_dir/sing-box-${version}-linux-${arch}/sing-box" "$destination"
echo "Installed verified sing-box ${version} (${arch}) to ${destination}"
