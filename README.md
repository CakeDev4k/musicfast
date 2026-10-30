# MusicFast

Um cliente nativo e leve do **YouTube Music** para Windows, Linux e macOS,
escrito em Rust com [egui](https://github.com/emilk/egui). Abre em menos de
um segundo, usa pouca memória e não carrega o site inteiro num navegador.

> **Aviso.** O MusicFast é um projeto independente, sem relação com o Google
> ou o YouTube. "YouTube" e "YouTube Music" são marcas do Google LLC. O app
> usa a API interna do site e o [yt-dlp](https://github.com/yt-dlp/yt-dlp),
> que podem mudar sem aviso. Usar clientes não oficiais pode contrariar os
> Termos de Serviço do YouTube; use por sua conta e risco.

*English summary: a fast native YouTube Music desktop client (Windows, Linux,
macOS) written in Rust/egui. The interface speaks Portuguese, English and
Spanish, following the system language.
Download it from the [releases page](https://github.com/CakeDev4k/musicfast/releases/latest).*

## Recursos

- Início, Explorar, busca com sugestões, álbuns, artistas, playlists e
  rádio, como no site.
- Login com a conta Google: biblioteca, curtidas, playlists, histórico. Com
  YouTube Premium, o áudio vem em 256 kbps; músicas com restrição de idade
  tocam.
- Letras sincronizadas do [LRCLIB](https://lrclib.net), com a linha cantada
  acendendo palavra por palavra (no estilo do Better Lyrics). Clique numa
  linha para pular até ela.
- Mini player sempre no topo (o alfinete sobre a capa, ou `Ctrl+Shift+M`).
- Em português, inglês e espanhol: segue o idioma do sistema (no Windows, o
  de exibição) ou o escolhido em **Idioma**, no menu da conta.
- Personalização (menu ⋮ da barra → **Personalizar o player**) da tela
  principal, da tela do player, da barra de baixo e do mini player: fundo
  com a cor da capa, uma cor fixa ou uma imagem/GIF (do computador ou de um
  link), com escurecer e desfoque, e um visualizador que se mexe com o som,
  em 11 estilos, com cores, quantidade, sensibilidade, opacidade, espessura
  e brilho ajustáveis.
- Fila, aleatório, repetir, pré-carga da próxima música.
- Controles de mídia do sistema (teclas de mídia, o painel do Windows, MPRIS
  no Linux, Now Playing no macOS).
- Atualização automática pelas releases do GitHub, com o pacote conferido
  por uma assinatura Ed25519.

## Instalar

Baixe o arquivo do seu sistema na
[página de releases](https://github.com/CakeDev4k/musicfast/releases/latest):

| Sistema | Arquivo |
| --- | --- |
| Windows 10/11 (64 bits), instalador | `musicfast-v…-x86_64-pc-windows-msvc-setup.exe` |
| Windows 10/11 (64 bits), portátil | `musicfast-v…-x86_64-pc-windows-msvc.zip` |
| Linux x86_64 | `musicfast-v…-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `musicfast-v…-aarch64-unknown-linux-gnu.tar.gz` |
| macOS 11+ (Apple silicon e Intel) | `musicfast-v…-macos-universal.dmg` |

- **Windows, instalador:** abra o `-setup.exe`. Instala só para o seu usuário
  (sem pedir administrador) em `%LOCALAPPDATA%\Programs\MusicFast`, com
  atalho no menu Iniciar e desinstalação por "Aplicativos instalados".
- **Windows, portátil:** extraia o zip numa pasta sua e abra o
  `musicfast.exe`. Nada é instalado; para remover, apague a pasta.
- Nos dois, o executável não tem assinatura digital: no aviso do
  SmartScreen, clique em **Mais informações → Executar assim mesmo**.
- **Linux:** extraia o `.tar.gz` e rode `./install.sh` para pôr o MusicFast
  no menu de aplicativos (ou abra o `./musicfast` direto). O login precisa do
  WebKitGTK 4.1 (`libwebkit2gtk-4.1-0` no Debian/Ubuntu, `webkit2gtk4.1` no
  Fedora).
- **macOS:** abra o `.dmg` e arraste o MusicFast para Aplicativos. O app não
  é assinado pela Apple: na primeira vez, clique com o botão direito nele e
  escolha **Abrir**.

Na primeira música o app baixa o yt-dlp e o Deno (conferidos pelo SHA-256
das releases oficiais deles), uma vez só.

### Atualizações

No Windows e no Linux o MusicFast pergunta ao GitHub uma vez por dia se saiu
versão nova, baixa em segundo plano, confere a assinatura e mostra
**Reiniciar para atualizar** na barra de cima. Se a versão nova não abrir, a
anterior volta sozinha. No macOS (app sem assinatura da Apple) ele mostra
**Versão nova**, que abre a página da release.

### Onde ficam os dados

| Sistema | Pasta |
| --- | --- |
| Windows | `%LOCALAPPDATA%\MusicFast` |
| macOS | `~/Library/Application Support/MusicFast` |
| Linux | `~/.local/share/musicfast` |

A sessão da conta fica cifrada pelo DPAPI no Windows, no Chaveiro do macOS e
no Secret Service do Linux (GNOME Keyring, KWallet). O app nunca vê a sua
senha: o login acontece na página do próprio Google, no navegador embutido
do sistema.

## Compilar

Precisa do Rust (o `rust-toolchain.toml` escolhe a versão sozinho pelo
[rustup](https://rustup.rs)).

```bash
cargo run --release -p musicfast
```

No Windows, o `iniciar.bat` compila e abre. No Linux, instale antes as
bibliotecas de desenvolvimento:

```bash
sudo apt install pkg-config libssl-dev libasound2-dev libgtk-3-dev libwebkit2gtk-4.1-dev libxkbcommon-dev libgl1-mesa-dev
```

Testes: `cargo test --workspace`.

### Como está organizado

| Crate | O que faz |
| --- | --- |
| `crates/i18n` | Os textos em português, inglês e espanhol, e o idioma do sistema. |
| `crates/ytm` | O catálogo do YouTube Music pela API interna do site (InnerTube). |
| `crates/player` | Tocar: o yt-dlp resolve o áudio, o Symphonia decodifica o AAC, a fila e o rádio. |
| `crates/app` | A janela (egui): telas, barra do player, letras, mini player, personalização, login, atualização. |
| `crates/release-sign` | Gera a chave das atualizações e assina as releases. |

## Publicar uma versão

1. Suba a versão em `Cargo.toml` (`[workspace.package] version`) e escreva
   `packaging/release-notes/v<versão>.md`.
2. `git tag v<versão> && git push origin v<versão>`.

O workflow `Release` compila os quatro pacotes, escreve o `checksums.txt`,
assina com a chave do segredo `MUSICFAST_SIGNING_KEY` e publica a release.
Sem esse segredo, ou com outra chave, os apps instalados recusam a
atualização. A chave pública está em `crates/app/update-public-key.hex`; a
privada se gera com `cargo run -p release-sign -- keygen <pasta> <pública>`
e nunca entra no repositório.

## Licença

[MIT](LICENSE). Ícones do [Lucide](https://lucide.dev) (ISC); veja
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
