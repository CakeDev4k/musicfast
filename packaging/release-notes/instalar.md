

## Como instalar

| Sistema | Arquivo |
| --- | --- |
| Windows 10/11 (64 bits), instalador | `musicfast-v…-x86_64-pc-windows-msvc-setup.exe` |
| Windows 10/11 (64 bits), portátil | `musicfast-v…-x86_64-pc-windows-msvc.zip` |
| Linux x86_64 | `musicfast-v…-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `musicfast-v…-aarch64-unknown-linux-gnu.tar.gz` |
| macOS 11+ (Apple silicon e Intel) | `musicfast-v…-macos-universal.dmg` |

- **Windows, instalador:** abra o `-setup.exe`. Instala só para o seu usuário, sem pedir administrador, com atalho no menu Iniciar.
- **Windows, portátil:** extraia o zip numa pasta sua (por exemplo `Documentos\MusicFast`) e abra o `musicfast.exe`.
- Nos dois, o executável não tem assinatura digital: no aviso do SmartScreen, clique em **Mais informações → Executar assim mesmo**.
- **Linux:** extraia o `.tar.gz`, rode `./install.sh` de dentro da pasta para pôr o MusicFast no menu, ou abra o `./musicfast` direto. Precisa do WebKitGTK 4.1 para o login (`libwebkit2gtk-4.1-0` no Debian/Ubuntu).
- **macOS:** abra o `.dmg` e arraste o MusicFast para Aplicativos. O app não é assinado pela Apple: na primeira vez, clique com o botão direito nele e escolha **Abrir** (ou rode `xattr -dr com.apple.quarantine /Applications/MusicFast.app`).

Na primeira música, o app baixa o [yt-dlp](https://github.com/yt-dlp/yt-dlp) e o [Deno](https://deno.com) (conferidos pelo SHA-256 das releases deles). Depois disso, abre em um segundo.

No Windows e no Linux o MusicFast se atualiza sozinho (o pacote é conferido por uma assinatura Ed25519). No macOS ele avisa da versão nova e abre esta página.
