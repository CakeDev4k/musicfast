; O instalador do Windows (Inno Setup 6). Gera
; `musicfast-v<versão>-x86_64-pc-windows-msvc-setup.exe`, ao lado do zip
; portátil na mesma release.
;
;   iscc /DVersion=0.1.1 /DSourceDir=<pasta com o musicfast.exe> ^
;        /DIconFile=<musicfast.ico> /DOutputDir=dist packaging\windows\musicfast.iss
;
; Por que Inno Setup, e não MSI ou NSIS: o `fastframe-update` atualiza uma
; cópia instalada rodando o setup novo com `/VERYSILENT /DIR=<pasta atual>`,
; que são argumentos do Inno. Mudar de instalador quebra a atualização de
; quem já instalou.
;
; O que o atualizador espera desta instalação (README do fastframe-update,
; "The contract between versions") e não pode mudar sem transição:
;   - o `musicfast-installer.txt` com `musicfast-installer-v1` ao lado do
;     .exe: é o que faz o app se reconhecer como instalado (e não portátil);
;   - o .exe se chama `musicfast.exe`;
;   - a pasta padrão `%LOCALAPPDATA%\Programs\MusicFast`.

#ifndef Version
  #error Passe /DVersion=<versão do Cargo.toml>
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

[Setup]
; Fixo para sempre: é por ele que o Windows sabe que a versão nova substitui
; a antiga, em vez de aparecer duas vezes em "Aplicativos instalados".
AppId={{6E0C3B57-1F7A-4C52-9D6B-0C7E5A2F9B41}
AppName=MusicFast
AppVersion={#Version}
AppVerName=MusicFast {#Version}
AppPublisher=MusicFast
AppPublisherURL=https://github.com/CakeDev4k/musicfast
AppSupportURL=https://github.com/CakeDev4k/musicfast/issues
AppUpdatesURL=https://github.com/CakeDev4k/musicfast/releases/latest
VersionInfoVersion={#Version}
VersionInfoProductName=MusicFast
VersionInfoDescription=Instalador do MusicFast
; Por usuário, sem pedir administrador: o atualizador roda o setup sozinho,
; em silêncio, e um pedido de UAC no meio travaria a atualização.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=
DefaultDirName={localappdata}\Programs\MusicFast
DisableDirPage=auto
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
; Fecha o MusicFast aberto em vez de pedir reinício do Windows.
CloseApplications=force
RestartApplications=no
UninstallDisplayName=MusicFast
UninstallDisplayIcon={app}\musicfast.exe
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir={#OutputDir}
OutputBaseFilename=musicfast-v{#Version}-x86_64-pc-windows-msvc-setup
#ifdef IconFile
SetupIconFile={#IconFile}
#endif

[Languages]
Name: "pt"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\musicfast.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "musicfast-installer.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; Uma pasta que já foi portátil e passa a ser instalada: sem isto os dois
; marcadores ficariam juntos.
Type: files; Name: "{app}\musicfast-portable.txt"

[Icons]
Name: "{autoprograms}\MusicFast"; Filename: "{app}\musicfast.exe"
Name: "{autodesktop}\MusicFast"; Filename: "{app}\musicfast.exe"; Tasks: desktopicon

[Run]
; `skipifsilent`: na atualização quem reabre o app é o atualizador, com o
; recibo da atualização; abrir aqui também daria duas janelas.
Filename: "{app}\musicfast.exe"; Description: "{cm:LaunchProgram,MusicFast}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; As pastas que o atualizador deixa ao lado do .exe (`.musicfast-update-*`).
; Os dados da conta e a biblioteca ficam em %LOCALAPPDATA%\MusicFast, fora
; daqui, e sobrevivem à desinstalação de propósito.
Type: filesandordirs; Name: "{app}\.musicfast-update-*"
