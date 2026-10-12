; Das-Meter for Windows: an Inno Setup script for a test installer (x64).
; It installs the app, and the Send Plugin as CLAP and VST3 into the
; standard Common Files folders, so DAWs find it.
;
;   iscc /DAppVersion=0.1.0 /DRoot=<repo> installer\windows\das-meter.iss
;
; Expects target\release\das-meter.exe and the bundles from
; scripts/bundle-send-plugin.sh in target\plugins. Not code-signed yet:
; Windows SmartScreen asks once (More info, Run anyway).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Root
  #define Root "..\.."
#endif

[Setup]
AppId={{6F1B7C2E-5D4A-4B8E-9C3F-2A7D1E0B9C44}
AppName=Das-Meter
AppVersion={#AppVersion}
AppPublisher=Das Werk
AppPublisherURL=https://github.com/daswerk/Das-Meter
DefaultDirName={autopf}\Das-Meter
DefaultGroupName=Das-Meter
DisableProgramGroupPage=yes
LicenseFile={#Root}\LICENSE-MIT
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
OutputDir={#Root}\target\installer
OutputBaseFilename=Das-Meter-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=Das-Meter

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Root}\target\release\das-meter.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\target\plugins\Das-Meter Send.clap"; DestDir: "{commoncf64}\CLAP"; Flags: ignoreversion
Source: "{#Root}\target\plugins\Das-Meter Send.vst3\*"; DestDir: "{commoncf64}\VST3\Das-Meter Send.vst3"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\Das-Meter"; Filename: "{app}\das-meter.exe"
Name: "{group}\Uninstall Das-Meter"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Das-Meter"; Filename: "{app}\das-meter.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\das-meter.exe"; Description: "{cm:LaunchProgram,Das-Meter}"; Flags: nowait postinstall skipifsilent
