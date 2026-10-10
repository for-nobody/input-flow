[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$settingsRoot = Join-Path $repositoryRoot 'apps\settings-winui\InputFlow.Settings'
$resourcePaths = @(
    Join-Path $settingsRoot 'Strings\en-US\Resources.resw'
    Join-Path $settingsRoot 'Strings\zh-CN\Resources.resw'
)

$resourceSets = @()
foreach ($resourcePath in $resourcePaths) {
    [xml]$document = Get-Content -LiteralPath $resourcePath -Raw -Encoding UTF8
    $entries = @($document.root.data)
    $keys = @($entries | ForEach-Object { [string]$_.name })
    $duplicateKeys = @($keys | Group-Object | Where-Object Count -gt 1 | ForEach-Object Name)
    if ($duplicateKeys.Count -ne 0) {
        throw "Duplicate localization keys in $resourcePath`: $($duplicateKeys -join ', ')"
    }
    $emptyKeys = @($entries | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.value) } | ForEach-Object { [string]$_.name })
    if ($emptyKeys.Count -ne 0) {
        throw "Empty localization values in $resourcePath`: $($emptyKeys -join ', ')"
    }
    $resourceSets += ,@($keys | Sort-Object)
}

$differences = @(Compare-Object $resourceSets[0] $resourceSets[1])
if ($differences.Count -ne 0) {
    throw "The en-US and zh-CN resource key sets differ: $($differences.InputObject -join ', ')"
}

$resourceKeys = $resourceSets[0]
$xamlFiles = @(Get-ChildItem -LiteralPath $settingsRoot -Filter '*.xaml' -File -Recurse |
    Where-Object FullName -NotMatch '\\(obj|bin)\\')
$uids = @()
foreach ($xamlFile in $xamlFiles) {
    $content = Get-Content -LiteralPath $xamlFile.FullName -Raw -Encoding UTF8
    foreach ($match in [regex]::Matches($content, 'x:Uid="([A-Za-z0-9_]+)"')) {
        $uids += $match.Groups[1].Value
    }
    $hardcodedAttributes = @([regex]::Matches(
        $content,
        '(?:Content|Text|Header|Label|Title|PlaceholderText|AutomationProperties\.(?:Name|HelpText))="(?!\{)[^\"]+"'))
    if ($hardcodedAttributes.Count -ne 0) {
        throw "Hard-coded localizable XAML text remains in $($xamlFile.FullName): $($hardcodedAttributes.Value -join ', ')"
    }
}

foreach ($uid in @($uids | Sort-Object -Unique)) {
    if (@($resourceKeys | Where-Object { $_.StartsWith("$uid.", [StringComparison]::Ordinal) }).Count -eq 0) {
        throw "x:Uid '$uid' has no resource entry."
    }
}

$sourceFiles = @(Get-ChildItem -LiteralPath $settingsRoot -Filter '*.cs' -File -Recurse |
    Where-Object { $_.Name -ne 'LocalizationSmoke.cs' -and $_.FullName -notmatch '\\(obj|bin)\\' })
$dynamicKeys = @()
foreach ($sourceFile in $sourceFiles) {
    $content = Get-Content -LiteralPath $sourceFile.FullName -Raw -Encoding UTF8
    foreach ($match in [regex]::Matches($content, 'AppResources\.(?:Get|Format)\("([^"]+)"')) {
        $dynamicKeys += $match.Groups[1].Value
    }
}
$missingDynamicKeys = @($dynamicKeys | Sort-Object -Unique | Where-Object { $resourceKeys -notcontains $_ })
if ($missingDynamicKeys.Count -ne 0) {
    throw "Dynamic localization keys are missing: $($missingDynamicKeys -join ', ')"
}

$invalidAutomationKeys = @($resourceKeys | Where-Object {
    $_ -match 'AutomationProperties\.' -and
    $_ -notmatch '\[using:Microsoft\.UI\.Xaml\.Automation\]AutomationProperties\.'
})
if ($invalidAutomationKeys.Count -ne 0) {
    throw "Automation property resources use an invalid namespace: $($invalidAutomationKeys -join ', ')"
}

Write-Host "Localization contract passed: $($resourceKeys.Count) keys, $(@($uids | Sort-Object -Unique).Count) x:Uid references."
