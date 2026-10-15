; OpenPTZ Inno Setup Script
; Builds a self-contained openptz-windows-x86_64-setup.exe installer.

#define MyAppName "OpenPTZ"
#define MyAppVersion "0.2.0"
#define MyAppPublisher "andryjay"
#define MyAppURL "https://github.com/jayAndrey/OpenPTZ"
#define MyAppExeName "openptz.exe"

[Setup]
AppId={{1B65787F-B637-4AEB-B220-3463684B69A9}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
LicenseFile=..\..\LICENSE-MIT
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\..\target\installer
OutputBaseFilename=openptz-windows-x86_64-setup
SetupIconFile=..\..\assets\openptz.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "addtopath"; Description: "Add OpenPTZ to user PATH environment variable"; GroupDescription: "System Integration:"

[Files]
Source: "..\..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\openptz.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\OpenPTZ Native GUI"; Filename: "{app}\{#MyAppExeName}"; Parameters: "gui"; IconFilename: "{app}\openptz.ico"; Comment: "Launch OpenPTZ Native Camera Controller"
Name: "{group}\OpenPTZ Network UI"; Filename: "{app}\{#MyAppExeName}"; Parameters: "ui"; IconFilename: "{app}\openptz.ico"; Comment: "Launch OpenPTZ Network Interface"
Name: "{group}\OpenPTZ Terminal UI"; Filename: "{app}\{#MyAppExeName}"; Parameters: "tui"; IconFilename: "{app}\openptz.ico"; Comment: "Launch OpenPTZ Terminal Interface"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\OpenPTZ"; Filename: "{app}\{#MyAppExeName}"; Parameters: "gui"; IconFilename: "{app}\openptz.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Parameters: "gui"; Description: "Launch OpenPTZ Native GUI"; Flags: nowait postinstall skipifsilent

[Code]
// Helper function to add/remove {app} directory to/from user environment PATH
const EnvironmentKey = 'Environment';

procedure CurStepChanged(CurStep: TSetupStep);
var
  Path: string;
  AppPath: string;
begin
  if (CurStep = ssPostInstall) and IsTaskSelected('addtopath') then
  begin
    AppPath := ExpandConstant('{app}');
    if RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path) then
    begin
      if Pos(Uppercase(AppPath), Uppercase(Path)) = 0 then
      begin
        if (Length(Path) > 0) and (Path[Length(Path)] <> ';') then
          Path := Path + ';';
        Path := Path + AppPath;
        RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path);
      end;
    end
    else
    begin
      RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', AppPath);
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Path: string;
  AppPath: string;
  P: Integer;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    AppPath := ExpandConstant('{app}');
    if RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path) then
    begin
      P := Pos(Uppercase(AppPath) + ';', Uppercase(Path));
      if P > 0 then
      begin
        Delete(Path, P, Length(AppPath) + 1);
        RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path);
      end
      else
      begin
        P := Pos(';' + Uppercase(AppPath), Uppercase(Path));
        if P > 0 then
        begin
          Delete(Path, P, Length(AppPath) + 1);
          RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path);
        end
        else
        begin
          P := Pos(Uppercase(AppPath), Uppercase(Path));
          if P > 0 then
          begin
            Delete(Path, P, Length(AppPath));
            RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path);
          end;
        end;
      end;
    end;
  end;
end;
