# MusicFast — desempenho

Registro das passadas de otimização e estabilidade, no formato do
`OTIMIZACAO.md` §11 (THRESHOLD HUB). Documento interno.

## Instrumentos

| Instrumento | O que mede | O que não mede |
|---|---|---|
| `musicfast --medir` (com a saída redirecionada: `musicfast.exe --medir \| Out-String`) | CPU de cada quadro das telas de verdade (`page::show`, `side::queue_list`) num contexto egui sem janela, rolando com a roda: construção (`run_ui`) e tesselação, p50/p95/máx em 150 quadros depois de 10 de aquecimento; o tempo das fontes e do primeiro quadro; e os vértices do último quadro, que provam que a tela desenhou (recusa medir abaixo de 2.000) | GPU, driver, espera do vsync; download e decodificação das capas (elas entram já como textura); a barra do player e a coluna da esquerda |
| `pwsh -File tools\medir-processo.ps1 [-Segundos 20]` | o processo que está rodando: CPU no intervalo (% de um núcleo, todas as threads), conjunto de trabalho e pico, memória privada, threads, handles | a memória de vídeo das texturas (fica no driver); o que o app está fazendo no momento — anote a tela e se está tocando |
| `cargo test -p musicfast` | `covers::fit_square` (capa reduzida, tarjas cortadas, capa pequena não ampliada) | — |

