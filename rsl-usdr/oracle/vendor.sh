#!/usr/bin/env bash
# Refresh the vendored libusdr subset from an upstream checkout at the commit in libusdr/PIN.
#
# Usage: vendor.sh <path-to-usdr-lib-checkout>
# Needs git, python3 and PyYAML (upstream's register-header generator).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
upstream=$(cd "$1" && pwd)
dest="$here/libusdr"
pin=$(cat "$dest/PIN")

head=$(git -C "$upstream" rev-parse HEAD)
if [[ "$head" != "$pin" ]]; then
    echo "upstream checkout is at $head, PIN is $pin" >&2
    exit 1
fi

src="$upstream/src"
rm -rf "$dest/lib" "$dest/gen"
mkdir -p "$dest/lib" "$dest/gen"

# The hardware transports (USB, PCIe, FT601, WebUSB, Verilator) are replaced by the sim plugin
# in shim/; everything else in src/lib is kept so libusdr's device registry links unmodified.
# The register YAMLs are kept too: they are the generated headers' source and document every field.
(cd "$src/lib" && find . \( -name '*.c' -o -name '*.h' -o -name '*.t' -o -name '*.inc' -o -name '*.yaml' \) \
    -not -path './lowlevel/usb_uram/*' -not -path './lowlevel/pcie_uram/*' \
    -not -path './lowlevel/usb_ft601/*' -not -path './lowlevel/verilator_ll/*' \
    -not -path './webusb/*' -not -path '*/utests/*' \
    -not -name 'libusb_generic.[ch]' -not -name 'test_*' -print0 \
    | xargs -0 cp --parents -t "$dest/lib")
# usdr_lowlevel.c includes this USB header for its declarations only.
(cd "$src/lib" && cp --parents lowlevel/usb_uram/usb_uram_generic.h "$dest/lib")

# Register headers upstream generates at CMake time from YAML.
gen() { python3 "$src/hwparser/gen_h.py" --yaml "$1" --ch "$dest/gen/def_$2.h" > "$dest/gen/def_$2.h"; }
for y in "$src"/lib/hw/*/*.yaml; do
    name=$(basename "$y" .yaml)
    [[ "$(basename "$(dirname "$y")")" == "$name" ]] && gen "$y" "$name"
done
for d in m2_dsdr ext_xmass ext_pciefe ext_fe_ch4_400_7200 ext_fe_100_5000; do
    for y in "$src/lib/device/$d"/*.yaml; do
        gen "$y" "$(basename "$y" .yaml)"
    done
done

cp "$upstream/LICENSE" "$dest/LICENSE"
(cd "$dest/lib" && find . -name '*.c' -not -path '*/templates/*' | sed 's|^\./||' | LC_ALL=C sort) > "$dest/sources.txt"
echo "vendored $(wc -l < "$dest/sources.txt") sources at $pin"
