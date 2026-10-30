# Mede o MusicFast que está rodando: memória e CPU num intervalo.
#
#     pwsh -File tools\medir-processo.ps1 [-Segundos 20]
#
# (pwsh, o PowerShell 7: o 5.1 lê este arquivo UTF-8 sem BOM como ANSI.)
#
# CPU é o tempo de processador gasto no intervalo (todas as threads: janela,
# player, capas), em % de UM núcleo. Memória: conjunto de trabalho (o que o
# Gerenciador de Tarefas mostra) e privada (o que é só do app). Não mede a
# memória de vídeo das texturas, que fica no driver.
param([int]$Segundos = 20)

$p = Get-Process musicfast -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { Write-Output "o MusicFast não está rodando"; exit 1 }
$cpu0 = $p.TotalProcessorTime.TotalMilliseconds
$t0 = Get-Date
$picoWs = 0
for ($i = 0; $i -lt $Segundos; $i++) {
    Start-Sleep -Seconds 1
    $p.Refresh()
    if ($p.WorkingSet64 -gt $picoWs) { $picoWs = $p.WorkingSet64 }
}
$p.Refresh()
$cpu = ($p.TotalProcessorTime.TotalMilliseconds - $cpu0) / ((Get-Date) - $t0).TotalMilliseconds * 100
"{0,-28} {1}" -f "janela", $p.MainWindowTitle
"{0,-28} {1:N1} %" -f "CPU (% de um núcleo)", $cpu
"{0,-28} {1:N0} MB (pico {2:N0} MB)" -f "conjunto de trabalho", ($p.WorkingSet64 / 1MB), ($picoWs / 1MB)
"{0,-28} {1:N0} MB" -f "memória privada", ($p.PrivateMemorySize64 / 1MB)
"{0,-28} {1}" -f "threads", $p.Threads.Count
"{0,-28} {1}" -f "handles", $p.HandleCount
