# A/B do processo vivo: abre o executável "antes" e o "depois", alternados,
# espera o início carregar e mede memória e CPU parado.
#
#     pwsh -File tools\ab-processo.ps1 -Antes <exe> -Depois <exe> [-Rodadas 3]
#
# Fecha o MusicFast que estiver aberto (com --sair, que grava a sessão). A
# sessão volta pausada, então nada toca. Mede o app parado no início: a
# memória inclui as capas que a rede entregou nesses segundos.
param(
    [Parameter(Mandatory)] [string]$Antes,
    [Parameter(Mandatory)] [string]$Depois,
    [int]$Rodadas = 3,
    [int]$Espera = 12,
    [int]$Janela = 10
)
$log = Join-Path $env:LOCALAPPDATA "MusicFast\musicfast.log"

# Pelo nome dos dois executáveis: a cópia "antes" costuma ter outro nome
# (musicfast-base.exe), e procurar só "musicfast" a deixava aberta segurando a
# vaga de instância única — as aberturas seguintes saíam na hora.
$nomes = @("musicfast", [IO.Path]::GetFileNameWithoutExtension($Antes), [IO.Path]::GetFileNameWithoutExtension($Depois)) | Select-Object -Unique
function Abertos { Get-Process -Name $nomes -ErrorAction SilentlyContinue }

function Fechar {
    if (-not (Abertos)) { return }
    & $Depois --sair | Out-Null
    for ($i = 0; $i -lt 50 -and (Abertos); $i++) { Start-Sleep -Milliseconds 200 }
    Abertos | Stop-Process -Force
}

$linhas = @()
for ($r = 1; $r -le $Rodadas; $r++) {
    foreach ($par in @(@("antes", $Antes), @("depois", $Depois))) {
        Fechar
        Start-Sleep -Seconds 1
        $p = Start-Process $par[1] -PassThru
        Start-Sleep -Seconds $Espera
        if ($p.HasExited) {
            # Outra cópia já estava aberta e recebeu o pedido (instância única).
            $linhas += [pscustomobject]@{ rodada = $r; exe = $par[0]; "cpu %" = "saiu: outra cópia aberta?" }
            continue
        }
        $p.Refresh()
        $cpu0 = $p.TotalProcessorTime.TotalMilliseconds; $t0 = Get-Date
        Start-Sleep -Seconds $Janela
        $p.Refresh()
        $cpu = ($p.TotalProcessorTime.TotalMilliseconds - $cpu0) / ((Get-Date) - $t0).TotalMilliseconds * 100
        $achado = Select-String -Path $log -Pattern "primeiro quadro em (\d+) ms" -ErrorAction SilentlyContinue | Select-Object -Last 1
        $quadro = if ($achado) { $achado.Matches[0].Groups[1].Value } else { "—" }
        $linhas += [pscustomobject]@{
            rodada = $r; exe = $par[0]
            "cpu %" = [math]::Round($cpu, 2)
            "trabalho MB" = [math]::Round($p.WorkingSet64 / 1MB)
            "privada MB" = [math]::Round($p.PrivateMemorySize64 / 1MB)
            threads = $p.Threads.Count
            "1º quadro ms" = $quadro
        }
    }
}
Fechar
$linhas | Format-Table -AutoSize | Out-String
