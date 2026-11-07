#!/usr/bin/env bash
# Collects target/release/trackstudioworld.exe and the Qt, KDE Frameworks and MinGW runtime
# files it loads into a portable folder. Runs in an MSYS2 UCRT64 shell.
#
#   .github/windows/package.sh <output folder>
set -euo pipefail

out=$1
prefix=/ucrt64
qt=$prefix/share/qt6

rm -rf "$out"
mkdir -p "$out/plugins"
cp target/release/trackstudioworld.exe "$out/"
cp LICENSE "$out/LICENSE.txt"

# Qt finds its plugins and QML modules through qt.conf.
cat > "$out/qt.conf" <<'EOF'
[Paths]
Prefix = .
Plugins = plugins
QmlImports = qml
EOF
for d in platforms styles imageformats iconengines tls kf6 kiconthemes6; do
    if [ -d "$qt/plugins/$d" ]; then cp -r "$qt/plugins/$d" "$out/plugins/"; fi
done
cp -r "$qt/qml" "$out/qml"

# The editor's dark colour scheme, and the Breeze icons that libKF6BreezeIcons.dll carries
# (recoloured to the scheme). Qt reads config files next to the executable as the lowest layer,
# so a user's own %LOCALAPPDATA%\kdeglobals still wins.
cp app/data/kdeglobals "$out/"
printf '[Icons]\nTheme=breeze\n' > "$out/trackstudioworldrc"

# Every DLL from the MSYS2 prefix that the executable, the plugins and the QML modules load.
bin=$(cygpath -m "$prefix/bin")
find "$out" -type f \( -iname '*.exe' -o -iname '*.dll' \) -print0 \
    | xargs -0 ntldd -R 2>/dev/null \
    | awk '$2 == "=>" { print $3 }' | tr '\\' '/' | grep -iF "$bin/" | sort -fu \
    | while read -r dll; do
        if [ ! -e "$out/$(basename "$dll")" ]; then cp "$(cygpath -u "$dll")" "$out/"; fi
    done

echo "$out: $(find "$out" -type f | wc -l) files, $(du -sh "$out" | cut -f1)"
du -s "$out"/* "$out"/qml/* | sort -rn | head -n 15