O executável de antes de cada passada fica em `target\medir\base\` para as
rodadas alternadas.

## 1. Primeira passada — listas grandes, capas e pânicos (29/10/2026)

**Sintoma.** Pedido do dono: "otimizar esse aplicativo o mais possível e
deixar ele o mais estável possível". Sem queixa de uso concreta; a passada
parte da medida.

**Base.** Esta máquina (Windows 11), release com `lto = "thin"`,
`codegen-units = 1`, outra sessão compilando em paralelo. `--medir` com tela
de 1020×650, catálogos sintéticos (títulos longos, 2 artistas, álbum, capa).
Processo vivo: o app aberto pelo dono, na build anterior a esta passada.

### 1.1 Cada linha copiava a playlist inteira, a cada quadro

`ui/page.rs`, `shelf_block`: para cada linha de música (inclusive as fora da
tela) montava o comando de tocar, e o comando de uma playlist era
`PlayTracks { tracks: tracks.clone(), index }` — a lista inteira copiada por
linha, por quadro: n² cópias de `Track` (com as `String` de título, artistas
e álbum). O cabeçalho ainda copiava todas as músicas a cada quadro para saber
se havia alguma. Agora `track_row` devolve `true` no clique e o comando é
montado só então (`shelf_tracks`); o cabeçalho pergunta
`page.tracks().next().is_some()` e copia no clique.

| `--medir` (p50 / p95 / máx, ms) | Antes | Depois |
|---|---|---|
| playlist 2.000 músicas | 1.634,8 / 1.848,0 / 2.100,5 | 0,35 / 0,49–0,74 / 0,69–1,05 |
| playlist 200 músicas | 17,3 / 22,1 / 24,8 | 0,06 / 0,07–0,09 / 0,10–0,31 |

Uma playlist de 2.000 músicas (as "Músicas curtidas" de muita gente) travava
o app: 1,6 s por quadro, e cada movimento do mouse pede um quadro. Os outros
cenários (início, grade, fila) não passam por esse código e ficaram dentro da
variação.

### 1.2 Pânico em thread de trabalho fechava o app

`Cargo.toml`: `panic = "abort"` no release. Tudo o que mais pode quebrar — o
leitor do JSON do YouTube quando o site muda a forma de uma resposta, o
decodificador de imagem com uma capa malformada — roda em threads de
trabalho, e um pânico em qualquer uma derrubava o processo. Agora
`panic = "unwind"`; `jobs::spawn_or` roda cada tarefa em `catch_unwind` e, se
ela entrar em pânico, manda uma resposta de erro no lugar (a tela que esperava
a página mostra "Não deu para carregar" com "Tentar de novo", em vez de girar
para sempre — §5.6 do guia); `covers::worker` pega o pânico por capa, e a
thread segue para a próxima. O endereço do pânico continua indo para o
`panicos.log` (o gancho do `fastframe-log`).

### 1.3 Capas no tamanho do arquivo, e a que falhava nunca voltava

`covers.rs`: a textura era o arquivo inteiro. As capas do
`googleusercontent` já vêm no tamanho pedido, mas a miniatura de vídeo do
`i.ytimg.com` não aceita tamanho no endereço e chegava com até 1280×720 —
3,6 MB de textura para um quadrado de 40 px, esticada de 16:9 para 1:1. E a
capa que falhava uma vez (sem rede) ficava em branco até sair do cache.

Agora o trabalhador corta o centro e reduz para o degrau em que a capa é
desenhada (`fit_square`); a miniatura 4:3 (`hqdefault`) perde as tarjas
pretas antes do corte. A chave do cache leva o degrau quando o endereço não
muda com o tamanho, para a capa grande do "Tocando agora" não reaproveitar a
de 60 px. Capa que falha é pedida de novo depois de 30 s — não a cada quadro:
o pedido falho pede um quadro, e sem o prazo isso virava um laço a 60 Hz sem
rede.

### 1.4 Os controles de mídia remontavam o estado a cada quadro

`app.rs`, `media_controls`: rodava em todo `logic`, e todo quadro montava um
`np::State` novo — cópias do título, dos artistas, do álbum, o endereço da
capa e o da música — para o `NowPlaying::update` descobrir que nada tinha
mudado. Numa rolagem são 60 por segundo. Agora os comandos do Windows (teclas
de mídia) continuam lidos a cada quadro, e o estado só é remontado quando a
revisão do `Snapshot` muda, quando a capa chega ao disco, num salto de posição
ou uma vez por segundo (o próprio `update` já só repassava a posição uma vez
por segundo). A faxina da pasta de capas (`clean_art`, ler e apagar arquivos)
saiu da thread da janela e vai junto com o download.

Sem medida em milissegundo: não passa pelo `--medir`. É contagem: de 60
montagens por segundo numa rolagem para no máximo 1 (mais as de mudança).

### 1.5 Gravar a sessão copiava e serializava a fila na thread da janela

`app.rs`, `autosave` (a cada 15 s): montava a sessão inteira (cópia da fila),
comparava com a última gravada (comparação da fila inteira) e serializava na
thread da janela; só a escrita no disco ia para outra thread. Tocando, a
posição muda, então isso rodava a cada 15 s.

| `--medir`, fila de 2.000 músicas | Antes (na janela) | Depois |
|---|---|---|
| copiar + comparar + serializar | p50 2,1–2,7 ms, máx 3,9–5,4 ms, 585 kB | na thread `estado`; na janela só a comparação dos campos pequenos e `Arc::ptr_eq` da fila |

`Saved::save_in_background` recebe a fila por `Arc` e copia e serializa lá; a
última sessão gravada fica sem a fila (`saved` + `saved_queue` por ponteiro).
Duas gravações não disputam mais o mesmo `.tmp` (trava `WRITING`, §6.4 do
guia).

### 1.6 As fontes do sistema seguravam o primeiro quadro

`main.rs`: o `FontSetup::default()` lê, antes de a janela pintar, uma fonte
instalada por escrita que a Inter não cobre (CJK, árabe, hebraico, índicas…).
São necessárias — o dono ouve músicas com título em japonês ("蕾に雷") —, mas
não para o primeiro quadro.

| `--medir` (3 rodadas) | Antes | Depois |
|---|---|---|
| fontes antes do 1º quadro | 82–118 ms (Inter + fallbacks) | 0,0 ms (só a Inter) |
| fallbacks | — | ~105 ms, na thread `fontes`, valem no quadro seguinte (`ctx.set_fonts` é seguro de outra thread) |

O log agora registra "primeiro quadro em N ms desde o início do processo".
Primeira leitura com a build nova: 229 ms (inclui criar a janela e o
contexto OpenGL, que nenhuma das duas builds controla).

Cuidado que falha em silêncio: um título em japonês que aparece no primeiro
quadro sai em caixinhas por ~0,1 s até as fontes chegarem.

### 1.7 Mapas que só cresciam

`app.rs`: `info`, `lyrics` e `related` ganhavam uma entrada por música tocada
e nunca encolhiam — um dia inteiro de rádio são centenas, e `related` guarda
uma página inteira por música. Agora, ao passar de 300 (`info`, `lyrics`) ou
12 (`related`), ficam só os da música atual (lei 8 do guia).

### Medidas finais

`--medir` na build final (com as mudanças das outras sessões desta noite no
mesmo código), máquina com Premiere, After Effects, Discord e outra sessão
compilando:

| Cenário (p50 / p95 / máx, ms) | Antes (2 rodadas) | Depois (4 rodadas) |
|---|---|---|
| playlist 2.000 músicas | 1.635–2.220 / 1.848–8.964 / 2.100–11.144 | 0,35–0,52 / 0,49–0,89 / 0,69–1,52 |
| playlist 200 músicas | 17–22 / 22–44 / 25–136 | 0,06–0,08 / 0,07–0,13 / 0,10–0,57 |
| início, grade, fila | 0,04–0,16 / 0,07–0,23 | 0,04–0,11 / 0,05–0,20 |

Os três últimos cenários não passam pelo código mudado; a diferença está na
variação da máquina e não é ganho desta passada.

Processo vivo (`tools\ab-processo.ps1`, 3 rodadas alternadas, autorizado
pelo dono): o app parado no início, sessão restaurada e pausada, medido de
12 a 22 s depois de abrir.

| Rodada | Exe | CPU (% de um núcleo) | Trabalho MB | Privada MB | Threads | 1º quadro |
|---|---|---|---|---|---|---|
| 1 | antes | 1,87 | 137 | 138 | 32 | (não registrava) |
| 1 | depois | 0,16 | 137 | 140 | 32 | 210 ms |
| 2 | antes | 0,00 | 138 | 138 | 32 | — |
| 2 | depois | 0,00 | 137 | 138 | 32 | 204 ms |
| 3 | antes | 0,00 | 138 | 139 | 31 | — |
| 3 | depois | 0,16 | 137 | 122 | 31 | 222 ms |

Os números só sustentam "não ficou pior": parado no início, as duas builds
gastam o mesmo. A memória não mudou porque as capas do início vêm do
`googleusercontent`, que já as servia no tamanho pedido; a redução de §1.3 só
age nas miniaturas de vídeo (`i.ytimg.com`: busca de vídeos, fila com vídeos),
e esse cenário não foi medido ao vivo.

**Cobertura.**
- `bench::tests::big_playlist_frame_is_not_quadratic`: quadro da playlist de
  2.000 abaixo de 500 ms no build de teste. Mutante (montar o comando por
  linha de novo): 12.096 ms, reprova.
- `covers::tests` (3): miniatura 1280×720 vira 60×60, tarjas do `hqdefault`
  saem, capa pequena não é ampliada. Mutante (sem a redução): reprova
  `video_thumbnail_becomes_a_small_square`.
- `--medir` recusa medir um quadro com menos de 2.000 vértices.

**Descartado.**
- *Tirar as fontes do sistema* (como o Produtividade Prime faz por padrão,
  33 MB): o dono ouve títulos em japonês; viraram carga em segundo plano
  (§1.6) em vez de saírem.
- *Virtualizar a lista de músicas com `show_rows`*: com o comando montado no
  clique, as 2.000 linhas fora da tela custam ~0,4 ms por quadro (só reservam
  espaço). As prateleiras misturam linhas de alturas diferentes; o ganho não
  paga a complexidade.

**Não conferido.**
- **A memória com miniaturas de vídeo** (busca "Vídeos", fila de rádio com
  vídeos): a redução de §1.3 é conta (uma miniatura 1280×720: 3,6 MB → 14 kB
  de textura a 60 px), não medida ao vivo.
- **Os 40–47 % de CPU** que a build anterior mostrou numa primeira tentativa
  do A/B, nos ~20 s depois de abrir, **não se repetiram** nas 3 rodadas
  limpas (0–1,9 %). Aquela tentativa tinha outra cópia aberta e a sessão
  restaurada carregando música; a hipótese do círculo do botão tocar pedir um
  quadro a cada quadro (`request_repaint()` sem prazo em `bar.rs`, enquanto o
  player está em "carregando") continua sem medida. Não foi mexido.
- Pânico de verdade em thread de trabalho: o caminho (`catch_unwind` →
  resposta de erro) não foi provocado no app rodando.
- O primeiro quadro (204–229 ms) não tem base ao vivo: a build anterior não
  registrava. O que caiu é medido pelo `--medir` (fontes antes do 1º quadro:
  82–118 ms → 0).

**Instrumento que mentia.** A primeira versão do `ab-processo.ps1` fechava as
cópias pelo nome `musicfast`, e o executável de base se chama
`musicfast-base.exe`: ele ficava aberto segurando a vaga de instância única,
e cada abertura seguinte entregava o pedido a ele e saía na hora — a tabela
mostrava "saiu" em vez de números. Agora o script fecha pelos dois nomes e
marca a abertura que saiu, em vez de medir zero.

**Lição.** "Rápido porque não fez nada" vale também para quem lê a medida: o
`--medir` imprime os vértices e recusa um quadro vazio, e foi assim que a
queda de 1,6 s para 0,4 ms pôde ser aceita.

## 2. Segunda passada — quadros, memória das capas, do player e dos fundos (30/10/2026)

**Sintoma.** Pedido do dono: o app "cresceu bastante" (letra, visualizador,
fundos personalizados) e "pode estar ficando mais pesado do que deveria".
A passada leu o sistema inteiro atrás do que roda a cada quadro sem
precisar e do que guarda memória sem teto.

**O que já estava certo e ficou.** O fork do eframe não desenha com a
janela minimizada ou coberta (só roda `logic`, no máximo 10 vezes por
segundo), então nada aqui trata disso. O callback de áudio não trava nem
aloca. A letra, as traduções e a faxina de cache rodam em threads.

### 2.1 Quadros na taxa do monitor sem precisar

- `ui/bar.rs`: o arco de "carregando" pedia `request_repaint()` sem prazo
  — a janela inteira refeita na taxa do monitor durante toda a resolução
  do yt-dlp (3 s numa música; mais de um minuto na primeira instalação). É
  a hipótese dos 40–47 % de CPU que §1 deixou "não conferido". Agora ~30
  quadros por segundo, como o arco do mini player já fazia.
- `app.rs` e `look::preview`: o visualizador também pedia quadro sem
  prazo enquanto toca. Agora `viz::FRAME` (33 ms, o ritmo da letra); a
  queda das faixas já usava o tempo real, então o movimento é o mesmo.

| Com o visualizador ligado, tocando | Antes | Depois |
|---|---|---|
| monitor de 60 Hz | 60 quadros/s | 30 (−50 %) |
| monitor de 144 Hz | 144 quadros/s | 30 (−79 %) |

Cada quadro refaz a interface inteira (o egui não redesenha só um pedaço);
o `--medir` mede o custo de um quadro, não quantos são pedidos. No processo
vivo, tocando com o visualizador do dono: 92,7 → 14,6 % de um núcleo
(mediana; "Medidas", abaixo).

### 2.2 Capas: sem alocar por quadro, e com teto em bytes

`covers.rs`: cada capa desenhada montava, a cada quadro, duas `String` (o
endereço com o tamanho e a chave do cache). A chave agora é um hash da
origem (`Thumbs::source_for`, sem montar o endereço) e do degrau; o
endereço só é montado para a capa que falta. O teto era 500 texturas — que
numa tela de alta densidade (grade no degrau de 360 px, 518 kB cada)
passavam de 250 MB de memória de vídeo; agora é `TEXTURE_BUDGET` (64 MB),
e as que saem nunca são as desenhadas no último quadro.

**Instrumento que mentia.** O `preload` do `--medir` gravava a capa com o
endereço puro, mas a tela procurava `endereço#degrau` (a chave mudou em
§1.3 e o `preload` não acompanhou): o `--medir` desenhava os quadradinhos
de espera, não as capas. Agora o `preload` grava na mesma chave que a tela
procura, em todos os degraus. Os números de §1 continuam valendo para o
que mediam (o custo das linhas), mas não incluíam desenhar as capas.

