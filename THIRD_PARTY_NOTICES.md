# Avisos de terceiros

O MusicFast é distribuído sob a licença MIT (`LICENSE`). Ele inclui ou usa
o seguinte trabalho de terceiros.

## Incluído no app

### Lucide (ícones)

Os traçados dos ícones em `crates/app/src/ui/icon_data.rs` vêm do
[Lucide](https://lucide.dev), achatados por `tools/gerar-icones.py`.

```
ISC License

Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part
of Feather (MIT). All other copyright (c) for Lucide are held by Lucide
Contributors 2022.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
```

### fastframe

As bibliotecas `fastframe-*` (fontes, texto, log, instância única, áudio,
controles de mídia, rolagem e atualização) são de
[crmne/fastframe](https://github.com/crmne/fastframe), licença MIT,
Copyright (c) 2026 Carmine Paolino. A fonte Inter embutida por
`fastframe-fonts` é distribuída sob a SIL Open Font License 1.1.

### Crates do Rust

As demais dependências (egui, eframe, winit, reqwest, symphonia, wry, tao,
serde, ring, keyring, image, rfd e outras, listadas no `Cargo.lock`) têm licenças MIT,
Apache-2.0 ou compatíveis; o texto de cada uma acompanha o código-fonte do
crate.

## Baixado pelo app na primeira vez

Estes programas não vêm no pacote do MusicFast. O app baixa as releases
oficiais deles do GitHub e confere o SHA-256 publicado antes de usar:

- [yt-dlp](https://github.com/yt-dlp/yt-dlp): Unlicense (domínio público).
  As versões empacotadas incluem componentes com outras licenças, listados
  em `THIRD_PARTY_LICENSES.txt` dentro da pasta dele.
- [Deno](https://github.com/denoland/deno): licença MIT.
