; SeePass - Inno Setup script.
; One file installs the app the first time and updates it later:
;   * first run: asks for the install folder (the app AND your encrypted vault and settings go there)
;   * later runs: reuse the remembered folder without asking (running a newer setup is the update)
; Build: iscc /DAppVersion=X.Y.Z /DSourceExe=path\to\seepass.exe installer\seepass.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\seepass.exe"
#endif

#define AppName "SeePass"
#define AppExe "SeePass.exe"
#define SetupName "SeePass-setup"
; Marks the install folder; the app keeps its files next to it (src/install.rs).
#define Marker ".seepass-install"

[Setup]
; AppId never changes: installing over an existing copy upgrades it in place, in the same folder.
AppId={{008148C3-3677-4352-BB72-90C6B71A8237}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Akulav
AppPublisherURL=https://github.com/Akulav/offline-password-manager
AppSupportURL=https://github.com/Akulav/offline-password-manager/issues
AppUpdatesURL=https://github.com/Akulav/offline-password-manager/releases
AppCopyright=Made by Akulav
VersionInfoVersion={#AppVersion}
VersionInfoCompany=Akulav
VersionInfoDescription={#AppName} Setup
VersionInfoProductName={#AppName}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\SeePass
; Ask for the folder on the first install only; updates reuse the previous one.
DisableDirPage=auto
UsePreviousAppDir=yes
DirExistsWarning=no
DisableProgramGroupPage=yes
DefaultGroupName={#AppName}
OutputBaseFilename={#SetupName}
OutputDir=..\dist
SetupIconFile=..\assets\seepass.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
CloseApplications=force
RestartApplications=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/ultra64
SolidCompression=yes
LicenseFile=..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.DeleteData=Also DELETE your encrypted vault, its backup and settings?%n%nThis cannot be undone. Choose No to keep them (recommended).
english.LaunchApp=Launch SeePass
english.DataNote=SeePass and your encrypted vault, its backup and settings will be stored in this folder.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Comment: "Offline password manager - made by Akulav"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Comment: "Offline password manager - made by Akulav"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Parameters: "--installed"; WorkingDir: "{app}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\{#Marker}"

[Code]
procedure InitializeWizard();
begin
  WizardForm.SelectDirLabel.Caption := WizardForm.SelectDirLabel.Caption + #13#10 + CustomMessage('DataNote');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    SaveStringToFile(ExpandConstant('{app}\{#Marker}'), '{#AppVersion}', False);
end;

function HasAppData(Dir: String): Boolean;
begin
  Result := FileExists(Dir + '\vault.seepass') or FileExists(Dir + '\vault.seepass.bak')
    or FileExists(Dir + '\settings.json');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  AppDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    AppDir := ExpandConstant('{app}');
    { Never delete data when silent; the default answer is No. }
    if (not UninstallSilent) and HasAppData(AppDir) then
      if MsgBox(CustomMessage('DeleteData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      begin
        DeleteFile(AppDir + '\vault.seepass');
        DeleteFile(AppDir + '\vault.seepass.bak');
        DeleteFile(AppDir + '\settings.json');
      end;
    RemoveDir(AppDir);
  end;
end;