### 2.3 A coluna das playlists

`ui/nav.rs`: a cada quadro, cada playlist da conta virava três `String`
copiadas e uma linha montada, inclusive as fora da tela. Agora as linhas
emprestam os nomes e só as visíveis são montadas (`show_rows`).

### 2.4 O player: uma cópia da fila a menos, e cache com teto em bytes

- `engine.rs`: a fila é um `Arc<Vec<Track>>` dividido com o `Snapshot` e
  muda por `Arc::make_mut`. Antes cada publicação copiava a fila inteira
  (o app guardava sempre duas cópias dela) e comparava 2.000 ids a cada
  mensagem; agora a comparação é por ponteiro. Bônus: a comparação por id
  deixava passar a mesma música com dados novos (a semente do rádio sem
  artistas, depois com eles); por ponteiro, a tela recebe.
- `keep_bytes`/`trim_kept`: guardava as 4 últimas músicas inteiras sem olhar
  o tamanho — quatro mixes de 1 h passavam de 230 MB. Agora também até
  `BYTES_BUDGET` (48 MB); a atual e a próxima nunca saem. Quatro músicas
  comuns (16 MB, ou 32 MB a 256 kbps) continuam todas.
- `source.rs`: o download virava `Arc<[u8]>`, conversão que copia a música
  inteira (dobro do pico de memória no fim do download). Agora
  `Audio(Arc<Vec<u8>>)`, sem cópia.

