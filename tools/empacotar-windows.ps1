# Gera os dois pacotes do Windows em dist\, com os mesmos nomes da release:
#
#   musicfast-v<versão>-x86_64-pc-windows-msvc.zip        portátil
#   musicfast-v<versão>-x86_64-pc-windows-msvc-setup.exe  instalador
#
#   powershell -ExecutionPolicy Bypass -File tools\empacotar-windows.ps1
#
# O workflow Release faz o mesmo no GitHub; isto é para testar o instalador
# antes de uma tag. Precisa do Inno Setup 6 (winget install JRSoftware.InnoSetup).
# Não assina nem publica nada: sem o checksums.txt assinado da release, um
# pacote daqui não serve de atualização para ninguém.

param([switch]$SemCompilar)

$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz

$versao = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$nome = "musicfast-v$versao-x86_64-pc-windows-msvc"

if (-not $SemCompilar) {
    cargo build --release --locked -p musicfast
    if ($LASTEXITCODE) { throw "cargo build falhou" }
}
$exe = Join-Path $raiz 'target\release\musicfast.exe'
# O .exe é de janela (sem console): só pela pipe o PowerShell espera e lê a saída.
$sondagem = (& $exe --version | Out-String).Trim()
if ($sondagem -ne "musicfast $versao") { throw "--version respondeu '$sondagem', esperado 'musicfast $versao'" }

$dist = Join-Path $raiz 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null

# Portátil: o zip com a pasta dentro, o formato que o atualizador desempacota.
$pasta = Join-Path $dist $nome
if (Test-Path $pasta) { Remove-Item -Recurse -Force $pasta }
New-Item -ItemType Directory $pasta | Out-Null
Copy-Item $exe, packaging\musicfast-portable.txt, LICENSE, README.md $pasta
$zip = Join-Path $dist "$nome.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
# tar do Windows, não Compress-Archive: o do PowerShell 5 grava "\" nos
# caminhos, e o atualizador desempacota com tar esperando "/".
tar.exe -a -c -f $zip -C $dist $nome
if ($LASTEXITCODE) { throw "tar falhou" }
Remove-Item -Recurse -Force $pasta

# Instalador. O .ico é o que o build.rs gravou para embutir no .exe.
$iscc = @(
    (Get-Command iscc -ErrorAction SilentlyContinue).Source,
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 não encontrado (winget install JRSoftware.InnoSetup)" }

$ico = Get-ChildItem target\release\build\musicfast-*\out\musicfast.ico -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
$opcoes = @("/Q", "/DVersion=$versao", "/DSourceDir=$(Split-Path $exe)", "/DOutputDir=$dist")
if ($ico) { $opcoes += "/DIconFile=$($ico.FullName)" }
& $iscc @opcoes packaging\windows\musicfast.iss
if ($LASTEXITCODE) { throw "iscc falhou" }

Get-ChildItem $dist -Filter "$nome*" | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
