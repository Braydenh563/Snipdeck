; Snipdeck installer (Inno Setup 6). Per-user: no admin prompt, no UAC.
;
; Build (see docs/packaging.md):
;   iscc /DAppVersion=1.0.0 [/DSourceExe=C:\path\snipdeck-app.exe] [/DOutputSuffix=-dev-abc1234] packaging\inno\snipdeck.iss
;
; Checked on Linux by scripts/installer_check.py (GUID, mutex, run-value name, source files).
; The library lives in %LOCALAPPDATA%\Snipdeck whatever the install folder is; Setup never touches it
; except when the user answers Yes to the erase question at uninstall.

#ifndef AppVersion
  #error Pass the version: iscc /DAppVersion=1.0.0 packaging\inno\snipdeck.iss
#endif

; Paths are relative to this file unless the caller overrides them.
#ifndef RepoRoot
  #define RepoRoot "..\.."
#endif
#ifndef SourceExe
  #define SourceExe RepoRoot + "\target\release\snipdeck-app.exe"
#endif
#ifndef OutputSuffix
  #define OutputSuffix ""
#endif
; Windows file versions are four numbers: drop any "-beta.1" suffix.
#define NumericVersion Copy(AppVersion, 1, Pos("-", AppVersion + "-") - 1)
#define SourceExeName ExtractFileName(SourceExe)
#define RepoUrl "https://github.com/Braydenh563/Snipdeck"

[Setup]
; AppId identifies this app to Windows (the uninstall entry) and to every later Setup, so a new
; version upgrades in place. It must NEVER change. The doubled "{{" is Inno's escape for "{".
AppId={{5ACCE38D-D2C8-4BD3-88BD-9E76E5D0D402}
AppName=Snipdeck
AppVersion={#AppVersion}
AppPublisher=Snipdeck
AppPublisherURL={#RepoUrl}
AppSupportURL={#RepoUrl}/issues
AppUpdatesURL={#RepoUrl}/releases
VersionInfoVersion={#NumericVersion}.0
VersionInfoProductName=Snipdeck
PrivilegesRequired=lowest
; {autopf} is %LOCALAPPDATA%\Programs for a per-user install.
DefaultDirName={autopf}\Snipdeck
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 version 1903 (build 18362), the oldest the app supports. Keep equal to the MSIX template.
MinVersion=10.0.18362
WizardStyle=modern
SetupIconFile={#RepoRoot}\assets\logo\snipdeck.ico
UninstallDisplayIcon={app}\snipdeck.exe
UninstallDisplayName=Snipdeck
OutputDir={#RepoRoot}\dist
OutputBaseFilename=Snipdeck-Setup-{#AppVersion}{#OutputSuffix}
Compression=lzma2/max
SolidCompression=yes
; Setup quits Snipdeck itself ([Code]); the Restart Manager would only kill it.
AppMutex=Snipdeck-main
CloseApplications=no
RestartApplications=no
; Optional signing. The plan is SignPath (docs/packaging.md); until then Setup is unsigned.
#ifdef SignTool
SignTool={#SignTool}
SignedUninstaller=yes
#endif

[Languages]
; The app's locales are de, es, fr, ja, pt-BR and zh-Hans. Inno 6 does not ship Chinese Simplified,
; so that one falls back to English here.
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[CustomMessages]
StillRunning=Snipdeck is still running. Right-click its tray icon, choose Quit, then run Setup again.
StillRunningUninstall=Snipdeck is still running. Right-click its tray icon, choose Quit, then run the uninstaller again.
EraseQuestion=Also erase your Snipdeck library?%n%nThis deletes your cards, settings and backups kept in:%n%1%n%nIt cannot be undone. Choose No to keep them.
DesktopIconTask=Create a &desktop shortcut

[Tasks]
; "Start with Windows" is not offered here: the app owns that setting (Settings, or first-run).
Name: "desktopicon"; Description: "{cm:DesktopIconTask}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; The release exe is built as snipdeck-app.exe and installed as snipdeck.exe.
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "snipdeck.exe"; Flags: ignoreversion
Source: "{#RepoRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\docs\user-guide.md"; DestDir: "{app}"; Flags: ignoreversion
; A second copy, extracted to {tmp} so Setup can run the NEW "--quit" before anything is replaced.
Source: "{#SourceExe}"; DestDir: "{tmp}"; Flags: dontcopy

[Icons]
Name: "{autoprograms}\Snipdeck"; Filename: "{app}\snipdeck.exe"
Name: "{autodesktop}\Snipdeck"; Filename: "{app}\snipdeck.exe"; Tasks: desktopicon

[Registry]
; "Start with Windows" is a Run value the app writes itself. Setup writes nothing; it only removes it.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Snipdeck"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\snipdeck.exe"; Description: "{cm:LaunchProgram,Snipdeck}"; Flags: nowait postinstall skipifsilent

[Code]
const
  MutexName = 'Snipdeck-main';

var
  WasRunning: Boolean;

function IsRunning: Boolean;
begin
  Result := CheckForMutexes(MutexName);
end;

{ Asks the running Snipdeck to quit like tray > Quit, then waits up to 15 s. True when none is running. }
function QuitRunning(const ExePath: String): Boolean;
var
  ResultCode, I: Integer;
begin
  Result := True;
  if not IsRunning then
    Exit;
  WasRunning := True;
  { Old versions ignore --quit (exit code 2, no window); the wait below then times out. }
  Exec(ExePath, '--quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  for I := 1 to 30 do
  begin
    if not IsRunning then
      Exit;
    Sleep(500);
  end;
  Result := not IsRunning;
end;

function InitializeSetup: Boolean;
begin
  Result := True;
  { A silent run has no wizard page to stop at: quit the old copy before Setup's own mutex check. }
  if WizardSilent and IsRunning then
  begin
    try
      ExtractTemporaryFile('{#SourceExeName}');
      QuitRunning(ExpandConstant('{tmp}\{#SourceExeName}'));
    except
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if IsRunning then
  begin
    ExtractTemporaryFile('{#SourceExeName}');
    if not QuitRunning(ExpandConstant('{tmp}\{#SourceExeName}')) then
      Result := CustomMessage('StillRunning');
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
begin
  { A silent upgrade has no "Start Snipdeck" checkbox: bring it back if it was running. }
  if (CurStep = ssPostInstall) and WasRunning and WizardSilent then
    Exec(ExpandConstant('{app}\snipdeck.exe'), '--hidden', '', SW_SHOWNORMAL, ewNoWait, ResultCode);
end;

function InitializeUninstall: Boolean;
begin
  Result := True;
  if IsRunning then
  begin
    if not QuitRunning(ExpandConstant('{app}\snipdeck.exe')) then
    begin
      MsgBox(CustomMessage('StillRunningUninstall'), mbError, MB_OK);
      Result := False;
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Data: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    { The library is never erased silently. }
    if not UninstallSilent then
    begin
      Data := ExpandConstant('{localappdata}\Snipdeck');
      if DirExists(Data) and
         (MsgBox(FmtMessage(CustomMessage('EraseQuestion'), [Data]), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
        DelTree(Data, True, True, True);
    end;
  end;
end;
