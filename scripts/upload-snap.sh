#!/usr/bin/env bash
# upload-snap.sh RUN_ID [CHANNEL]: upload the snap that the "Store packages" run RUN_ID built, to the Snap Store.
#   Needs a Snap Store login (SNAPCRAFT_STORE_CREDENTIALS or `snapcraft login`). The first upload needs the name:
#   snapcraft register rustidian
# Downloads go direct, never through the proxy variables. The default channel is edge; promote with
#   snapcraft release rustidian REVISION stable
set -euo pipefail
run=${1:?usage: upload-snap.sh RUN_ID [CHANNEL]}; channel=${2:-edge}
unset HTTPS_PROXY HTTP_PROXY ALL_PROXY https_proxy http_proxy all_proxy
dir=$(mktemp -d "${TMPDIR:-/tmp}/rustidian-snap.XXXXXX")
gh run download "$run" --repo MasihMoafi/rustidian --name rustidian-snap --dir "$dir"
(cd "$dir" && sha256sum --check rustidian_amd64.snap.sha256)
unsquashfs -q -cat "$dir/rustidian_amd64.snap" meta/snap.yaml | rg '^(name|version|confinement):'
snapcraft upload "$dir/rustidian_amd64.snap" --release="$channel"
