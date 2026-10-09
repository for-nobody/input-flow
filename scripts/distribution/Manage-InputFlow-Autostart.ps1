[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Enable', 'Disable', 'Status')]
    [string]$Action,
    [string]$AgentPath = '',
    [string]$StartupDirectory = [Environment]::GetFolderPath('Startup')
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($AgentPath)) {
    $AgentPath = Join-Path $PSScriptRoot 'inputflow-agent.exe'
}
$shortcutName = 'InputFlow Agent.lnk'
$resolvedStartup = [System.IO.Path]::GetFullPath($StartupDirectory)
$shortcutPath = Join-Path $resolvedStartup $shortcutName
$resolvedAgent = [System.IO.Path]::GetFullPath($AgentPath)

if (-not ('InputFlow.Distribution.ShortcutFile' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Text;

namespace InputFlow.Distribution
{
    [ComImport]
    [Guid("00021401-0000-0000-C000-000000000046")]
    internal class ShellLink { }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("000214F9-0000-0000-C000-000000000046")]
    internal interface IShellLinkW
    {
        void GetPath([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder file, int count, IntPtr findData, uint flags);
        void GetIDList(out IntPtr idList);
        void SetIDList(IntPtr idList);
        void GetDescription([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder description, int count);
        void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string description);
        void GetWorkingDirectory([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder directory, int count);
        void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string directory);
        void GetArguments([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder arguments, int count);
        void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string arguments);
        void GetHotkey(out short hotkey);
        void SetHotkey(short hotkey);
        void GetShowCmd(out int showCommand);
        void SetShowCmd(int showCommand);
        void GetIconLocation([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder iconPath, int count, out int iconIndex);
        void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string iconPath, int iconIndex);
        void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string path, uint reserved);
        void Resolve(IntPtr window, uint flags);
        void SetPath([MarshalAs(UnmanagedType.LPWStr)] string path);
    }

    public sealed class ShortcutInfo
    {
        public string TargetPath { get; set; }
        public string WorkingDirectory { get; set; }
    }

    public static class ShortcutFile
    {
        public static void Write(string shortcutPath, string targetPath, string workingDirectory)
        {
            IShellLinkW link = (IShellLinkW)new ShellLink();
            try
            {
                link.SetPath(targetPath);
                link.SetArguments(string.Empty);
                link.SetWorkingDirectory(workingDirectory);
                link.SetDescription("Start InputFlow Agent for the current user");
                link.SetIconLocation(targetPath, 0);
                link.SetShowCmd(1);
                ((IPersistFile)link).Save(shortcutPath, true);
            }
            finally
            {
                Marshal.FinalReleaseComObject(link);
            }
        }

        public static ShortcutInfo Read(string shortcutPath)
        {
            IShellLinkW link = (IShellLinkW)new ShellLink();
            try
            {
                ((IPersistFile)link).Load(shortcutPath, 0);
                StringBuilder target = new StringBuilder(32768);
                StringBuilder working = new StringBuilder(32768);
                link.GetPath(target, target.Capacity, IntPtr.Zero, 0);
                link.GetWorkingDirectory(working, working.Capacity);
                return new ShortcutInfo
                {
                    TargetPath = target.ToString(),
                    WorkingDirectory = working.ToString()
                };
            }
            finally
            {
                Marshal.FinalReleaseComObject(link);
            }
        }
    }
}
'@
}

function Read-ExistingShortcut {
    if (-not (Test-Path -LiteralPath $shortcutPath -PathType Leaf)) {
        return $null
    }

    $shortcut = [InputFlow.Distribution.ShortcutFile]::Read($shortcutPath)
    $target = [System.IO.Path]::GetFullPath($shortcut.TargetPath)
    if ([System.IO.Path]::GetFileName($target) -ine 'inputflow-agent.exe') {
        throw "Refusing to modify '$shortcutPath': its target is not inputflow-agent.exe."
    }

    return [pscustomobject]@{
        TargetPath = $target
        WorkingDirectory = $shortcut.WorkingDirectory
    }
}

function Write-State([string]$State, [object]$Existing) {
    [ordered]@{
        action = $Action.ToLowerInvariant()
        state = $State
        shortcut_path = $shortcutPath
        expected_agent_path = $resolvedAgent
        target_path = if ($null -eq $Existing) { $null } else { $Existing.TargetPath }
        working_directory = if ($null -eq $Existing) { $null } else { $Existing.WorkingDirectory }
    } | ConvertTo-Json
}

$existing = Read-ExistingShortcut

switch ($Action) {
    'Enable' {
        if (-not (Test-Path -LiteralPath $resolvedAgent -PathType Leaf)) {
            throw "Agent does not exist: $resolvedAgent"
        }
        New-Item -ItemType Directory -Path $resolvedStartup -Force | Out-Null
        [InputFlow.Distribution.ShortcutFile]::Write(
            $shortcutPath,
            $resolvedAgent,
            (Split-Path -Parent $resolvedAgent))

        $verified = Read-ExistingShortcut
        if ($verified.TargetPath -ine $resolvedAgent) {
            throw "Startup shortcut verification failed: $($verified.TargetPath)"
        }
        Write-State 'enabled' $verified
    }
    'Disable' {
        if ($null -ne $existing) {
            Remove-Item -LiteralPath $shortcutPath -Force
        }
        if (Test-Path -LiteralPath $shortcutPath) {
            throw "Startup shortcut still exists: $shortcutPath"
        }
        Write-State 'disabled' $null
    }
    'Status' {
        if ($null -eq $existing) {
            Write-State 'disabled' $null
        }
        elseif ($existing.TargetPath -ieq $resolvedAgent) {
            Write-State 'enabled_current_path' $existing
        }
        else {
            Write-State 'enabled_other_path' $existing
        }
    }
}
