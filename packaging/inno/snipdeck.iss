; Snipdeck installer (Inno Setup 6). Per-user: no admin prompt, no UAC.
;
; Build (see docs/packaging.md):
;   iscc /DAppVersion=1.0.0 [/DSourceExe=C:\path\snipdeck-app.exe] [/DOutputSuffix=-dev-abc1234] packaging\inno\snipdeck.iss
;
; Checked on Linux by scripts/installer_check.py (GUID, mutex, run-value name, source files, the
; registration table packaging/registration.json, a line per language for every custom message).
; This file is UTF-8 WITH a byte-order mark: Inno reads a file without one as ANSI and garbles the translations.
; Build with /DArm64=1 for the native ARM64 exe (the default installs the x64 exe, which also runs on ARM64).
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
#ifdef Arm64
  #define Arch "arm64"
#else
  #define Arch "x64compatible"
#endif

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
ArchitecturesAllowed={#Arch}
ArchitecturesInstallIn64BitMode={#Arch}
; The ".snipdeck" association and the "snipdeck:" URL scheme (packaging/registration.json) need a shell refresh.
ChangesAssociations=yes
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
; Every message has a line for EVERY language in [Languages] (scripts/installer_check.py fails otherwise).
; Chinese Simplified has no Setup language, so it shows the English lines.
; %n is a line break; %1 and %2 are filled by FmtMessage.
english.StillRunning=Snipdeck is still running. Right-click its tray icon, choose Quit, then run Setup again.
german.StillRunning=Snipdeck läuft noch. Klicken Sie mit der rechten Maustaste auf das Symbol im Infobereich, wählen Sie Beenden und starten Sie das Setup erneut.
spanish.StillRunning=Snipdeck sigue en ejecución. Haga clic con el botón derecho en su icono de la bandeja, elija Salir y vuelva a ejecutar la instalación.
french.StillRunning=Snipdeck est toujours en cours d'exécution. Faites un clic droit sur son icône de la zone de notification, choisissez Quitter, puis relancez l'installation.
japanese.StillRunning=Snipdeck はまだ実行中です。タスクトレイのアイコンを右クリックして「終了」を選び、もう一度セットアップを実行してください。
brazilianportuguese.StillRunning=O Snipdeck ainda está em execução. Clique com o botão direito no ícone da bandeja, escolha Sair e execute a instalação novamente.

english.StillRunningUninstall=Snipdeck is still running. Right-click its tray icon, choose Quit, then run the uninstaller again.
german.StillRunningUninstall=Snipdeck läuft noch. Klicken Sie mit der rechten Maustaste auf das Symbol im Infobereich, wählen Sie Beenden und starten Sie die Deinstallation erneut.
spanish.StillRunningUninstall=Snipdeck sigue en ejecución. Haga clic con el botón derecho en su icono de la bandeja, elija Salir y vuelva a ejecutar el desinstalador.
french.StillRunningUninstall=Snipdeck est toujours en cours d'exécution. Faites un clic droit sur son icône de la zone de notification, choisissez Quitter, puis relancez la désinstallation.
japanese.StillRunningUninstall=Snipdeck はまだ実行中です。タスクトレイのアイコンを右クリックして「終了」を選び、もう一度アンインストーラーを実行してください。
brazilianportuguese.StillRunningUninstall=O Snipdeck ainda está em execução. Clique com o botão direito no ícone da bandeja, escolha Sair e execute o desinstalador novamente.

english.EraseQuestion=Also erase your Snipdeck library?%n%nThis deletes your cards, settings and backups kept in:%n%1%n%nIt cannot be undone. Choose No to keep them.
german.EraseQuestion=Auch Ihre Snipdeck-Bibliothek löschen?%n%nDadurch werden Ihre Karten, Einstellungen und Sicherungen gelöscht, die hier liegen:%n%1%n%nDies kann nicht rückgängig gemacht werden. Wählen Sie Nein, um sie zu behalten.
spanish.EraseQuestion=¿Borrar también su biblioteca de Snipdeck?%n%nSe eliminarán sus tarjetas, ajustes y copias de seguridad guardados en:%n%1%n%nNo se puede deshacer. Elija No para conservarlos.
french.EraseQuestion=Supprimer aussi votre bibliothèque Snipdeck ?%n%nCela supprime vos cartes, vos paramètres et vos sauvegardes conservés dans :%n%1%n%nCette action est irréversible. Choisissez Non pour les conserver.
japanese.EraseQuestion=Snipdeck のライブラリも削除しますか?%n%n次の場所にあるカード、設定、バックアップが削除されます:%n%1%n%nこの操作は元に戻せません。残す場合は「いいえ」を選んでください。
brazilianportuguese.EraseQuestion=Apagar também a sua biblioteca do Snipdeck?%n%nIsso exclui seus cartões, configurações e backups guardados em:%n%1%n%nNão é possível desfazer. Escolha Não para mantê-los.

english.DesktopIconTask=Create a &desktop shortcut
german.DesktopIconTask=&Desktop-Verknüpfung erstellen
spanish.DesktopIconTask=Crear un acceso directo en el &escritorio
french.DesktopIconTask=Créer un raccourci sur le &Bureau
japanese.DesktopIconTask=デスクトップにショートカットを作成する(&D)
brazilianportuguese.DesktopIconTask=Criar um atalho na área de &trabalho

english.DowngradeWarning=Snipdeck %1 is already installed and this Setup is for the older version %2.%n%nYour library may have been updated to a newer format that the older version cannot open. Install the older version anyway?
german.DowngradeWarning=Snipdeck %1 ist bereits installiert, dieses Setup enthält die ältere Version %2.%n%nIhre Bibliothek wurde möglicherweise in ein neueres Format umgewandelt, das die ältere Version nicht öffnen kann. Die ältere Version trotzdem installieren?
spanish.DowngradeWarning=Snipdeck %1 ya está instalado y esta instalación es de la versión anterior %2.%n%nEs posible que su biblioteca se haya actualizado a un formato más nuevo que la versión anterior no puede abrir. ¿Instalar la versión anterior de todos modos?
french.DowngradeWarning=Snipdeck %1 est déjà installé et cette installation correspond à la version plus ancienne %2.%n%nVotre bibliothèque a peut-être été convertie dans un format plus récent que l'ancienne version ne peut pas ouvrir. Installer quand même l'ancienne version ?
japanese.DowngradeWarning=Snipdeck %1 がすでにインストールされており、このセットアップは古いバージョン %2 です。%n%nライブラリが新しい形式に更新されている場合、古いバージョンでは開けない可能性があります。それでも古いバージョンをインストールしますか?
brazilianportuguese.DowngradeWarning=O Snipdeck %1 já está instalado e esta instalação é da versão mais antiga %2.%n%nSua biblioteca pode ter sido atualizada para um formato mais novo que a versão antiga não consegue abrir. Instalar a versão antiga mesmo assim?

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
; Leftovers: the app writes these itself; Setup writes nothing and only removes them on uninstall.
; The list equals "registry_leftovers" in packaging/registration.json and Get-RegistryLeftovers in
; tools/launcher/data.ps1 (scripts/installer_check.py compares them).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Snipdeck"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"; ValueType: none; ValueName: "Snipdeck"; Flags: uninsdeletevalue
; The data folder the app records when it is not the default (crates/snipdeck-platform/src/data_record.rs).
Root: HKCU; Subkey: "Software\Snipdeck"; ValueType: none; Flags: uninsdeletekey
; File association and URL scheme, written from packaging/registration.json (per user, so under Software\Classes).
Root: HKCU; Subkey: "Software\Classes\.snipdeck"; ValueType: string; ValueName: ""; ValueData: "Snipdeck.Archive"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Snipdeck.Archive"; ValueType: string; ValueName: ""; ValueData: "Snipdeck archive"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Snipdeck.Archive\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\snipdeck.exe,0"
Root: HKCU; Subkey: "Software\Classes\Snipdeck.Archive\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\snipdeck.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\snipdeck"; ValueType: string; ValueName: ""; ValueData: "URL:Snipdeck"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\snipdeck"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\snipdeck\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\snipdeck.exe,0"
Root: HKCU; Subkey: "Software\Classes\snipdeck\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\snipdeck.exe"" ""%1"""

[Run]
; {code:LaunchParams} is --hidden when Snipdeck was running before the upgrade, so it comes back in the tray.
Filename: "{app}\snipdeck.exe"; Parameters: "{code:LaunchParams}"; Description: "{cm:LaunchProgram,Snipdeck}"; Flags: nowait postinstall skipifsilent

[Code]
const
  MutexName = 'Snipdeck-main';
  { The uninstall entry Inno writes for AppId (the doubled brace in [Setup] is one brace in the key name). }
  UninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{5ACCE38D-D2C8-4BD3-88BD-9E76E5D0D402}_is1';
  { Where the app records a data folder chosen with --data-dir (crates/snipdeck-platform/src/data_record.rs). }
  DataKey = 'Software\Snipdeck';
  DataValue = 'DataDirs';
  { The tables below equal the ones in data_record.rs (scripts/installer_check.py compares them). }
  MarkerFile = 'snipdeck-data-folder.txt';
  OwnEntries = 'snipdeck.db|snipdeck.db-wal|snipdeck.db-shm|snipdeck.lock|blobs|settings.json|settings.json.bad|maintenance.json|drafts|backups|logs';
  ProtectedNames = 'documents|desktop|downloads|pictures|music|videos|users|appdata|local|roaming|windows|system32|program files|program files (x86)|programdata|onedrive';

var
  WasRunning: Boolean;
  RecordedFolders: String;

function IsRunning: Boolean;
begin
  Result := CheckForMutexes(MutexName);
end;

{ The [Run] "Start Snipdeck" entry: hidden in the tray when it was running before an upgrade. Unverified on Windows. }
function LaunchParams(Param: String): String;
begin
  if WasRunning then
    Result := '--hidden'
  else
    Result := '';
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

{ True when the installed version is newer than this Setup (compared on the numbers; "-beta" is ignored). }
function IsDowngrade(var Installed: String): Boolean;
var
  Numeric: String;
  P: Integer;
  InstalledV, ThisV: Int64;
begin
  Result := False;
  if not RegQueryStringValue(HKCU, UninstallKey, 'DisplayVersion', Installed) then
    Exit;
  Numeric := Installed;
  P := Pos('-', Numeric);
  if P > 0 then
    Numeric := Copy(Numeric, 1, P - 1);
  if StrToVersion(Numeric, InstalledV) and StrToVersion('{#NumericVersion}', ThisV) then
    Result := ComparePackedVersion(InstalledV, ThisV) > 0;
end;

function InitializeSetup: Boolean;
var
  Installed: String;
begin
  Result := True;
  { An older exe must not meet a newer database: ask, and let a silent run answer No. Unverified on Windows. }
  if IsDowngrade(Installed) then
  begin
    if SuppressibleMsgBox(FmtMessage(CustomMessage('DowngradeWarning'), [Installed, '{#AppVersion}']),
         mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDNO then
    begin
      Result := False;
      Exit;
    end;
  end;
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
  { Read now: the [Registry] uninsdeletekey entries remove the record before the erase question comes. }
  if not RegQueryMultiStringValue(HKCU, DataKey, DataValue, RecordedFolders) then
    RecordedFolders := '';
  if IsRunning then
  begin
    if not QuitRunning(ExpandConstant('{app}\snipdeck.exe')) then
    begin
      MsgBox(CustomMessage('StillRunningUninstall'), mbError, MB_OK);
      Result := False;
    end;
  end;
end;

{ Mirrors safe_to_erase in data_record.rs: a drive-letter folder below the root, not the profile, not a
  well-known folder, no "..". }
function SafeToErase(const Path: String): Boolean;
var
  P, Last: String;
begin
  Result := False;
  P := Path;
  StringChangeEx(P, '/', '\', True);
  P := RemoveBackslash(P);
  if (Length(P) <= 3) or (Pos('\..\', P + '\') > 0) then
    Exit;
  if (P[2] <> ':') or (P[3] <> '\') or (Pos(Lowercase(P[1]), 'abcdefghijklmnopqrstuvwxyz') = 0) then
    Exit;
  if CompareText(P, RemoveBackslash(GetEnv('USERPROFILE'))) = 0 then
    Exit;
  Last := Lowercase(ExtractFileName(P));
  Result := Pos('|' + Last + '|', '|' + ProtectedNames + '|') = 0;
end;

{ A recorded folder is cleaned only when it passes SafeToErase, exists and holds the marker file the app wrote. }
function ErasableFolder(const Folder: String): Boolean;
begin
  Result := SafeToErase(Folder) and DirExists(Folder) and FileExists(RemoveBackslash(Folder) + '\' + MarkerFile);
end;

{ Deletes Snipdeck's own names and the marker, then removes the folder only if nothing else is left in it
  (RemoveDir fails on a folder with content). Never a DelTree on the folder itself. Unverified on Windows. }
procedure EraseOwnFiles(const Folder: String);
var
  Rest, Name, Full: String;
  P: Integer;
begin
  Rest := OwnEntries + '|';
  while Rest <> '' do
  begin
    P := Pos('|', Rest);
    Name := Copy(Rest, 1, P - 1);
    Rest := Copy(Rest, P + 1, Length(Rest));
    Full := RemoveBackslash(Folder) + '\' + Name;
    if DirExists(Full) then
      DelTree(Full, True, True, True)
    else if FileExists(Full) then
      DeleteFile(Full);
  end;
  DeleteFile(RemoveBackslash(Folder) + '\' + MarkerFile);
  RemoveDir(RemoveBackslash(Folder));
end;

{ The erasable folders among the recorded ones (separated by #0), one per line, without the default folder. }
function ErasableRecorded(const DefaultFolder: String): String;
var
  Rest, Item: String;
  P: Integer;
begin
  Result := '';
  Rest := RecordedFolders;
  while Rest <> '' do
  begin
    P := Pos(#0, Rest);
    if P = 0 then
    begin
      Item := Rest;
      Rest := '';
    end
    else
    begin
      Item := Copy(Rest, 1, P - 1);
      Rest := Copy(Rest, P + 1, Length(Rest));
    end;
    if (Item <> '') and ErasableFolder(Item) and (CompareText(RemoveBackslash(Item), DefaultFolder) <> 0) and
       (Pos(#13#10 + RemoveBackslash(Item) + #13#10, #13#10 + Result + #13#10) = 0) then
    begin
      if Result <> '' then
        Result := Result + #13#10;
      Result := Result + RemoveBackslash(Item);
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Data, Custom, Folders, Rest, Profile, Tmp: String;
  P: Integer;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    Profile := GetEnv('USERPROFILE');
    Tmp := GetEnv('TEMP');
    { Files pasted from Files items are temporary copies (possibly decrypted): always removed.
      An unset variable would make these paths at the drive root, so nothing is deleted then. }
    if Profile <> '' then
      DelTree(Profile + '\Snipdeck paste files', True, True, True);
    if Tmp <> '' then
      DelTree(Tmp + '\Snipdeck', True, True, True);
    { The library is never erased silently. }
    if not UninstallSilent then
    begin
      Data := RemoveBackslash(ExpandConstant('{localappdata}\Snipdeck'));
      Custom := ErasableRecorded(Data);
      Folders := '';
      if DirExists(Data) then
        Folders := Data;
      if Custom <> '' then
      begin
        if Folders <> '' then
          Folders := Folders + #13#10;
        Folders := Folders + Custom;
      end;
      if (Folders <> '') and
         (MsgBox(FmtMessage(CustomMessage('EraseQuestion'), [Folders]), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
      begin
        if DirExists(Data) then
          DelTree(Data, True, True, True);
        Rest := Custom;
        if Rest <> '' then
          Rest := Rest + #13#10;
        while Rest <> '' do
        begin
          P := Pos(#13#10, Rest);
          EraseOwnFiles(Copy(Rest, 1, P - 1));
          Rest := Copy(Rest, P + 2, Length(Rest));
        end;
      end;
    end;
  end;
end;
