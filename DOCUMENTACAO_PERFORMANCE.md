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

Processo vivo (`toolsb-processo.ps1`, 3 rodadas alternadas, autorizado
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
