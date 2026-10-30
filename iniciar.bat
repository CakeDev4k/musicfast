@echo off
rem Compila (se preciso) e abre o MusicFast em release.
rem O rust-toolchain.toml da raiz escolhe o Rust 1.98 sozinho.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo [MusicFast] Cargo nao encontrado. Instale o Rust em https://rustup.rs
    pause
    exit /b 1
)

echo [MusicFast] Compilando...
cargo build --release -p musicfast
if errorlevel 1 (
    echo [MusicFast] A compilacao falhou.
    pause
    exit /b 1
)

start "" "%~dp0target\release\musicfast.exe"
endlocal