### 2.5 Fundos personalizados

`look.rs`:
- A imagem desfocada encolhe antes do desfoque (até sobrar 4 px de
  desfoque) e a placa de vídeo a amplia de volta, lisa. No desfoque
  máximo, a foto de 1280×720 vira 160×90 (textura de 3,7 MB → 58 kB), e o
  `fast_blur` roda em 64 vezes menos pixels; num GIF, por quadro.
- GIF: o orçamento por superfície caiu de 160 MB para 64 MB, e o GIF que
  passa dele encolhe todos os quadros em vez de perder o fim da animação
  (abaixo de 240 px, corta como antes). Um GIF comum de fundo (498×280, 60
  quadros, 33 MB) não muda.

### 2.6 Pequenos, por quadro

- `ytm::parse::thumbs` (`Thumbs::compact`): das capas que aceitam o tamanho
  no endereço guarda só a maior (`url_for` só usa ela). Menos endereços em
  cada página, na fila e no `estado.json`; a fila gravada antes é compactada
  ao abrir (`Saved::load`).
- `app.rs`, `track_wanted`: duas `String` por quadro (o idioma e a marca da
  tradução da letra), com ou sem letra na tela; agora só quando a tradução
  vai mesmo ser conferida.
- `look::ensure_lyrics_fonts` sai na hora sem fonte escolhida e não copia
  os nomes; `fonts::drop_previews_when_closed` sai na hora sem prévias
  carregadas.

