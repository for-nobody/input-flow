[CmdletBinding()]
param(
    [string]$SettingsExecutable = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($SettingsExecutable)) {
    $SettingsExecutable = Join-Path $repositoryRoot 'apps\settings-winui\InputFlow.Settings\bin\Debug\net10.0-windows10.0.26100.0\win-x64\InputFlow.Settings.exe'
}
$SettingsExecutable = [System.IO.Path]::GetFullPath($SettingsExecutable)
if (-not (Test-Path -LiteralPath $SettingsExecutable -PathType Leaf)) {
    throw "Settings executable is missing: $SettingsExecutable"
}

$preferencePath = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'InputFlow\ui-preferences.json'
$preferencePath = [System.IO.Path]::GetFullPath($preferencePath)
$expectedPreferencePath = [System.IO.Path]::GetFullPath((Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'InputFlow\ui-preferences.json'))
if (-not $preferencePath.Equals($expectedPreferencePath, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to modify an unexpected preference path: $preferencePath"
}
$preferenceExisted = Test-Path -LiteralPath $preferencePath -PathType Leaf
$preferenceBackup = if ($preferenceExisted) { [System.IO.File]::ReadAllBytes($preferencePath) } else { $null }

Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName UIAutomationClient
$startedProcesses = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()
$simplifiedChinese = -join @([char]0x7B80, [char]0x4F53, [char]0x4E2D, [char]0x6587)
$settingsChinese = -join @([char]0x8BBE, [char]0x7F6E)
$openNavigationChinese = -join @([char]0x6253, [char]0x5F00, [char]0x5BFC, [char]0x822A)
$navigationChinese = 'InputFlow ' + $settingsChinese + (-join @([char]0x5BFC, [char]0x822A))
$windowTitleChinese = 'InputFlow ' + $settingsChinese
$languageComboChinese = $settingsChinese + (-join @(
    [char]0x754C, [char]0x9762, [char]0x663E, [char]0x793A,
    [char]0x8BED, [char]0x8A00))
$restartChinese = (-join @([char]0x91CD, [char]0x65B0, [char]0x6253, [char]0x5F00)) +
    ' Settings ' + (-join @([char]0x540E, [char]0x751F, [char]0x6548))
$systemOption = 'System default / ' + (-join @(
    [char]0x8DDF, [char]0x968F, [char]0x7CFB, [char]0x7EDF))
$descendants = [System.Windows.Automation.TreeScope]::Descendants
$nameProperty = [System.Windows.Automation.AutomationElement]::NameProperty

function Find-NamedElement(
    [System.Windows.Automation.AutomationElement]$Root,
    [string]$Name
) {
    return $Root.FindFirst(
        $descendants,
        [System.Windows.Automation.PropertyCondition]::new($nameProperty, $Name))
}

function Open-Settings([string]$ExpectedTitle, [string]$ExpectedNavigationName) {
    $directory = Split-Path -Parent $SettingsExecutable
    $process = Start-Process `
        -FilePath $SettingsExecutable `
        -WorkingDirectory $directory `
        -WindowStyle Hidden `
        -PassThru
    $script:startedProcesses.Add($process)
    $deadline = [DateTimeOffset]::Now.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 200
        $process.Refresh()
    } until ($process.HasExited -or $process.MainWindowHandle -ne 0 -or [DateTimeOffset]::Now -ge $deadline)
    if ($process.HasExited) {
        throw "Settings exited before creating a window: $($process.ExitCode)"
    }
    if ($process.MainWindowHandle -eq 0) {
        Stop-Process -Id $process.Id -Force
        throw 'Settings did not create a window within 15 seconds.'
    }
    if ($process.MainWindowTitle -ne $ExpectedTitle) {
        Stop-Process -Id $process.Id -Force
        throw "Unexpected Settings title '$($process.MainWindowTitle)'; expected '$ExpectedTitle'."
    }

    $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    if ($null -eq (Find-NamedElement $root $ExpectedNavigationName)) {
        Stop-Process -Id $process.Id -Force
        throw "Localized navigation UIA name was not found: $ExpectedNavigationName"
    }
    return [pscustomobject]@{ Process = $process; Root = $root }
}

