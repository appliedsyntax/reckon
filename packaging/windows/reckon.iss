; Inno Setup script for the Windows installer. Built by build.ps1:
;   iscc /DAppVersion=0.1.0 /DStageDir=... /DOutDir=... reckon.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6F0B8C2E-4B7D-4E59-9C51-7A3D2B9E1F40}
AppName=Reckon
AppVersion={#AppVersion}
AppPublisher=The Reckon Authors
DefaultDirName={autopf}\Reckon
DefaultGroupName=Reckon
DisableProgramGroupPage=yes
OutputDir={#OutDir}
OutputBaseFilename=Reckon-{#AppVersion}-windows-x64-setup
SetupIconFile=reckon.ico
UninstallDisplayIcon={app}\reckon.ico
LicenseFile={#StageDir}\LICENSE.txt
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\Reckon"; Filename: "{app}\Reckon.exe"; IconFilename: "{app}\reckon.ico"
Name: "{autodesktop}\Reckon"; Filename: "{app}\Reckon.exe"; IconFilename: "{app}\reckon.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\Reckon.exe"; Description: "{cm:LaunchProgram,Reckon}"; Flags: nowait postinstall skipifsilent