### Medidas

`--medir` na build desta passada (release em `target\medir`, com o
trabalho em andamento da letra no mesmo código):

| Cenário (p50 / p95 / máx, ms) | Depois |
|---|---|
| playlist 2.000 músicas | 0,44 / 0,60 / 1,02 |
| playlist 200 músicas | 0,06 / 0,07 / 0,09 |
| início (10 carrosséis) | 0,10 / 0,17 / 0,46 |
| biblioteca (grade 300) | 0,08 / 0,10 / 0,19 |
| fila 2.000 músicas | 0,03 / 0,06 / 0,14 |

Sem coluna "antes": a base desenhava os quadradinhos no lugar das capas
(§2.2), então não mede a mesma coisa.

Processo vivo, A/B alternado (um script derivado de `tools\ab-processo.ps1`,
na pasta de rascunho da sessão). Pasta de dados isolada (`LOCALAPPDATA`
trocado só para o teste): a fila e a aparência do dono (visualizador "seda"
na barra), volume zero — o visualizador lê o som antes do volume — e sem a
conta. "Antes" é o `target\release\musicfast.exe` que o dono usava
(compilado às 14:34, antes desta passada); "depois", esta build. Janela
maximizada (1920×1009), monitor de 60 Hz. Cada abertura: 12 s para o início
carregar, 10 s parado, `--tocar-pausar`, 8 s, 15 s tocando; toda rodada
abaixo tocou (a linha "saída de áudio" no log).

