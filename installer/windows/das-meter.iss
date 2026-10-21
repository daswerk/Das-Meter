; Das-Meter for Windows: a machine-wide Inno Setup installer (x64, #46).
; It installs the app in Program Files and the Send Plugin as CLAP and VST3
; (a checkbox each, both on) into the standard Common Files folders, so DAWs
; find it, and opens .dasmeter-preset files with the app. Uninstalling keeps
; Presets, Themes and settings.toml unless asked to delete them.
;
;   iscc /DAppVersion=0.1.0 /DRoot=<repo> installer\windows\das-meter.iss
;
; Expects target\release\das-meter.exe and the bundles from
; scripts/bundle-send-plugin.sh in target\plugins. Signed by the release
; pipeline once signing is set up; until then SmartScreen asks once (More
; info, Run anyway).
;
; The app's Update runs it with /SILENT /SUPPRESSMSGBOXES /NORESTART
; /NOCLOSEAPPLICATIONS /RELAUNCH=1: a Send Plugin a DAW has open is replaced
; at the next restart (it's never closed under the user), and the app starts
; again afterwards.

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
UninstallDisplayIcon={app}\das-meter.exe
ChangesAssociations=yes
; Restart Manager: offers to close what has the app or the Send Plugin open
; (the plug-in binaries are .clap and .vst3 files).
CloseApplications=yes
CloseApplicationsFilter=*.exe,*.dll,*.clap,*.vst3
RestartApplications=no

[Tasks]
Name: "clap"; Description: "CLAP"; GroupDescription: "Install the Send Plugin as:"
Name: "vst3"; Description: "VST3"; GroupDescription: "Install the Send Plugin as:"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Root}\target\release\das-meter.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\THIRD-PARTY-LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion
; A plug-in a DAW has open is replaced at the next restart.
Source: "{#Root}\target\plugins\Das-Meter Send.clap"; DestDir: "{commoncf64}\CLAP"; Tasks: clap; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "{#Root}\target\plugins\Das-Meter Send.vst3\*"; DestDir: "{commoncf64}\VST3\Das-Meter Send.vst3"; Tasks: vst3; Flags: ignoreversion recursesubdirs restartreplace uninsrestartdelete

[InstallDelete]
; A format unticked on an update is taken away.
Type: files; Name: "{commoncf64}\CLAP\Das-Meter Send.clap"; Tasks: not clap
Type: filesandordirs; Name: "{commoncf64}\VST3\Das-Meter Send.vst3"; Tasks: not vst3

[Registry]
; .dasmeter-preset opens with the app, which imports it.
Root: HKA; Subkey: "Software\Classes\.dasmeter-preset"; ValueType: string; ValueName: ""; ValueData: "DasMeter.Preset"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.dasmeter-preset\OpenWithProgids"; ValueType: string; ValueName: "DasMeter.Preset"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\DasMeter.Preset"; ValueType: string; ValueName: ""; ValueData: "Das-Meter Preset"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\DasMeter.Preset\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\das-meter.exe,0"
Root: HKA; Subkey: "Software\Classes\DasMeter.Preset\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\das-meter.exe"" ""%1"""

[Icons]
Name: "{group}\Das-Meter"; Filename: "{app}\das-meter.exe"
Name: "{group}\Uninstall Das-Meter"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Das-Meter"; Filename: "{app}\das-meter.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\das-meter.exe"; Description: "{cm:LaunchProgram,Das-Meter}"; Flags: nowait postinstall skipifsilent
; After the app's own Update: start it again, as the user (not elevated).
Filename: "{app}\das-meter.exe"; Flags: nowait runasoriginaluser; Check: Relaunch

[Code]
var
  DeleteData: Boolean;

function Relaunch(): Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

{ Uninstall: the app and the plug-ins go; Presets, Themes and settings.toml
  stay unless "Also delete my Presets and Themes" is ticked. }
function InitializeUninstall(): Boolean;
var
  Form: TSetupForm;
  Text: TNewStaticText;
  Box: TNewCheckBox;
  Remove, Cancel: TNewButton;
begin
  Result := True;
  DeleteData := False;
  if UninstallSilent then
    Exit;
  Form := CreateCustomForm;
  try
    Form.ClientWidth := ScaleX(400);
    Form.ClientHeight := ScaleY(132);
    Form.Caption := 'Uninstall Das-Meter';
    Form.Position := poScreenCenter;

    Text := TNewStaticText.Create(Form);
    Text.Parent := Form;
    Text.Left := ScaleX(16);
    Text.Top := ScaleY(16);
    Text.Width := Form.ClientWidth - ScaleX(32);
    Text.AutoSize := False;
    Text.WordWrap := True;
    Text.Height := ScaleY(32);
    Text.Caption := 'Das-Meter and its Send Plugin will be removed. Your Presets, ' +
      'Themes and settings stay, for when you install it again.';

    Box := TNewCheckBox.Create(Form);
    Box.Parent := Form;
    Box.Left := ScaleX(16);
    Box.Top := ScaleY(56);
    Box.Width := Form.ClientWidth - ScaleX(32);
    Box.Height := ScaleY(20);
    Box.Caption := 'Also delete my Presets and Themes';
    Box.Checked := False;

    Remove := TNewButton.Create(Form);
    Remove.Parent := Form;
    Remove.Width := ScaleX(88);
    Remove.Height := ScaleY(26);
    Remove.Left := Form.ClientWidth - ScaleX(16 + 88 + 8 + 88);
    Remove.Top := Form.ClientHeight - ScaleY(16 + 26);
    Remove.Caption := 'Uninstall';
    Remove.ModalResult := mrOk;
    Remove.Default := True;

    Cancel := TNewButton.Create(Form);
    Cancel.Parent := Form;
    Cancel.Width := ScaleX(88);
    Cancel.Height := ScaleY(26);
    Cancel.Left := Form.ClientWidth - ScaleX(16 + 88);
    Cancel.Top := Remove.Top;
    Cancel.Caption := 'Cancel';
    Cancel.ModalResult := mrCancel;
    Cancel.Cancel := True;

    Result := Form.ShowModal() = mrOk;
    DeleteData := Box.Checked;
  finally
    Form.Free();
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and DeleteData then
    DelTree(ExpandConstant('{userappdata}\Das-Meter'), True, True, True);
end;
