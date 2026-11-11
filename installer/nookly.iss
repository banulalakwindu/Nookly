; Nookly installer (Inno Setup 6). Built by build.ps1:
;   ISCC.exe /DAppVersion=0.1.0 /DOutDir=<folder> installer\nookly.iss

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef OutDir
  #define OutDir "..\dist"
#endif

[Setup]
AppId={{8D994A83-65FE-4DF7-97E0-76482363209A}
AppName=Nookly
AppVersion={#AppVersion}
AppVerName=Nookly {#AppVersion}
AppPublisher=Banula Lakwindu
AppPublisherURL=https://github.com/banulalakwindu/Nookly
AppSupportURL=https://github.com/banulalakwindu/Nookly/issues
AppUpdatesURL=https://github.com/banulalakwindu/Nookly/releases
AppCopyright=Copyright (C) 2026 Banula Lakwindu; based on WindyLand by Shabab
AppMutex=Nookly.SingleInstance
CloseApplications=yes
DefaultDirName={autopf}\Nookly
DefaultGroupName=Nookly
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\LICENSE
SetupIconFile=..\assets\nookly.ico
UninstallDisplayIcon={app}\nookly.exe
UninstallDisplayName=Nookly
OutputDir={#OutDir}
OutputBaseFilename=NooklySetup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
InfoBeforeFile=privacy.txt
VersionInfoCompany=Banula Lakwindu
VersionInfoDescription=Nookly Setup
VersionInfoProductName=Nookly
VersionInfoProductVersion={#AppVersion}
VersionInfoVersion={#AppVersion}
VersionInfoCopyright=Copyright (C) 2026 Banula Lakwindu; based on WindyLand by Shabab

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
en.StartupTask=Start Nookly when Windows starts
en.RemoveData=Also remove your Nookly data (to-do list, water log, shelf, settings)?
en.UpdatesTask=Check for updates automatically (asks GitHub for the latest version about twice a day)

[Tasks]
Name: "startup"; Description: "{cm:StartupTask}"
Name: "desktop"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked
Name: "updates"; Description: "{cm:UpdatesTask}"

[Files]
Source: "..\target\release\nookly.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Nookly"; ValueData: """{app}\nookly.exe"""; Tasks: startup; Flags: uninsdeletevalue

[Icons]
Name: "{autoprograms}\Nookly"; Filename: "{app}\nookly.exe"
Name: "{autodesktop}\Nookly"; Filename: "{app}\nookly.exe"; Tasks: desktop

[Run]
Filename: "{app}\nookly.exe"; Description: "{cm:LaunchProgram,Nookly}"; Flags: postinstall nowait

[UninstallRun]
; Remove the Claude Code / Codex hooks so they don't point at a deleted exe.
Filename: "{app}\nookly.exe"; Parameters: "--disconnect-agents"; Flags: runhidden waituntilterminated; RunOnceId: "DisconnectAgents"

[UninstallDelete]
Type: filesandordirs; Name: "{app}"

[Code]
// Remember the "Check for updates automatically" choice in Nookly's
// settings (a later line wins, so appending is enough).
procedure CurStepChanged(CurStep: TSetupStep);
var
  Dir, Line: String;
begin
  if CurStep = ssPostInstall then
  begin
    Dir := ExpandConstant('{userappdata}\Nookly');
    ForceDirectories(Dir);
    if WizardIsTaskSelected('updates') then
      Line := 'update_check=1'
    else
      Line := 'update_check=0';
    SaveStringToFile(Dir + '\settings.txt', #13#10 + Line + #13#10, True);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    if DirExists(ExpandConstant('{userappdata}\Nookly')) then
      if MsgBox(CustomMessage('RemoveData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(ExpandConstant('{userappdata}\Nookly'), True, True, True);
end;