| 5 rodadas válidas de cada | Parado, CPU | Tocando, CPU (% de um núcleo) | Trabalho tocando |
|---|---|---|---|
| antes | 0–6,8 % | 77,8 / 87,4 / 92,7 / 93,3 / 97,1 | 151–217 MB |
| depois | 0–0,2 % | 13,0 / 13,2 / 14,6 / 15,2 / 15,3 | 151–211 MB |

Duas rodadas saíram da conta, uma de cada: "antes" com 0,9 % e 8 MB de
trabalho, "depois" com 0,4 % — a janela minimizada ou coberta no meio da
medida (minimizada, o Windows esvazia o conjunto de trabalho e o fork do
eframe não desenha).

A queda (6×) passa da conta de quadros (60 → 30, 2×). Hipótese, não
conferida (o processo vivo não conta quadros): o pedido sem prazo não ficava
preso ao vsync nesta máquina — notebook com duas placas, RTX 5050 e Radeon
740M — e a janela redesenhava bem acima de 60 por segundo. Com
`request_repaint_after` o ritmo é o do relógio, com ou sem vsync.

A memória não muda neste roteiro (uma música de 8,6 MB e a página inicial):
os tetos de §2.2–2.5 agem em mixes longos, GIFs e grades grandes, que ele não
abre.

**Cobertura.**
- `covers::tests::over_the_budget_the_oldest_go_and_the_visible_stay` e
  `the_same_cover_has_one_key_per_bucket`.
- `engine::tests::common_songs_keep_four_and_long_mixes_keep_two`.
- `look::tests::a_gif_over_the_budget_shrinks_and_keeps_every_frame` e
  `a_blurred_backdrop_is_blurred_small`.
- `ytm` `compact_thumbs_ask_for_the_same_urls`.

**Não conferido.**
- **A memória de §2.2–2.5 ao vivo**: é conta dos tetos; o roteiro A/B não
  abre mix longo, GIF nem grade grande.
- O arco de "carregando" (§2.1) sem medida própria: dura segundos por
  música, curto demais para a janela de 15 s do A/B.
- O visual do desfoque reduzido e do GIF encolhido não foi olhado na tela.

**Descartado.**
- *Guardar o layout da letra entre quadros*: cada quadro monta um
  `LayoutJob` por linha (~0,2 ms com a letra aberta), mas o egui já guarda
  as galleys; uma cópia própria arriscaria a letra velha na tela quando a
  tradução chega ou as fontes trocam.
