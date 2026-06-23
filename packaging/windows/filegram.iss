; Inno Setup script for the Filegram Windows installer.
; Compiled in CI (build.yml) for x64 and x86; shipped alongside the portable
; zip so winget can use the proper installer manifest while scoop keeps using
; the zip. Defines are provided by ISCC via /D flags. All file paths must be
; absolute because ISCC resolves relative paths against the .iss directory,
; not the workflow CWD.
;   AppVersion     numeric X.Y.Z (used for VersionInfoVersion / Apps & Features)
;   AppFullVersion same string as Cargo.toml (may carry a -dev suffix)
;   AppArch        x64 | x86
;   SourceExe      absolute path to the already-built filegram.exe
;   IconFile       absolute path to assets/icon/filegram.ico
;   OutputDir      absolute path where the installer .exe is dropped
;   OutputName     installer base name without extension

#ifndef AppVersion
  #error AppVersion must be provided via /DAppVersion=X.Y.Z
#endif
#ifndef AppFullVersion
  #define AppFullVersion AppVersion
#endif
#ifndef AppArch
  #error AppArch must be provided via /DAppArch=x64|x86
#endif
#ifndef SourceExe
  #error SourceExe must be provided via /DSourceExe=<absolute path>
#endif
#ifndef IconFile
  #error IconFile must be provided via /DIconFile=<absolute path>
#endif
#ifndef OutputDir
  #error OutputDir must be provided via /DOutputDir=<absolute path>
#endif
#ifndef OutputName
  #define OutputName "filegram-windows-" + AppArch + "-setup"
#endif

#define MyAppName "Filegram"
#define MyAppExeName "filegram.exe"
#define MyAppPublisher "Filegram"
#define MyAppURL "https://github.com/filegram/filegram-desktop"

[Setup]
; Stable AppId — never change it; upgrades match by this GUID.
AppId={{7F4D2B8C-9A3E-4F1B-B8D5-2C6E1A9F4D73}
AppName={#MyAppName}
AppVersion={#AppFullVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
DisableReadyPage=yes
DisableFinishedPage=yes
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; Default to per-user install (no UAC prompt); user may elevate via the dialog.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
SetupIconFile={#IconFile}
#if AppArch == "x64"
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"

[Registry]
; App Paths lets Windows resolve "filegram" from Run / Start search.
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#MyAppExeName}"; \
  ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#MyAppExeName}"; \
  ValueType: string; ValueName: "Path"; ValueData: "{app}"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; \
  Flags: nowait postinstall skipifsilent