function Open-LanguageSettings(
    [System.Windows.Automation.AutomationElement]$Root,
    [string]$OpenNavigationName,
    [string]$SettingsItemName,
    [string]$LanguageComboName
) {
    $open = Find-NamedElement $Root $OpenNavigationName
    if ($null -ne $open) {
        $open.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 300
    }
    $settings = Find-NamedElement $Root $SettingsItemName
    if ($null -eq $settings) { throw "Navigation item was not found: $SettingsItemName" }
    $settings.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Start-Sleep -Milliseconds 500
    $combo = Find-NamedElement $Root $LanguageComboName
    if ($null -eq $combo) { throw "Language combo was not found: $LanguageComboName" }
    return $combo
}

function Select-Language(
    [System.Windows.Automation.AutomationElement]$Root,
    [System.Windows.Automation.AutomationElement]$Combo,
    [string]$VisibleOption,
    [string]$ExpectedRestartTitle,
    [string]$ExpectedStoredValue
) {
    $Combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Start-Sleep -Milliseconds 300
    $text = Find-NamedElement $Root $VisibleOption
    if ($null -eq $text) { throw "Language option was not found: $VisibleOption" }
    $item = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($text)
    $selection = $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
    $selection.Select()
    Start-Sleep -Milliseconds 500
    if (-not $selection.Current.IsSelected) { throw "Language option was not selected: $VisibleOption" }
    if ($null -eq (Find-NamedElement $Root $ExpectedRestartTitle)) {
        throw "Localized restart notice was not exposed through UIA: $ExpectedRestartTitle"
    }
    $stored = Get-Content -LiteralPath $preferencePath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($stored.language -ne $ExpectedStoredValue) {
        throw "Stored language '$($stored.language)' does not match '$ExpectedStoredValue'."
    }
}

function Close-Settings($Instance) {
    $Instance.Root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
    if (-not $Instance.Process.WaitForExit(10000)) {
        Stop-Process -Id $Instance.Process.Id -Force
        throw 'Settings did not close within 10 seconds.'
    }
    if ($Instance.Process.ExitCode -ne 0) {
        throw "Settings closed with exit code $($Instance.Process.ExitCode)."
    }
}

try {
    $english = Open-Settings 'InputFlow Settings' 'InputFlow Settings navigation'
    $combo = Open-LanguageSettings $english.Root 'Open Navigation' 'Settings' 'Settings display language'
    Select-Language $english.Root $combo $simplifiedChinese 'Reopen Settings to apply' 'zh-CN'
    Close-Settings $english

    $chinese = Open-Settings $windowTitleChinese $navigationChinese
    $combo = Open-LanguageSettings $chinese.Root $openNavigationChinese $settingsChinese $languageComboChinese
    Select-Language $chinese.Root $combo 'English' $restartChinese 'en-US'
    Close-Settings $chinese

    $forcedEnglish = Open-Settings 'InputFlow Settings' 'InputFlow Settings navigation'
    $combo = Open-LanguageSettings $forcedEnglish.Root 'Open Navigation' 'Settings' 'Settings display language'
    Select-Language $forcedEnglish.Root $combo $systemOption 'Reopen Settings to apply' 'system'
    Close-Settings $forcedEnglish

    $systemEnglish = Open-Settings 'InputFlow Settings' 'InputFlow Settings navigation'
    Close-Settings $systemEnglish

    [ordered]@{
        english_to_chinese = 'PASS'
        chinese_uia = 'PASS'
        chinese_to_english = 'PASS'
        english_uia = 'PASS'
        return_to_system = 'PASS'
        agent_required = $false
    } | ConvertTo-Json
}
finally {
    foreach ($startedProcess in $startedProcesses) {
        try {
            if (-not $startedProcess.HasExited) {
                $startedProcess.Kill()
                [void]$startedProcess.WaitForExit(5000)
            }
        }
        catch [InvalidOperationException] {
            # The exact process started by this script already exited.
        }
        finally {
            $startedProcess.Dispose()
        }
    }
    if ($preferenceExisted) {
        [System.IO.File]::WriteAllBytes($preferencePath, $preferenceBackup)
    }
    elseif (Test-Path -LiteralPath $preferencePath -PathType Leaf) {
        [System.IO.File]::Delete($preferencePath)
    }
}