- *As fontes do sistema* (~33 MB, CJK e afins): o `fastframe-fonts` lê os
  arquivos inteiros para a memória. Mapear (`memmap2`, que ele já usa para
  procurar) é mudança no fastframe, não aqui.

## 3. Terceira passada — o player que travava sem voltar (30/10/2026)

**Sintoma.** Achado lendo o código, depois da §2: dois jeitos de o player
parar para sempre sem dizer nada.

### 3.1 O yt-dlp sem prazo

`job.rs`: `output` esperava o yt-dlp com `wait_with_output`, sem prazo, e o
yt-dlp não recebia `--socket-timeout`. Uma rede engasgada ou um desafio de
JavaScript que não termina deixava a música em "Buscando o áudio" para
sempre — e marcada em `resolving`, então nunca era pedida de novo.

Agora `output(cmd, limit)` lê a saída em paralelo e confere o filho a cada
20 ms; passado `RESOLVE_LIMIT` (60 s; uma resolução normal leva 1–4 s), volta
`ErrorKind::TimedOut` e a tela mostra "o yt-dlp demorou demais e foi
encerrado". O yt-dlp ganhou `--socket-timeout 15`.

Encerrar só o yt-dlp não bastava: ele abre o Deno, e a versão de arquivo
único (a do winget) é um lançador que abre o Python. Cada chamada ganha um
job do Windows só dela, aninhado no do app (Windows 8 em diante), e o prazo
encerra o job inteiro (`TerminateJobObject`); fechado no fim da chamada, ele
leva junto o neto que tenha sobrado (`KILL_ON_JOB_CLOSE`). Sem jobs
aninhados, só o filho sai, como antes.

### 3.2 As threads do player sem proteção contra pânico

§1.2 protegeu as threads da janela (`jobs::spawn_or`); as do player
ficaram de fora.

- Resolver, baixar, montar a fila, rádio, autoplay, instalar e manter o
  yt-dlp: um pânico matava a thread sem resposta, e o motor esperava para
  sempre (`resolving`, `fetching`, `installing` e `more_inflight` ligados:
  a música parada, o rádio sem crescer até reabrir o app). Agora
  `engine::answering` roda cada uma em `catch_unwind` e, no pânico, manda a
  resposta de erro montada antes.
- A thread do motor: um pânico (um índice fora da fila, por exemplo) a
  matava calada — a tela seguia, nada mais tocava, e os comandos caíam no
  vazio. Agora cada volta do laço (`Engine::turn`) é protegida; no pânico,
  `recover` para o que carregava, esquece os pedidos em andamento (podem ser
  refeitos) e publica o estado. A música que já toca segue (está no
  callback). Depois de 20 pânicos (`MAX_PANICS`) o laço desiste, em vez de
  virar um laço de pânicos.

**Cobertura.**
- `job::tests::a_stuck_child_is_ended_at_the_deadline`: `cmd /C ping` de
  30 s, prazo de 0,5 s, volta `TimedOut` em ~0,6 s. Conferido à mão que o
  `ping` (o neto) não sobra rodando.
- `job::tests::a_quick_child_answers_with_its_output`.
- `engine::tests::a_worker_that_panics_still_answers`.
- `engine::tests::a_panic_in_a_message_leaves_the_player_answering`: um
  índice fora da fila faz `prepare_next` entrar em pânico; a volta devolve
  `false`, o que carregava é esquecido e a mensagem seguinte (volume) chega
  ao `Snapshot`. Sem a proteção, o pânico escapa e derruba o próprio teste.
- `engine::tests::what_was_loading_stops_after_a_panic`.

**Não conferido.**
- Um yt-dlp travado de verdade no app rodando: o prazo foi provado com o
  `ping`, não com uma rede engasgada.
- As mensagens novas do yt-dlp e dos pânicos são em português direto, como
  as outras do `ytdlp.rs`; as da fila e do rádio usam `ytm_internal_error`,
  que já existe nos três idiomas.
