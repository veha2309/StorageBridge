#define AppVersion "0.3.0"
#ifndef BundleDir
  #define BundleDir "..\release\StorageBridge"
#endif
[Setup]
AppId={{63AB6BD9-0D40-4F99-A6A7-226807D9B35F}
AppName=Storage Bridge
AppVersion={#AppVersion}
AppPublisher=Storage Bridge
DefaultDirName={localappdata}\Programs\StorageBridge
DefaultGroupName=Storage Bridge
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\release
OutputBaseFilename=StorageBridge-Setup-{#AppVersion}-x64
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
AppMutex=Local\StorageBridgeDesktop,Local\StorageBridgeEngine
UninstallDisplayIcon={app}\storage_bridge.exe

[Files]
Source: "{#BundleDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Icons]
Name: "{group}\Storage Bridge"; Filename: "{app}\storage_bridge.exe"
Name: "{autodesktop}\Storage Bridge"; Filename: "{app}\storage_bridge.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\storage_bridge.exe"; Description: "Open Storage Bridge"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "StorageBridge"; Flags: uninsdeletevalue

[Code]
function InitializeSetup(): Boolean;
var InstallDir: String;
begin
  Result := True;
  if not RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\WinFsp', 'InstallDir', InstallDir) and
     not RegQueryStringValue(HKLM, 'SOFTWARE\WinFsp', 'InstallDir', InstallDir) then
    MsgBox('WinFsp is required to mount the drive. Install it from https://winfsp.dev/rel/ before mounting. Storage Bridge can still be installed now.', mbInformation, MB_OK);
end;
