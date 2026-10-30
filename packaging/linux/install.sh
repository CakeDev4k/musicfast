#!/bin/sh
# Põe o MusicFast no menu de aplicativos deste usuário (sem sudo): o atalho
# em ~/.local/share/applications e o ícone no tema hicolor. O executável fica
# onde está: rode este script de dentro da pasta extraída, e não a mova
# depois (a atualização automática troca o executável no mesmo lugar).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
mkdir -p "$data/applications" "$data/icons/hicolor/256x256/apps"
cp "$here/musicfast.png" "$data/icons/hicolor/256x256/apps/musicfast.png"
sed "s|@EXEC@|\"$here/musicfast\"|" "$here/musicfast.desktop" > "$data/applications/musicfast.desktop"
chmod 644 "$data/applications/musicfast.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true
echo "MusicFast instalado no menu de aplicativos."
