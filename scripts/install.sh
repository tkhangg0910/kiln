#!/bin/sh
# Installs Kilno, or updates it, into ~/.local/lib/kilno, then starts it. No sudo, no change to
# your shell's rc files, and start at login stays as you set it (off until you turn it on).
#   curl -fsSL https://raw.githubusercontent.com/tkhangg0910/kilno/main/scripts/install.sh | sh
# Kilno's Update button runs this too, with KILNO_DIR naming the folder it runs from.
# Settings live in ~/.local/share/kilno and are kept.
set -eu

# Out of the install folder first: a shell standing in it must not hold it while it is swapped.
cd "${TMPDIR:-/tmp}"

dir="${KILNO_DIR:-$HOME/.local/lib/kilno}"
dir="${dir%/}"
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) ;;
    *) echo "Kilno is built for Linux x86_64 only." >&2; exit 1 ;;
esac

api=https://api.github.com/repos/tkhangg0910/kilno/releases/latest
json=$(curl -fsSL "$api")
url=$(printf '%s\n' "$json" | grep -o '"browser_download_url": *"[^"]*-linux-x64\.tar\.gz"' | head -n 1 | sed 's/.*"\(https[^"]*\)"/\1/')
[ -n "$url" ] || { echo "The latest Kilno release has no Linux download." >&2; exit 1; }
name=$(basename "$url")
echo "Downloading $name..."
curl -fsSL "$url" -o "$work/$name"
curl -fsSL "$url.sha256" -o "$work/$name.sha256"
want=$(cut -d ' ' -f 1 < "$work/$name.sha256")
got=$(sha256sum "$work/$name" | cut -d ' ' -f 1)
[ "$want" = "$got" ] || { echo "The download doesn't match its checksum, so it wasn't installed." >&2; exit 1; }
tar -C "$work" -xzf "$work/$name"

# The Update button started this as Kilno quit; give it a moment, then close one that hangs.
for _ in 1 2 3 4 5 6 7 8 9 10; do
    pgrep -x kilno > /dev/null 2>&1 || break
    sleep 1
done
pkill -x kilno 2> /dev/null || true

# A swap rather than an unzip over the top, so files a new version dropped do not linger; a
# failed swap puts the old Kilno back.
mkdir -p "$(dirname "$dir")"
if [ -d "$dir" ]; then mv "$dir" "$dir.old"; fi
if mv "$work/kilno" "$dir"; then
    rm -rf "$dir.old"
else
    [ -d "$dir.old" ] && mv "$dir.old" "$dir"
    exit 1
fi

mkdir -p "$bin" "$apps" "$icons"
ln -sf "$dir/kilno" "$bin/kilno"
sed "s|^Exec=kilno|Exec=\"$dir/kilno\"|" "$dir/kilno.desktop" > "$apps/kilno.desktop"
cp "$dir/kilno.png" "$icons/kilno.png"
echo "Kilno is installed in $dir."
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "Add $bin to your PATH to run kilno from a terminal." ;;
esac

# The new Kilno, when there is a desktop to show it on.
if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    nohup "$dir/kilno" > /dev/null 2>&1 &
fi
